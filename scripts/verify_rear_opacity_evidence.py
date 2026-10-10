#!/usr/bin/env python3
"""Replay retained C60 native/history checks without launching ReShiki.

Optional --export-dir checks the independent signed-CLI exports with Pillow.
Those exports are measurements, not desktop captures. No input is modified.
"""

import argparse
import hashlib
import json
import math
import struct
import xml.etree.ElementTree as ET
from pathlib import Path

PREFIX = "c60-rear-opacity-"
HASHES = {
    "before-desktop": "121280660ab89d7a53a7f0a336d5c23edc4c7a46e8dafe143443c9ff884a7d6e",
    "100": "a2ea34712470ce645f4efa7386089d662001ce2a13ecd07c6d51982ebb5dc2ef",
    "25": "29e4ac58273964ae673267769c56e5f1fe4e036e06a4875ebbbe2ff645ec4ee1",
    "0": "5ba045fc362dbc16ef3aebd08f5d41b463e64f1a810391c45c16340c5fe4373c",
}
GROUPS = [
    ["100", "25-undo"],
    ["25", "25-redo", "0-undo", "25-fresh-reopen", "25-noop"],
    ["0", "0-redo", "0-fresh-reopen"],
]


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def f32(value):
    return struct.unpack("f", struct.pack("f", value))[0]


def native_check(directory, typed_before=None):
    before_path = typed_before or directory / (PREFIX + "before-desktop.rsk")
    raw = {"before-desktop": before_path.read_bytes()}
    for group in GROUPS:
        for name in group:
            raw[name] = (directory / (PREFIX + name + ".rsk")).read_bytes()
    for name, expected in HASHES.items():
        require(sha(raw[name]) == expected, f"immutable native SHA changed: {name}")
    for group in GROUPS:
        require(all(raw[x] == raw[group[0]] for x in group), f"raw equality: {group}")
    docs = {name: json.loads(data) for name, data in raw.items()}
    before = docs["before-desktop"]
    require(before["version"] == 19, "typed-before version")
    require(len(before["atoms"]) == 60 and len(before["bonds"]) == 90, "C60 counts")
    original = {k: v for k, v in before.items() if k != "version"}
    require(
        {k: v for k, v in docs["100"].items() if k != "version"} == original,
        "100% may change only document version",
    )
    ids = [a["id"] for a in before["atoms"]]
    require(len(set(ids)) == 60, "atom identity uniqueness")
    require(
        all(
            math.isfinite(v)
            for a in before["atoms"]
            for v in [a["position"]["x"], a["position"]["y"], a["depth"]]
        ),
        "finite XYZ",
    )
    lo = min(f32(a["depth"]) for a in before["atoms"])
    hi = max(f32(a["depth"]) for a in before["atoms"])
    span = f32(hi - lo)
    require(span > 0.001, "retained 3D depth range")
    for name, doc in docs.items():
        if name == "before-desktop":
            continue
        require(doc["version"] == 22, f"candidate version: {name}")
        require(
            {k: v for k, v in doc.items() if k not in {"version", "depth_appearance"}} == original,
            f"graph/XYZ/other native field changed: {name}",
        )
        if name in GROUPS[0]:
            require(not doc.get("depth_appearance"), f"100% has alpha scope: {name}")
            continue
        alpha = 0.0 if name in GROUPS[2] else 0.25
        scopes = doc["depth_appearance"]
        require(len(scopes) == 1, f"scope count: {name}")
        scope = scopes[0]
        require(scope["atoms"] == ids, f"scope owner IDs: {name}")
        require(scope["automatic"] is True and scope["strength"] == 0, f"RGB state: {name}")
        require(scope["rear_opacity"] == alpha, f"alpha setting: {name}")
        require(
            not any(scope.get(k) for k in ["overrides", "rear_overrides", "rear_weights"]),
            f"unexpected paint override: {name}",
        )
        require(set(scope["weights"]) == {str(x) for x in ids}, f"weight IDs: {name}")
        for atom in before["atoms"]:
            expected = f32(f32(hi - f32(atom["depth"])) / span)
            require(
                f32(scope["weights"][str(atom["id"])]) == expected,
                f"stored rear weight changed: {name}/{atom['id']}",
            )
    changed = json.loads(raw["25"])
    changed["depth_appearance"][0]["rear_opacity"] = 0.0
    require(changed == docs["0"], "25% to0% may change rear_opacity only")
    weights = docs["25"]["depth_appearance"][0]["weights"]
    rear_count = sum(f32(weight) > 0.5 for weight in weights.values())
    front_count = len(weights) - rear_count
    require((front_count, rear_count) == (30, 30), "front/rear atom partition")
    return {
        "native_files": len(raw),
        "atoms": 60,
        "bonds": 90,
        "xyz_values": 180,
        "all_nonappearance_fields_exact": True,
        "raw_equality_groups": GROUPS,
        "native_sha256": {name: sha(data) for name, data in raw.items()},
        "single_scope_rgb_strength": 0,
        "rear_weights_float32_exact": True,
        "front_atoms": front_count,
        "rear_atoms": rear_count,
    }


