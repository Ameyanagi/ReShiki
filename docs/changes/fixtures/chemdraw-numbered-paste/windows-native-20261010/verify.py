"""Verify the preserved controlled evidence with Python's standard library."""

import copy
import hashlib
import io
import json
import struct
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent
HEAD = "127d1a6247d390d33d420f2168f4a657d2659f7b"
EXE = "3e56439b55cd3c7dc047f431bb56f60407d17bc56fae0e1cb76641f1ee2e6fb9"
NS = {
    "p": "http://schemas.openxmlformats.org/presentationml/2006/main",
    "r": "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
}


def cdx_objects(data):
    assert data[:12] == b"VjCD0100\x04\x03\x02\x01"
    position = 22 if data[22:24] == b"\x00\x80" else 28
    objects = []

    def read(ancestors=()):
        nonlocal position
        assert len(ancestors) < 32 and len(objects) < 1024
        assert position + 6 <= len(data)
        code, identifier = struct.unpack_from("<HI", data, position)
        position += 6
        assert code >= 0x8000
        obj = {"code": code, "id": identifier, "ancestors": ancestors, "properties": []}
        objects.append(obj)
        while True:
            assert position + 2 <= len(data)
            tag = struct.unpack_from("<H", data, position)[0]
            if tag == 0:
                position += 2
                break
            if tag >= 0x8000:
                read((*ancestors, (code, identifier)))
                continue
            offset = position
            assert position + 4 <= len(data)
            tag, size = struct.unpack_from("<HH", data, position)
            position += 4
            if size == 65535:
                assert position + 4 <= len(data)
                size = struct.unpack_from("<I", data, position)[0]
                position += 4
            assert position + size <= len(data)
            obj["properties"].append((tag, data[position : position + size], offset, position))
            position += size

    read()
    assert data[position:] in (b"", b"\x00\x00")
    return objects


def validate_port(data):
    objects = cdx_objects(data)
    ports = [o for o in objects if any(p[0] == 0x044B for p in o["properties"])]
    assert len(ports) == 1
    port = ports[0]
    assert port["code"] == 0x8004 and port["id"] == 15
    assert port["ancestors"][-2][0:2] == (0x8004, 5)
    numbers = [p for p in port["properties"] if p[0] == 0x044B]
    types = [p for p in port["properties"] if p[0] == 0x0400]
    assert len(numbers) == 1 and len(types) == 1
    assert len(numbers[0][1]) == 1 and struct.unpack("<b", numbers[0][1])[0] > 0
    assert types[0][1] == b"\x0c\x00"
    return numbers[0], types[0]


def chemical_atoms(document):
    fields = (
        "id",
        "element",
        "charge",
        "radical_electrons",
        "isotope",
        "explicit_h",
        "no_implicit",
        "aromatic",
        "stereo",
        "map_num",
        "label_h",
    )
    return [{key: atom[key] for key in fields} for atom in document["atoms"]]


def validate_native(document, reference):
    assert document["version"] == 19
    assert len(document["atoms"]) == 6 and len(document["bonds"]) == 5
    assert chemical_atoms(document) == chemical_atoms(reference)
    assert document["bonds"] == reference["bonds"]
    assert document["abbreviations"] == reference["abbreviations"]
    assert document["atom_labels"] == reference["atom_labels"]
    assert not any(
        document.get(key) for key in ("annotations", "arrows", "graphics", "groups", "reactions")
    )
    shift = {
        key: document["atoms"][0]["position"][key] - reference["atoms"][0]["position"][key]
        for key in ("x", "y")
    }
    for actual, expected in zip(document["atoms"], reference["atoms"]):
        assert actual.get("depth", 0) == 0
        for key in ("x", "y"):
            assert abs(actual["position"][key] - expected["position"][key] - shift[key]) < 0.0001


def validate_slide(slide_data, relationship_data):
    slide = ET.fromstring(slide_data)
    relationships = ET.fromstring(relationship_data)
    assert len(slide.findall(".//p:graphicFrame", NS)) == 1
    objects = slide.findall(".//p:oleObj", NS)
    assert len(objects) == 2  # Choice and fallback for one logical object.
    assert all(o.attrib["progId"] == "ChemDraw_x64.Document.6.0" for o in objects)
    assert all(o.attrib["name"] == "CS ChemDraw 64-bit Drawing" for o in objects)
    assert {o.attrib["{" + NS["r"] + "}id"] for o in objects} == {"rId2"}
    relationship = next(r for r in relationships if r.attrib["Id"] == "rId2")
    assert relationship.attrib["Type"] == NS["r"] + "/oleObject"
    assert relationship.attrib["Target"] == "../embeddings/oleObject1.bin"
    assert relationship.attrib.get("TargetMode", "Internal") == "Internal"


