#!/usr/bin/env python3
"""Audit the retained Windows desktop artifacts with Python's standard library.

This is a fixed evidence reader, not a general EMF validator or native renderer.
It neither imports application code nor launches an application/build/network tool.
"""

import argparse
import base64
import copy
import hashlib
import json
import math
import struct
import subprocess
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
BUILT = "783be63a2752103cd2c436a79e02a03d442e6085"
REVIEWED = "692d4c534cdbe2cb74332a22d87f226039360422"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def emf(data):
    require(len(data) >= 88, "short EMF header")
    require(struct.unpack_from("<I", data, 0)[0] == 1, "missing EMF header")
    require(data[40:44] == b" EMF", "EMF signature")
    declared_bytes, declared_records = struct.unpack_from("<II", data, 48)
    require(declared_bytes == len(data), "EMF byte count")
    records = []
    offset = 0
    while offset < len(data):
        require(offset + 8 <= len(data), "short record header")
        kind, size = struct.unpack_from("<II", data, offset)
        require(size >= 8 and size % 4 == 0, "record size/alignment")
        require(offset + size <= len(data), "record boundary")
        records.append((kind, data[offset : offset + size]))
        offset += size
    require(len(records) == declared_records, "record count")
    require(records[-1][0] == 14, "missing final EOF")
    require(all(kind != 14 for kind, _ in records[:-1]), "early EOF")
    plus = []
    for kind, record in records:
        if kind != 70:
            continue
        require(len(record) >= 12, "short comment")
        count = struct.unpack_from("<I", record, 8)[0]
        require(count <= len(record) - 12, "comment boundary")
        if record[12:16] != b"EMF+":
            continue
        cursor, end = 16, 12 + count
        while cursor < end:
            require(cursor + 12 <= end, "short EMF+ header")
            typ, flags, size, payload_size = struct.unpack_from("<HHII", record, cursor)
            require(size >= 12 and size % 4 == 0, "EMF+ size/alignment")
            require(cursor + size <= end and payload_size <= size - 12, "EMF+ boundary")
            raw = record[cursor : cursor + size]
            plus.append((typ, flags, raw, raw[12 : 12 + payload_size]))
            cursor += size
    return struct.unpack_from("<4i", data, 24), records, plus


def selected(plus, kind):
    return [raw for typ, _, raw, _ in plus if typ == kind]


def bitmaps(plus):
    return [
        payload
        for typ, flags, _, payload in plus
        if typ == 0x4008
        and (flags >> 8) & 0x7F == 5
        and len(payload) >= 8
        and struct.unpack_from("<I", payload, 4)[0] == 1
    ]


def check_vectors(source, exported):
    frame, source_records, source_plus = emf(source)
    require(frame == (0, 0, 10000, 6000), "source 100 x 60 mm frame")
    require(len(source_records) == 411, "source record count")
    frame, records, plus = emf(exported)
    require(frame == (0, 0, 10282, 6282), "padded export frame")
    require(len(records) == 422, "export record count")
    nested = []
    for typ, flags, _, payload in plus:
        if typ != 0x4008 or (flags >> 8) & 0x7F != 5:
            continue
        require(flags & 0x8000 == 0, "unexpected continued image object")
        require(len(payload) >= 16, "short image object")
        if struct.unpack_from("<I", payload, 4)[0] != 2:
            continue
        metafile_type, count = struct.unpack_from("<II", payload, 8)
        require(metafile_type == 4 and count == len(payload) - 16, "nested metafile")
        nested.append(payload[16:])
    require(len(nested) == 1, "expected one nested vector metafile")
    _, nested_records, nested_plus = emf(nested[0])
    require(len(nested_records) == 45, "nested record count")
    for kind, count, label in [(0x400D, 23, "DrawLines"), (0x401C, 16, "DrawString")]:
        expected = selected(source_plus, kind)
        require(len(expected) == count, f"source {label} count")
        require(selected(nested_plus, kind) == expected, f"byte-exact nested {label}")
    require(len(bitmaps(source_plus)) == 1, "source raster inset count")
    require(bitmaps(nested_plus) == bitmaps(source_plus), "retained raster inset object")
    require(selected(nested_plus, 0x401B) == selected(source_plus, 0x401B), "raster placement")
    require(sum(kind == 87 for kind, _ in records) == 23, "classic POLYLINE16 fallback")
    require(sum(kind == 84 for kind, _ in records) == 16, "classic EXTTEXTOUTW fallback")
    require(sum(kind == 81 for kind, _ in records) == 1, "classic raster fallback")
    return {
        "source_frame_mm": [100, 60],
        "export_frame_mm": [102.82, 62.82],
        "export_records": 422,
        "nested_sha256": sha(nested[0]),
        "exact_DrawLines": 23,
        "exact_DrawString": 16,
        "raster_inset_exact": True,
        "classic_vector_text_fallback": True,
    }


