#!/usr/bin/env python3
"""Verify preserved Office SVG evidence using only Python's standard library."""

from __future__ import annotations

import argparse
import copy
import hashlib
import io
import json
import math
import posixpath
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
SVG = "http://www.w3.org/2000/svg"
A = "http://schemas.openxmlformats.org/drawingml/2006/main"
WP = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
EXTENT = (1104900, 368300)
BASELINE = "493a0cb49f79f76ae2ee0716a690c20ce3981f15f701c0a9651552addaa8bf95"
INITIAL = "a526729562d646abb83038001f9a51041329ebb6ec768e72f634b8e99ec8b5a9"
FINAL = "3d343a93e4cf5f3977252804f85eb419e809132eb9ac0a245f4d7c05d2fabbe7"
SOURCE = "3d261b32e9baa73b6d2cb133de2163fa3699569a"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def load(name):
    return json.loads((HERE / name).read_text(encoding="utf-8"))


def local(node):
    return node.tag.rsplit("}", 1)[-1]


def svg_facts(data):
    root = ET.fromstring(data)
    require(root.tag == f"{{{SVG}}}svg", "SVG root required")
    return {
        "root": root,
        "text": sum(local(n) == "text" for n in root.iter()),
        "paths": sum(local(n) == "path" for n in root.iter()),
        "baselines": sum("dominant-baseline" in n.attrib for n in root.iter()),
    }


def point_size(root):
    dimensions = []
    for name in ("width", "height"):
        value = root.attrib[name]
        require(value.endswith("pt"), "Source SVG needs physical point dimensions")
        value = float(value[:-2])
        require(math.isfinite(value) and value > 0, "Invalid physical dimension")
        dimensions.append(value)
    return tuple(dimensions)


def check_svg_pair(ordinary, outlined):
    before, after = svg_facts(ordinary), svg_facts(outlined)
    require(before["text"] == before["baselines"] == 3, "Ordinary text control changed")
    require(after["text"] == 0 and after["paths"] == 5, "Office labels must be outlines")
    old_size, new_size = point_size(before["root"]), point_size(after["root"])
    require(all(abs(a - b) < 0.001 for a, b in zip(old_size, new_size)), "SVG extent changed")
    return new_size


def check_native(doc):
    require(doc["version"] == 19, "Controlled native input version changed")
    atoms = {a["id"]: a for a in doc["atoms"]}
    require(len(atoms) == len(doc["atoms"]) == 6 and len(doc["bonds"]) == 5, "Native graph size")
    require(
        sorted(a["element"] for a in atoms.values()) == ["C", "C", "C", "C", "O", "Si"],
        "Native elements",
    )
    require(all(a["charge"] == 0 for a in atoms.values()), "Native charge changed")
    adjacent = {i: set() for i in atoms}
    edges = set()
    for bond in doc["bonds"]:
        a, b = bond["a"], bond["b"]
        require(a in atoms and b in atoms and a != b and bond["order"] == 1, "Native bond")
        pair = tuple(sorted((a, b)))
        require(pair not in edges, "Duplicate bond")
        edges.add(pair)
        adjacent[a].add(b)
        adjacent[b].add(a)
    silicon = next(i for i, a in atoms.items() if a["element"] == "Si")
    oxygen = next(i for i, a in atoms.items() if a["element"] == "O")
    require(len(adjacent[silicon]) == 4 and oxygen in adjacent[silicon], "Silicon attachment")
    require(
        len(adjacent[oxygen]) == 2
        and all(len(adjacent[i]) == 1 for i, a in atoms.items() if a["element"] == "C"),
        "O/C attachment",
    )
    require(len(doc["abbreviations"]) == 1, "Native abbreviation count")
    abbreviation = doc["abbreviations"][0]
    require(abbreviation["label"] == "SiMe3" and abbreviation["anchor"] == silicon, "SiMe3 anchor")
    require(
        set(abbreviation["members"]) == {silicon, *(adjacent[silicon] - {oxygen})},
        "SiMe3 membership",
    )
    require(
        abbreviation["label_style"]["family"] == "Arial"
        and abbreviation["label_style"]["size_pt"] == 10,
        "Label style changed",
    )


