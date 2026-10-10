#!/usr/bin/env python3
"""Check preserved Windows Office artifacts; never operate an application."""

from __future__ import annotations

import copy
import hashlib
import io
import json
import math
import posixpath
import re
import struct
import xml.etree.ElementTree as E
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[4]
EXTENT = (1114425, 371475)
SOURCE = "b09bf5ed5d57f298988b7528dd9f776b0169e025"
EXE = "35736c95d4b2db9c264e5b03e36d24cfe70527ce185dc9fc1b170773f4ba1710"
NS = {
    "p": "http://schemas.openxmlformats.org/presentationml/2006/main",
    "a": "http://schemas.openxmlformats.org/drawingml/2006/main",
    "r": "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    "asvg": "http://schemas.microsoft.com/office/drawing/2016/SVG/main",
    "s": "http://www.w3.org/2000/svg",
    "w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
    "wp": "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
    "pic": "http://schemas.openxmlformats.org/drawingml/2006/picture",
}
NUMBER = r"[-+]?(?:\d*\.\d+|\d+\.?\d*)(?:[eE][-+]?\d+)?"
TOKEN = re.compile(r"[MLQCZ]|" + NUMBER)


def require(value, message):
    if not value:
        raise ValueError(message)


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def load(name):
    return json.loads((HERE / name).read_bytes())


def source_pair(ordinary, office):
    before, after = E.fromstring(ordinary), E.fromstring(office)
    require(len(before.findall(".//s:text", NS)) == 3, "Ordinary text control")
    require(
        sum("dominant-baseline" in e.attrib for e in before.iter()) == 3, "Source baseline control"
    )
    require(
        not after.findall(".//s:text", NS) and len(after.findall(".//s:path", NS)) == 5,
        "Outlined labels",
    )
    sizes = []
    for root in [before, after]:
        dimensions = []
        for key in ["width", "height"]:
            value = root.get(key)
            require(value is not None and value.endswith("pt"), "Physical point dimensions")
            value = float(value[:-2])
            require(math.isfinite(value) and value > 0, "Positive finite dimension")
            dimensions.append(value)
        sizes.append(dimensions)
    require(all(abs(a - b) < 0.0001 for a, b in zip(*sizes)), "Figure size agreement")
    return before, after


def segments(data):
    """Normalize the limited absolute SVG M/L/Q/C/Z syntax actually present."""
    tokens = TOKEN.findall(data)
    assert re.sub(r"[\s,]", "", TOKEN.sub("", data)) == ""
    out = []
    index = 0
    current = (0.0, 0.0)
    origin = current
    command = None
    while index < len(tokens):
        if tokens[index] in ["M", "L", "Q", "C", "Z"]:
            command = tokens[index]
            index += 1
        assert command is not None
        if command == "Z":
            out.append(("Z", ()))
            current = origin
            command = None
            continue
        size = {"M": 2, "L": 2, "Q": 4, "C": 6}[command]
        values = tuple(float(v) for v in tokens[index : index + size])
        assert len(values) == size and all(math.isfinite(v) for v in values)
        index += size
        if command == "Q":
            q, end = values[:2], values[2:]
            first = tuple(current[i] + (q[i] - current[i]) * 2 / 3 for i in [0, 1])
            second = tuple(end[i] + (q[i] - end[i]) * 2 / 3 for i in [0, 1])
            out.append(("C", first + second + end))
            current = end
        else:
            out.append((command, values))
            current = values[-2:]
        if command == "M":
            origin = current
            command = "L"
    return out


def path_geometry(root):
    out = []

    def walk(node, matrix=(1.0, 0.0, 0.0, 1.0, 0.0, 0.0)):
        if node.get("transform"):
            match = re.fullmatch(r"matrix\(([^)]+)\)", node.get("transform"))
            assert match and matrix == (1.0, 0.0, 0.0, 1.0, 0.0, 0.0)
            matrix = tuple(float(v) for v in re.findall(NUMBER, match.group(1)))
            assert len(matrix) == 6
        if node.tag == "{" + NS["s"] + "}path":
            normalized = []
            for command, points in segments(node.get("d")):
                transformed = []
                for index in range(0, len(points), 2):
                    x, y = points[index : index + 2]
                    a, b, c, d, e, f = matrix
                    transformed.extend((a * x + c * y + e, b * x + d * y + f))
                normalized.append((command, tuple(transformed)))
            out.append(normalized)
        for child in node:
            walk(child, matrix)

    walk(root)
    return out


