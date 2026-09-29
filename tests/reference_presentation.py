"""Check documented hidden-dummy and bond-spacing changes against legacy output.

Chemistry and the rest of each response are still compared without modification.
Binary drawings use the independent Python codec, not the Rust implementation.
"""

import base64
import json
import math
import struct
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from engine.cdx_exchange import OBJECTS, PROPERTIES, from_cdx  # noqa: E402


def current_cdx(data):
    """Decode only the captured arrow/FillType corrections with the old codec.

    Keep the legacy expected drawing on its historical mappings. No chemistry,
    geometry, text, or style properties are removed from either comparison.
    """
    try:
        legacy = from_cdx(data)
    except ValueError:
        legacy = None  # The old reader can reject the genuine arrow object tag.
    objects = OBJECTS.copy()
    fill = PROPERTIES[0xA37]
    try:
        OBJECTS.update({0x8021: "arrow", 0x800D: "scheme", 0x800E: "step"})
        PROPERTIES[0xA37] = (
            "FillType",
            "INT16",
            {"Unspecified": 0, "None": 1, "Solid": 2, "Shaded": 4},
        )
        current = from_cdx(data)
        return current, int(current != legacy)
    finally:
        OBJECTS.clear()
        OBJECTS.update(objects)
        PROPERTIES[0xA37] = fill


def tree(node):
    return (node.tag, node.attrib, node.text, node.tail, [tree(child) for child in node])


def compare(actual, expected, format):
    changed = 0
    if format == "cdx":
        actual, changed = current_cdx(base64.b64decode(actual, validate=True))
        expected = from_cdx(base64.b64decode(expected, validate=True))
    actual = ET.fromstring(actual)
    expected = ET.fromstring(expected)
    # The old writer widens f32 before multiplying by 100, yielding values
    # such as 18.000000715255737 instead of 18. Only the document percentage
    # may differ, and both spellings must represent exactly the same f32.
    spacing = actual.get("BondSpacing")
    legacy_spacing = expected.get("BondSpacing")
    if spacing is not None and legacy_spacing is not None and spacing != legacy_spacing:
        a, e = float(spacing), float(legacy_spacing)
        if math.isfinite(a) and math.isfinite(e) and struct.pack("<f", a) == struct.pack("<f", e):
            expected.set("BondSpacing", spacing)
            changed += 1
    for node in expected.iter("n"):
        if node.get("Element") != "0" or node.get("NodeType") is not None:
            continue
        label = node.find("t")
        if label is None or "".join(label.itertext()) != "*":
            continue
        # ChemDraw-resaved hidden-dummy fixtures establish these exact values.
        # Retain the sentinel text, atom identity, charge, connectivity and IDs.
        node.set("Visible", "no")
        node.set("NodeType", "Unspecified")
        node.set("NumHydrogens", "0")
        if label.get("LabelAlignment") == "Auto":
            del label.attrib["LabelAlignment"]
        changed += 1
    if not changed or tree(actual) != tree(expected):
        raise ValueError(
            "Drawing differs beyond hidden-dummy, bond-spacing, and arrow compatibility contracts"
        )


def main():
    request = json.load(sys.stdin)
    compare(request["actual"], request["expected"], request["format"])


if __name__ == "__main__":
    main()