def check_geometry(root, kind):
    pictures = [n for n in root.iter() if local(n) == "pic"]
    require(len(pictures) == 1, "One Office picture required")
    require(not any(local(n) == "srcRect" for n in root.iter()), "Unexpected Office crop")
    transform = pictures[0].find(f".//{{{A}}}xfrm")
    require(transform is not None, "Picture transform missing")
    require(
        all(transform.get(k, "0") in ("0", "false") for k in ("rot", "flipH", "flipV")),
        "Unexpected rotation/reflection",
    )
    extent = transform.find(f"{{{A}}}ext")
    require(
        extent is not None and tuple(int(extent.get(k)) for k in ("cx", "cy")) == EXTENT,
        "Office extent changed",
    )
    offset = transform.find(f"{{{A}}}off")
    require(offset is not None, "Picture offset missing")
    offset = tuple(int(offset.get(k)) for k in ("x", "y"))
    require(offset == ((0, 0) if kind == "word" else (5543550, 3244850)), "Office picture moved")
    if kind == "word":
        inline = root.findall(f".//{{{WP}}}inline")
        require(len(inline) == 1, "One Word inline picture required")
        extent = inline[0].find(f"{{{WP}}}extent")
        require(
            extent is not None and tuple(int(extent.get(k)) for k in ("cx", "cy")) == EXTENT,
            "Word inline extent changed",
        )
    return pictures[0]


def office_facts(data, kind, outlined):
    member = "word/document.xml" if kind == "word" else "ppt/slides/slide1.xml"
    relations = (
        "word/_rels/document.xml.rels" if kind == "word" else "ppt/slides/_rels/slide1.xml.rels"
    )
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        root = ET.fromstring(archive.read(member))
        picture = check_geometry(root, kind)
        vectors = [n for n in picture.iter() if local(n) == "svgBlip"]
        require(len(vectors) == 1, "Office picture SVG reference missing")
        relationship = vectors[0].attrib[f"{{{R}}}embed"]
        relationships = ET.fromstring(archive.read(relations))
        targets = [n.attrib["Target"] for n in relationships if n.attrib["Id"] == relationship]
        require(len(targets) == 1, "SVG relationship missing")
        entry = posixpath.normpath(posixpath.join(posixpath.dirname(member), targets[0]))
        svg_entries = [n for n in archive.namelist() if n.endswith(".svg")]
        require(svg_entries == [entry], "Unreferenced or ambiguous SVG payload")
        payload = archive.read(entry)
    facts = svg_facts(payload)
    require(facts["baselines"] == 0, "Saved Office SVG baseline control changed")
    require(facts["text"] == (0 if outlined else 3), "Saved SVG text/outlines changed")
    require(facts["paths"] == (5 if outlined else 1), "Saved SVG vector paths changed")
    expected = (
        "8fd43b86eaa7e03d23e5408efc220a962fd91fc2ad93af0a3ec255e87da2d359"
        if outlined
        else "c57ec65d4c2b5cab105d95481ad0f1ed2ed6e89705645b5e45393578fc7edc56"
    )
    require(sha(payload) == expected, "Saved Office SVG payload changed")
    return {"root": root, "entry": entry, "sha256": sha(payload)}


def check_producers(receipt):
    require(
        receipt["source_head"] == SOURCE and receipt["signed_executable_sha256"] == FINAL,
        "Final source/app identity",
    )
    require(
        receipt["office_actual_producer_signed_sha256"] == INITIAL,
        "Earlier Office screenshots must keep their actual producer",
    )
    require(
        receipt["final_office_svg_identical_to_actual_office_inserted_svg"],
        "Final export bridge missing",
    )
    require(
        receipt["native_drawing_byte_exact"] and receipt["undo_redo_disabled"],
        "Native export/history control",
    )


def check_sources(source_root=None):
    initial = load("initial-default-build-source-provenance.json")
    final = load("final-default-build-source-provenance.json")
    require(initial["all_local_artifacts_fresh_false"], "Initial own-source rebuild missing")
    require(
        final["production_compiler_artifact_fresh"] is False
        and not final["foreign_worktree_inputs"],
        "Final fresh own app proof",
    )
    old, new = initial["input_sha256"], final["input_sha256"]
    require(
        len(old) == len(new) == 533 and old.keys() == new.keys(),
        "Compiled input dictionary changed",
    )
    require(
        {p for p in old if old[p] != new[p]} == {"src/app/inspector.rs"},
        "Exporter changed after Office producer",
    )
    old_rust = load("initial-source-mtime-receipt.json")["before"]
    new_rust = load("final-source-proof.json")["input_sha256"]
    require(
        len(old_rust) == len(new_rust) == 1061 and old_rust.keys() == new_rust.keys(),
        "Rust/manifest source set",
    )
    require(
        {p for p in old_rust if old_rust[p] != new_rust[p]}
        == {"src/app/inspector.rs", "src/app/inspector/tests.rs"},
        "Label/test correction scope",
    )
    require(
        load("initial-bundle-provenance.json")["signed_executable_sha256"] == INITIAL
        and load("final-bundle-provenance.json")["signed_executable_sha256"] == FINAL,
        "Bundle identities",
    )
    if source_root is not None:
        for mapping in (new, new_rust):
            for path, digest in mapping.items():
                require(
                    sha((source_root / path).read_bytes()) == digest,
                    f"Source differs from preserved build: {path}",
                )