def check_native(document, source, reference):
    require(document["version"] == 20, "native version")
    require(
        all(document[key] == [] for key in ["atoms", "bonds", "annotations", "arrows", "groups"]),
        "one picture only",
    )
    require(len(document["graphics"]) == 1, "picture count")
    graphic = document["graphics"][0]
    require(graphic["kind"] == "picture", "picture kind")
    require(
        base64.b64decode(graphic["picture"]["emf"], validate=True) == source,
        "retained original EMF",
    )
    png = base64.b64decode(graphic["picture"]["png"], validate=True)
    require(png[:8] == b"\x89PNG\r\n\x1a\n" and png[12:16] == b"IHDR", "PNG header")
    require(struct.unpack_from(">II", png, 16) == (4724, 2834), "PNG pixel dimensions")
    require(
        sha(png) == "1dabe5ad6f5806cd121f294a19eaa697dfc9f6957518da19ecbfe858733139ca",
        "canonical preview",
    )
    reference_graphic = reference["graphics"][0]
    require(
        graphic["picture"] == reference_graphic["picture"],
        "same original source and preview as harness",
    )
    require(
        graphic["axis_x"] == reference_graphic["axis_x"]
        and graphic["axis_y"] == reference_graphic["axis_y"],
        "unscaled original picture axes",
    )
    # Native default units: 42 world units = 14.4 pt, hence 210 per inch.
    dimensions = [math.hypot(*graphic[axis].values()) * 25.4 / 210 for axis in ["axis_x", "axis_y"]]
    require(
        all(abs(got - want) < 0.0001 for got, want in zip(dimensions, [100, 60])),
        "native physical dimensions",
    )
    normalized = copy.deepcopy(document)
    normalized["graphics"][0]["origin"] = reference_graphic["origin"]
    require(normalized == reference, "GUI/reference difference is only placement origin")
    return {
        "version": 20,
        "pictures": 1,
        "picture_mm": dimensions,
        "preview_pixels": [4724, 2834],
        "embedded_source_exact": True,
    }


def jpeg_size(data):
    require(data[:2] == b"\xff\xd8", "JPEG SOI")
    cursor = 2
    while cursor < len(data):
        require(data[cursor] == 255, "JPEG marker")
        while data[cursor] == 255:
            cursor += 1
        marker = data[cursor]
        cursor += 1
        require(marker not in (0xDA, 0xD9), "JPEG frame missing")
        length = struct.unpack_from(">H", data, cursor)[0]
        require(length >= 2 and cursor + length <= len(data), "JPEG segment")
        if marker in (0xC0, 0xC1, 0xC2):
            height, width = struct.unpack_from(">HH", data, cursor + 3)
            return width, height
        cursor += length
    raise ValueError("JPEG frame missing")


def check_hash(data, expected):
    require(
        len(data) == expected["bytes"] and sha(data) == expected["sha256"], "manifest bytes/hash"
    )


def negative_controls(document, source, exported, reference, screenshot, screenshot_entry):
    cases = []
    wrong_source = copy.deepcopy(document)
    wrong_source["graphics"][0]["picture"]["emf"] = base64.b64encode(
        source[:-1] + bytes([source[-1] ^ 1])
    ).decode()
    cases.append(
        ("altered embedded original", lambda: check_native(wrong_source, source, reference))
    )
    scaled = copy.deepcopy(document)
    scaled["graphics"][0]["axis_x"]["x"] *= 2
    cases.append(("doubled picture width", lambda: check_native(scaled, source, reference)))
    frame = bytearray(exported)
    struct.pack_into("<i", frame, 32, 10283)
    cases.append(("one frame-unit width change", lambda: check_vectors(source, bytes(frame))))
    _, _, source_plus = emf(source)
    line = selected(source_plus, 0x400D)[0]
    changed_line = bytearray(exported)
    offset = exported.index(line)
    changed_line[offset + len(line) - 1] ^= 1
    cases.append(
        ("altered nested source vector", lambda: check_vectors(source, bytes(changed_line)))
    )
    fallback = bytearray(exported)
    offset = 0
    while struct.unpack_from("<I", fallback, offset)[0] != 87:
        offset += struct.unpack_from("<I", fallback, offset + 4)[0]
    struct.pack_into("<I", fallback, offset, 86)
    cases.append(
        ("missing classic vector fallback", lambda: check_vectors(source, bytes(fallback)))
    )
    cases.append(("changed raw screenshot", lambda: check_hash(screenshot[:-1], screenshot_entry)))
    rejected = []
    for label, check in cases:
        try:
            check()
        except ValueError:
            rejected.append(label)
        else:
            raise ValueError(f"negative control accepted: {label}")
    return rejected