def svg_parts(path):
    root = ET.parse(path).getroot()
    parts = []

    def visit(node, alpha=1.0, layers=0):
        if "opacity" in node.attrib:
            alpha *= float(node.attrib["opacity"])
            layers += 1
        tag = node.tag.rsplit("}", 1)[-1]
        if tag in {"path", "line", "polygon", "polyline", "circle", "ellipse", "text"}:
            parts.append((alpha, layers, tag, sorted(node.attrib.items())))
        for child in node:
            visit(child, alpha, layers)

    visit(root)
    return root.attrib, parts


def export_check(directory):
    from PIL import Image

    for ext in ["svg", "png", "pdf"]:
        require(
            (directory / ("c60-typed-before." + ext)).read_bytes()
            == (directory / ("c60-100." + ext)).read_bytes(),
            f"100% baseline export changed: {ext}",
        )
    analyses = [
        json.loads((directory / ("analyze-" + name + ".stdout")).read_text())
        for name in ["100", "25", "0"]
    ]
    require(analyses[0] == analyses[1] == analyses[2], "chemical analysis changed")
    require(analyses[0]["value"]["analysis"]["formula"] == "C60", "C60 formula")
    parts = {name: svg_parts(directory / ("c60-" + name + ".svg")) for name in ["100", "25", "0"]}
    rear = [p for p in parts["25"][1] if p[0] < 1]
    require(len(rear) == 16, "25% faded primitive count")
    require(all(p[0] == 0.25 and p[1] == 1 for p in rear), "alpha applied more than once")
    front = [p[2:] for p in parts["25"][1] if p[0] == 1]
    require(len(front) == 15 and front == [p[2:] for p in parts["0"][1]], "front geometry changed")
    require(all(p[0] == 1 for p in parts["0"][1]), "0% retained faded primitives")
    samples = json.loads((directory / "export-pixel-samples.json").read_text())["samples"]
    require({x["classification"] for x in samples} == {"front", "rear"}, "pixel control classes")
    images = {name: Image.open(directory / ("c60-" + name + ".png")) for name in ["100", "25", "0"]}
    for sample in samples:
        for name in ["100", "25", "0"]:
            pixel = sample["PNG0_pixel"] if name == "0" else sample["pixel"]
            expected = [0, 0, 0, 255]
            if sample["classification"] == "rear" and name == "25":
                expected = [191, 191, 191, 255]
            elif sample["classification"] == "rear" and name == "0":
                expected = [255, 255, 255, 255]
            actual = images[name].getpixel(tuple(pixel))
            require(
                isinstance(actual, tuple) and list(actual) == expected,
                f"PNG {name} sample changed",
            )
    pdf = (directory / "c60-25.pdf").read_bytes()
    require(pdf.startswith(b"%PDF-") and b"/ca 0.25" in pdf, "PDF alpha")
    return {
        "typed_before_exports_byte_identical100": ["svg", "png", "pdf"],
        "analysis": analyses[0]["value"]["analysis"],
        "effective_rear_svg_alpha": 0.25,
        "alpha_wrappers_per_rear_primitive": 1,
        "opaque_front_primitives_exact25_to0": 15,
        "faded_primitives25": 16,
        "faded_primitives0": 0,
        "PNG_paper": "opaque white; rear RGB191 at25%, white at0%; front black unchanged",
        "PNG_bounds": {name: image.size for name, image in images.items()},
        "pixel_samples_checked": len(samples),
        "PDF_partial_alpha_present": True,
        "scope": "fresh signed-CLI exports, not desktop-canvas screenshots",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--native-dir",
        type=Path,
        default=Path(__file__).resolve().parent.parent / "tests/fixtures/rear-opacity",
    )
    parser.add_argument("--typed-before", type=Path)
    parser.add_argument("--export-dir", type=Path)
    args = parser.parse_args()
    result = {"status": "PASS", "native": native_check(args.native_dir, args.typed_before)}
    if args.export_dir:
        result["exports"] = export_check(args.export_dir)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