def negative_controls(ordinary, outlined, native, word_root, receipt):
    results = []

    def rejected(name, operation):
        try:
            operation()
        except ValueError:
            results.append(name)
        else:
            raise ValueError(f"Negative control was accepted: {name}")

    bad_svg = ET.fromstring(outlined)
    ET.SubElement(bad_svg, f"{{{SVG}}}text").text = "SiMe3"
    rejected("text accidentally retained", lambda: check_svg_pair(ordinary, ET.tostring(bad_svg)))
    bad_svg = ET.fromstring(outlined)
    bad_svg.set("width", "175.18264pt")
    rejected("SVG physical width doubled", lambda: check_svg_pair(ordinary, ET.tostring(bad_svg)))
    bad_native = copy.deepcopy(native)
    bad_native["bonds"][0]["order"] = 2
    rejected("chemical bond changed", lambda: check_native(bad_native))
    bad_native = copy.deepcopy(native)
    bad_native["abbreviations"][0]["anchor"] = next(
        a["id"] for a in native["atoms"] if a["element"] == "O"
    )
    rejected("SiMe3 anchor moved to oxygen", lambda: check_native(bad_native))
    bad_office = copy.deepcopy(word_root)
    bad_office.find(f".//{{{WP}}}extent").set("cx", str(EXTENT[0] + 1))
    rejected("Office extent changed by one EMU", lambda: check_geometry(bad_office, "word"))
    bad_office = copy.deepcopy(word_root)
    ET.SubElement(bad_office, f"{{{A}}}srcRect", {"l": "10000"})
    rejected("Office picture cropped", lambda: check_geometry(bad_office, "word"))
    bad_receipt = copy.deepcopy(receipt)
    bad_receipt["office_actual_producer_signed_sha256"] = FINAL
    rejected("earlier screenshots relabeled as final app", lambda: check_producers(bad_receipt))
    return results


def verify(source_root=None):
    manifest = load("evidence-manifest.json")["files"]
    for path, record in manifest.items():
        data = (ROOT / path).read_bytes()
        require(
            len(data) == record["bytes"] and sha(data) == record["sha256"],
            f"Evidence changed: {path}",
        )
    receipt = load("final-candidate-export-receipt.json")
    check_producers(receipt)
    require(
        load("baseline-export-receipt.json")["signed_executable_sha256"] == BASELINE,
        "Historical before producer",
    )
    for a, b in receipt["byte_exact_pairs"]:
        require(
            (HERE / a).read_bytes() == (HERE / b).read_bytes(),
            f"Final identity pair changed: {a}, {b}",
        )
    ordinary = (HERE / "single-baseline-ordinary.svg").read_bytes()
    outlined = (HERE / "single-final-office.svg").read_bytes()
    size = check_svg_pair(ordinary, outlined)
    native = load("single-final-after-export.rsk")
    check_native(native)
    office = {}
    for kind, extension in (("word", "docx"), ("powerpoint", "pptx")):
        office[kind] = office_facts(
            (HERE / f"{kind}-baseline-ordinary.{extension}").read_bytes(), kind, False
        )
        candidate = office_facts(
            (HERE / f"{kind}-candidate-office.{extension}").read_bytes(), kind, True
        )
        require(
            load(f"{kind}-candidate-receipt.json")["candidate_producer_signed_sha256"] == INITIAL,
            "Office capture producer",
        )
        office[kind] = candidate
    selected = (HERE / "final-candidate-single-exported-ax.txt").read_text(encoding="utf-8")
    require(
        "Export SVG · Office picture figure" in selected and "SVG-OFFICE" not in selected,
        "Final accessible export label",
    )
    check_sources(source_root)
    controls = negative_controls(ordinary, outlined, native, office["word"]["root"], receipt)
    authored_mm = [pt * 25.4 / 72 for pt in size]
    office_mm = [emu / 36000 for emu in EXTENT]
    return {
        "status": "PASS",
        "immutable_evidence_files": len(manifest),
        "source_checked": source_root is not None,
        "native": {"version": 19, "atoms": 6, "bonds": 5, "abbreviation": "SiMe3"},
        "source_svg_points": size,
        "source_svg_mm": authored_mm,
        "actual_office_extent_emu": EXTENT,
        "actual_office_mm": office_mm,
        "office_vs_source_percent": [
            (actual / authored - 1) * 100 for actual, authored in zip(office_mm, authored_mm)
        ],
        "negative_controls_rejected": controls,
        "scope": "Artifact/source provenance and recorded properties; does not replay GUI or establish Windows Office acceptance.",
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--source-root",
        type=Path,
        help="Additionally compare the current checkout with the preserved build input hashes",
    )
    args = parser.parse_args()
    print(json.dumps(verify(args.source_root), indent=2))