def compare_geometry(source, stored, tolerance=0.001):
    lhs, rhs = path_geometry(source), path_geometry(stored)
    assert len(lhs) == len(rhs) == 5
    delta = 0.0
    counts = []
    for first, second in zip(lhs, rhs):
        assert len(first) == len(second)
        counts.append(len(first))
        for (kind1, points1), (kind2, points2) in zip(first, second):
            assert kind1 == kind2 and len(points1) == len(points2)
            delta = (
                max(delta, *(abs(a - b) for a, b in zip(points1, points2))) if points1 else delta
            )
    assert delta <= tolerance, delta
    return {
        "path_count": len(lhs),
        "segments_per_path": counts,
        "maximum_transformed_control_coordinate_delta_svg_units": delta,
        "tolerance_svg_units": tolerance,
        "note": "Q segments converted mathematically to C before comparison; Office rounded coordinates. This is bounded vector geometry agreement, not byte identity or pixel-render equivalence.",
    }


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


def native_facts(doc):
    check_native(doc)
    require(
        all(math.isfinite(a["position"][k]) for a in doc["atoms"] for k in ["x", "y"]),
        "Finite native positions",
    )
    require(all("z" not in a["position"] for a in doc["atoms"]), "No invented depth fields")


def svg_facts(raw, outlined, source):
    root = E.fromstring(raw)
    require(root.tag == "{" + NS["s"] + "}svg", "SVG required")
    texts = ["".join(e.itertext()) for e in root.findall(".//s:text", NS)]
    require(texts == ([] if outlined else ["O", "SiMe", "3"]), "Saved label encoding")
    require(len(root.findall(".//s:path", NS)) == (5 if outlined else 1), "Saved vector paths")
    require(not any("dominant-baseline" in e.attrib for e in root.iter()), "Saved baseline removal")
    require(not root.findall(".//s:image", NS), "No raster inside SVG")
    require(
        not [v for e in root.iter() for k, v in e.attrib.items() if k.endswith("href")],
        "No external SVG reference",
    )
    return compare_geometry(source, root) if outlined else None


def dimensions(picture, kind):
    require(not picture.findall(".//a:srcRect", NS), "No picture crop")
    transform = picture.find(".//a:xfrm", NS)
    require(transform is not None, "Picture transform")
    require(
        all(transform.get(k, "0") in ["0", "false"] for k in ["rot", "flipH", "flipV"]),
        "No rotation/reflection",
    )
    extent = transform.find("a:ext", NS)
    require(
        extent is not None and tuple(int(extent.get(k)) for k in ["cx", "cy"]) == EXTENT,
        "Saved picture extent",
    )
    offset = transform.find("a:off", NS)
    expected = (5538787, 3243262) if kind == "powerpoint" else (0, 0)
    require(
        offset is not None and tuple(int(offset.get(k)) for k in ["x", "y"]) == expected,
        "Picture offset",
    )


def resolve(relmap, relation, member, archive):
    require(relation in relmap, "Missing relation")
    item = relmap[relation]
    require(
        item.get("TargetMode") is None and item.get("Type").endswith("/image"),
        "Local image relationship",
    )
    target = posixpath.normpath(posixpath.join(posixpath.dirname(member), item.get("Target")))
    require(target in archive.namelist(), "Missing image payload")
    return target, archive.read(target)


def png_facts(raw):
    require(raw.startswith(b"\x89PNG\r\n\x1a\n"), "PNG fallback required")
    require(raw[12:16] == b"IHDR", "PNG dimensions required")
    width, height, bits, color, _, _, interlace = struct.unpack(">IIBBBBB", raw[16:29])
    require(
        (width, height, bits, color, interlace) == (234, 78, 8, 6, 0),
        "Fallback dimensions/encoding",
    )
    return {"bytes": len(raw), "sha256": digest(raw), "pixels": [width, height]}