def source_check(root):
    manifest = json.loads((HERE / "emf-import-source-v3.json").read_bytes())["files"]
    selected_files = {
        name: item
        for name, item in manifest.items()
        if name.endswith(".rs")
        or (name.endswith("Cargo.toml") or name == "Cargo.lock")
        or name.startswith(("assets/", "presets/"))
    }
    require(len(selected_files) == 1120, "compiled source selection count")
    queries = "".join(f"{BUILT}:{name}\n" for name in selected_files)
    raw = subprocess.run(
        ["git", "-C", str(root), "cat-file", "--batch"],
        input=queries.encode(),
        capture_output=True,
        check=True,
    ).stdout
    offset = 0
    for name, expected in selected_files.items():
        end = raw.index(b"\n", offset)
        header = raw[offset:end].split()
        require(len(header) == 3 and header[1] == b"blob", f"missing source object {name}")
        size = int(header[2])
        blob = raw[end + 1 : end + 1 + size]
        check_hash(blob, expected)
        offset = end + 2 + size
    changed = subprocess.run(
        ["git", "-C", str(root), "diff", "--name-only", BUILT, REVIEWED],
        capture_output=True,
        check=True,
        text=True,
    ).stdout.splitlines()
    code = [
        name
        for name in changed
        if name.startswith(("src/", "crates/", "native/"))
        or name in ("Cargo.toml", "Cargo.lock", "build.rs")
    ]
    require(
        code
        == [
            "crates/agent/tests/headless.rs",
            "crates/mcp/tests/transcripts/tools-fake-legacy.jsonl",
            "crates/mcp/tests/transcripts/tools-fake-modern.jsonl",
        ],
        "test-only code delta",
    )
    require(
        all(name in code or name.startswith(("docs/", "tests/fixtures/mcp/")) for name in changed),
        "unexpected reviewed source delta",
    )
    return {
        "matched_manifest_inputs": 1120,
        "built_commit": BUILT,
        "reviewed_commit": REVIEWED,
        "all_changed_paths": changed,
        "production_changes": [],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--source-root",
        type=Path,
        help="also compare retained source manifest to local Git objects; no build",
    )
    args = parser.parse_args()
    manifest = json.loads((HERE / "evidence-manifest.json").read_bytes())
    screenshots = []
    for item in manifest["files"]:
        path = (REPO / item["path"]).resolve()
        require(REPO in path.parents, "manifest path outside repository")
        data = path.read_bytes()
        check_hash(data, item)
        if path.suffix == ".jpg":
            require(jpeg_size(data) == (2556, 1712), "raw screenshot dimensions")
            screenshots.append((data, item))
    require(len(screenshots) == 5, "selected raw screenshot count")
    source = (HERE / "source.emf").read_bytes()
    require(
        source == (HERE.parent / "controlled-spectrum.emf").read_bytes(),
        "same controlled source as prior evidence",
    )
    document = json.loads((HERE / "desktop-import-20261010.rsk").read_bytes())
    reference = json.loads((HERE.parent / "retained-source.rsk").read_bytes())
    exported = (HERE / "desktop-vector-reexport-20261010.emf").read_bytes()
    root_receipt = json.loads((HERE / "native-verification.json").read_bytes())
    require(
        root_receipt["candidate"]["native_saved_sha256"]
        == sha((HERE / "desktop-import-20261010.rsk").read_bytes()),
        "root native receipt correlation",
    )
    require(
        root_receipt["candidate"]["exported_sha256"] == sha(exported),
        "root export receipt correlation",
    )
    result = {
        "status": "PASS",
        "raw_files": len(manifest["files"]),
        "raw_bytes": sum(item["bytes"] for item in manifest["files"]),
        "raw_screenshots": len(screenshots),
        "native": check_native(document, source, reference),
        "vectors": check_vectors(source, exported),
        "negative_controls_rejected": negative_controls(
            document, source, exported, reference, *screenshots[0]
        ),
        "scope": "Retained artifact identity and semantics; not fresh GUI execution, native playback, Office acceptance or physical display-DPI validation.",
    }
    if args.source_root:
        result["source_bridge"] = source_check(args.source_root)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