def main():
    manifest = json.loads((ROOT / "evidence-manifest.json").read_bytes())
    for row in manifest["files"]:
        data = (ROOT / row["path"]).read_bytes()
        assert len(data) == row["bytes"]
        assert hashlib.sha256(data).hexdigest() == row["sha256"]
    direct_cdx = (ROOT / "windows-si-single-v1.cdx").read_bytes()
    embedded_cdx = (ROOT / "powerpoint-ole-CONTENTS.cdx").read_bytes()
    validate_port(direct_cdx)
    number, node_type = validate_port(embedded_cdx)
    reference = json.loads((ROOT.parent / "single-desktop.rsk").read_bytes())
    direct = (ROOT / "windows-si-clipboard-after-v1.rsk").read_bytes()
    powerpoint = (ROOT / "windows-si-powerpoint-paste-after-v1.rsk").read_bytes()
    assert direct == powerpoint
    native = json.loads(powerpoint)
    validate_native(native, reference)
    with zipfile.ZipFile(
        io.BytesIO((ROOT / "windows-si-powerpoint-v1.pptx").read_bytes())
    ) as archive:
        slide = archive.read("ppt/slides/slide1.xml")
        relationships = archive.read("ppt/slides/_rels/slide1.xml.rels")
        validate_slide(slide, relationships)
        embedding = archive.read("ppt/embeddings/oleObject1.bin")
        assert (
            hashlib.sha256(embedding).hexdigest()
            == "65b5a68c52d103c4a55a48aefbc5bfe03df6d1c094680d8077d735e30b96559c"
        )
    qualification = json.loads((ROOT / "candidate-artifact-qualification.json").read_bytes())
    assert qualification["head"] == HEAD and qualification["exe_sha256"] == EXE
    assert qualification["job_conclusion"] == "success"
    assert qualification["all_inventory_bytes_match_exact_git_objects"]
    build = json.loads((ROOT / "candidate-build-summary.json").read_bytes())
    assert build["head_sha"] == HEAD and build["exe"]["sha256"] == EXE
    assert build["qualified"] and build["source_bytes_unchanged"]
    assert build["tracked_source_count"] == 2581
    assert len(build["own_compiler_artifacts"]) == 15
    assert all(not a["fresh"] for a in build["own_compiler_artifacts"])
    parity = json.loads((ROOT / "baseline-source-parity.json").read_bytes())
    assert len(parity["rows"]) == 10 and all(r["equal"] for r in parity["rows"])

    controls = []

    def reject(name, operation):
        try:
            operation()
        except AssertionError:
            controls.append(name)
        else:
            raise AssertionError("Corruption accepted: " + name)

    for value, name in [(0, "zero connection number"), (128, "negative signed connection number")]:
        changed = bytearray(embedded_cdx)
        changed[number[3]] = value
        reject(name, lambda changed=changed: validate_port(bytes(changed)))
    changed = bytearray(embedded_cdx)
    changed[node_type[3] : node_type[3] + 2] = b"\x01\x00"
    reject("wrong node applicability", lambda: validate_port(bytes(changed)))
    for name, change in [
        ("bond order", lambda d: d["bonds"][0].update(order=2)),
        ("formal charge", lambda d: d["atoms"][2].update(charge=1)),
        ("abbreviation anchor", lambda d: d["abbreviations"][0].update(anchor=2)),
        (
            "relative geometry",
            lambda d: d["atoms"][4]["position"].update(x=d["atoms"][4]["position"]["x"] + 1),
        ),
    ]:
        changed = copy.deepcopy(native)
        change(changed)
        reject(name, lambda changed=changed: validate_native(changed, reference))
    changed = slide.replace(b"ChemDraw_x64.Document.6.0", b"Other.Document.6.0")
    reject("wrong OLE producer", lambda: validate_slide(changed, relationships))
    print(
        json.dumps(
            {
                "status": "PASS",
                "exact_files": len(manifest["files"]),
                "rejected_semantic_controls": controls,
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