def office_facts(raw, kind, source):
    facts = []
    with zipfile.ZipFile(io.BytesIO(raw)) as archive:
        if kind == "powerpoint":
            presentation = E.fromstring(archive.read("ppt/presentation.xml"))
            prels = {
                r.get("Id"): r.get("Target")
                for r in E.fromstring(archive.read("ppt/_rels/presentation.xml.rels"))
            }
            ids = presentation.findall("./p:sldIdLst/p:sldId", NS)
            require(len(ids) == 2, "Two controlled slides")
            members = [
                posixpath.normpath("ppt/" + prels[s.get("{" + NS["r"] + "}id")]) for s in ids
            ]
            media = [n for n in archive.namelist() if n.startswith("ppt/media/")]
            require(
                len(media) == 2 and all(n.endswith(".svg") for n in media),
                "PPT SVG-only picture media",
            )
        else:
            members = ["word/document.xml"]
            media = [n for n in archive.namelist() if n.startswith("word/media/")]
            require(
                len(media) == 4 and sum(n.endswith(".svg") for n in media) == 2,
                "Word separate vectors and fallback media",
            )
        for member in members:
            root = E.fromstring(archive.read(member))
            relfile = posixpath.dirname(member) + "/_rels/" + posixpath.basename(member) + ".rels"
            relations = list(E.fromstring(archive.read(relfile)))
            relmap = {r.get("Id"): r for r in relations}
            if kind == "powerpoint":
                pictures = root.findall("./p:cSld/p:spTree/p:pic", NS)
                require(len(pictures) == 1, "One picture per slide")
                layout = [r for r in relations if r.get("Type").endswith("/slideLayout")]
                require(len(layout) == 1, "Slide layout relationship")
                layout_path = posixpath.normpath(
                    posixpath.dirname(member) + "/" + layout[0].get("Target")
                )
                require(
                    E.fromstring(archive.read(layout_path)).get("type") == "blank",
                    "Controlled blank layout",
                )
                inline = None
            else:
                inline = root.findall("./w:body/w:p/w:r/w:drawing/wp:inline", NS)
                require(
                    len(inline) == 2 and not root.findall(".//wp:anchor", NS),
                    "Two Word inline pictures",
                )
                pictures = [b.find(".//pic:pic", NS) for b in inline]
            for picture in pictures:
                index = len(facts)
                dimensions(picture, kind)
                if inline is not None:
                    extent = inline[index].find("wp:extent", NS)
                    require(
                        tuple(int(extent.get(k)) for k in ["cx", "cy"]) == EXTENT,
                        "Word inline extent",
                    )
                vectors = picture.findall(".//asvg:svgBlip", NS)
                require(len(vectors) == 1, "One SVG relation per picture")
                svg_member, svg = resolve(
                    relmap, vectors[0].get("{" + NS["r"] + "}embed"), member, archive
                )
                bridge = svg_facts(svg, index == 1, source)
                blip = picture.find(".//a:blip", NS)
                fallback_id = blip.get("{" + NS["r"] + "}embed")
                if kind == "word":
                    fallback_member, fallback = resolve(relmap, fallback_id, member, archive)
                    bitmap = {"member": fallback_member, **png_facts(fallback)}
                else:
                    require(fallback_id is None, "PPT has no raster fallback")
                    bitmap = None
                facts.append(
                    {
                        "kind": "office" if index == 1 else "ordinary",
                        "member": member,
                        "svg_member": svg_member,
                        "svg_sha256": digest(svg),
                        "geometry_bridge": bridge,
                        "fallback": bitmap,
                    }
                )
    require(len(facts) == 2, "Two logical pictures")
    return facts


def source_identity(receipt, build, qualification):
    require(
        receipt["native_producer_source"] == SOURCE
        and receipt["qualified_executable_sha256"] == EXE,
        "Native source/executable identity",
    )
    require(receipt["qualified_executable_signed"] is False, "No invented signing claim")
    require(
        build["head_sha"] == SOURCE and build["exe"]["sha256"] == EXE and build["signed"] is False,
        "Build identity",
    )
    require(
        build["optional_features"] == [] and build["source_bytes_unchanged"],
        "Default features/source freeze",
    )
    require(
        len(build["own_compiler_artifacts"]) == 15
        and all(a["fresh"] is False for a in build["own_compiler_artifacts"]),
        "Fresh own compiler artifacts",
    )
    require(
        qualification["head"] == SOURCE
        and qualification["job_conclusion"] == "success"
        and qualification["exe_sha256"] == EXE,
        "Successful exact-head artifact",
    )


