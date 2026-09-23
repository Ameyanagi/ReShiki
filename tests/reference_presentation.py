"""Check the explicit 0.7 dummy-label contract against legacy drawing output.

Chemistry and the rest of each response are still compared without modification.
Binary drawings use the independent Python codec, not the Rust implementation.
"""

import base64
import json
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from engine.cdx_exchange import from_cdx  # noqa: E402


def tree(node):
    return (node.tag, node.attrib, node.text, node.tail, [tree(child) for child in node])


def compare(actual, expected, format):
    if format == "cdx":
        actual = from_cdx(base64.b64decode(actual, validate=True))
        expected = from_cdx(base64.b64decode(expected, validate=True))
    actual = ET.fromstring(actual)
    expected = ET.fromstring(expected)
    changed = 0
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
        raise ValueError("Drawing differs beyond the explicit hidden-dummy presentation contract")


def main():
    request = json.load(sys.stdin)
    compare(request["actual"], request["expected"], request["format"])


if __name__ == "__main__":
    main()