def mutate_zip(raw, member, before, after):
    result = io.BytesIO()
    with zipfile.ZipFile(io.BytesIO(raw)) as original, zipfile.ZipFile(result, "w") as modified:
        for info in original.infolist():
            value = original.read(info.filename)
            if info.filename == member:
                require(before in value, "Control actually mutates a semantic value")
                value = value.replace(before, after, 1)
            modified.writestr(info, value)
    return result.getvalue()


def negative_controls(
    ordinary, office, native, ppt, word, source, provenance, build, qualification
):
    passed = []

    def reject(label, call):
        try:
            call()
        except (ValueError, AssertionError):
            passed.append(label)
        else:
            raise ValueError("Corruption accepted: " + label)

    changed = office.replace(
        b"<defs/>", b'<text xmlns="http://www.w3.org/2000/svg">O</text><defs/>', 1
    )
    reject("retained Office text", lambda: source_pair(ordinary, changed))
    changed = office.replace(b'width="87.59132pt"', b'width="175.18264pt"', 1)
    reject("doubled authored width", lambda: source_pair(ordinary, changed))
    doc = copy.deepcopy(native)
    doc["bonds"][0]["order"] = 2
    reject("wrong chemical bond", lambda: native_facts(doc))
    doc = copy.deepcopy(native)
    doc["abbreviations"][0]["anchor"] = next(a["id"] for a in doc["atoms"] if a["element"] == "O")
    reject("wrong abbreviation anchor", lambda: native_facts(doc))
    doc = copy.deepcopy(native)
    doc["atoms"][0]["position"]["x"] = float("inf")
    reject("nonfinite native position", lambda: native_facts(doc))
    bad = mutate_zip(ppt, "ppt/slides/slide2.xml", b'r:embed="rId2"', b'r:embed="rId1"')
    reject("wrong PPT vector relationship", lambda: office_facts(bad, "powerpoint", source))
    bad = mutate_zip(ppt, "ppt/slides/slide1.xml", b'cx="1114425"', b'cx="1114426"')
    reject("one-EMU PPT width change", lambda: office_facts(bad, "powerpoint", source))
    bad = mutate_zip(ppt, "ppt/media/image2.svg", b"M206.354", b"M216.354")
    reject("changed saved Office vector geometry", lambda: office_facts(bad, "powerpoint", source))
    bad = mutate_zip(word, "word/document.xml", b'r:embed="rId6"', b'r:embed="rId7"')
    reject("wrong Word fallback relationship", lambda: office_facts(bad, "word", source))
    bad = mutate_zip(word, "word/document.xml", b'cy="371475"', b'cy="371476"')
    reject("one-EMU Word height change", lambda: office_facts(bad, "word", source))
    changed = copy.deepcopy(provenance)
    changed["qualified_executable_signed"] = True
    reject("invented signing status", lambda: source_identity(changed, build, qualification))
    return passed


def main():
    manifest = load("evidence-manifest.json")
    for row in manifest["files"]:
        data = (ROOT / row["path"]).read_bytes()
        require(
            len(data) == row["bytes"] and digest(data) == row["sha256"],
            "Artifact bytes changed: " + row["path"],
        )
    ordinary = (HERE / "windows-single-ordinary-v1.svg").read_bytes()
    office = (HERE / "windows-single-office-v1.svg").read_bytes()
    _, source = source_pair(ordinary, office)
    native = load("windows-native-input.rsk")
    native_facts(native)
    ppt = (HERE / "windows-office-svg-comparison-v1.pptx").read_bytes()
    word = (HERE / "windows-office-svg-comparison-v1.docx").read_bytes()
    powerpoint, document = (
        office_facts(ppt, "powerpoint", source),
        office_facts(word, "word", source),
    )
    require(
        [f["svg_sha256"] for f in powerpoint] == [f["svg_sha256"] for f in document],
        "Word/PPT saved SVG identity",
    )
    provenance = load("provenance.json")
    build = load("candidate-build-summary.json")["facts"]
    qualification = load("candidate-artifact-qualification.json")["facts"]
    source_identity(provenance, build, qualification)
    controls = negative_controls(
        ordinary, office, native, ppt, word, source, provenance, build, qualification
    )
    print(
        json.dumps(
            {
                "result": "PASS",
                "exact_files": len(manifest["files"]),
                "native": "version19, six atoms/five single bonds, neutral SiMe3 abbreviation",
                "extent_emu": EXTENT,
                "saved_svg_source_byte_identity": False,
                "word_fallback_pixels": [234, 78],
                "negative_controls": controls,
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
