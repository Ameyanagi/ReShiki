#!/usr/bin/env python3
"""Reproduce synthetic zero-style CDX/CDS without ReShiki's exporter."""

import hashlib
import importlib
import json
import struct
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[2] / "reference/engine"))
codec = importlib.import_module("cdx_exchange")


def zero_style_text(element):
    # SDK CDXString: zero style runs inherit the containing object's defaults.
    return b"\0\0" + "".join(span.text or "" for span in element if span.tag == "s").encode("ascii")


setattr(codec, "encode_text", zero_style_text)
source = (HERE / "defaults.cdxml").read_text()
root = ET.fromstring(source)
drawing = codec.to_cdx(source)
font_defaults = b""
for prefix, tag in [("Label", 0x080A), ("Caption", 0x080B)]:
    settings = [
        int(root.attrib[prefix + "Font"]),
        int(root.attrib[prefix + "Face"]),
        int(root.attrib[prefix + "Size"]) * 20,
        3,
    ]
    font_defaults += codec.property_bytes(tag, struct.pack("<HHHH", *settings))
stationery = drawing[:28] + font_defaults + drawing[28:]
for name, data in [("defaults.cdx", drawing), ("defaults.cds", stationery)]:
    (HERE / name).write_bytes(data)
try:
    codec.to_cdx((HERE / "unused-palette.cdxml").read_text())
except ValueError as error:
    unused_palette_limit = str(error)
else:
    raise AssertionError("The unused out-of-range source color cannot fit a CDX color channel")
manifest = {
    "provenance": "Synthetic hand-authored CDXML encoded with independent reference/engine/cdx_exchange.py; no ReShiki exporter or native ChemDraw authoring",
    "binary_adaptation": "SDK zero-style CDXString payloads; CDS additionally has packed LabelStyle 0x080a and CaptionStyle 0x080b document defaults",
    "cds_scope": "Full stationery payload contains artwork; drawing decoder and style loader are distinct consumers",
    "expected": {
        "atom_label": {"family": "Times New Roman", "size_pt": 18, "bold": True, "italic": True},
        "caption": {"family": "Arial", "size_pt": 12, "bold": False, "italic": True},
    },
    "unused_palette": {
        "provenance": "Synthetic representative of preparation/unused-wide-color/1.1 from the unchanged original scene oracle",
        "files": {"cdxml": "unused-palette.cdxml", "cdx": None, "cds": None},
        "expected_import": {"atoms": 2, "bonds": 1, "atom_text_styles": 0, "notices": []},
        "binary_encoder_limit": unused_palette_limit,
        "binary_representation_limit": "CDX/CDS color tables use normalized unsigned 16-bit channels; source r=1.1 is unrepresentable. It is retained unchanged, never clamped, stripped, or wrapped.",
    },
    "sha256": {
        name: hashlib.sha256((HERE / name).read_bytes()).hexdigest()
        for name in ["defaults.cdxml", "defaults.cdx", "defaults.cds", "unused-palette.cdxml"]
    },
}
(HERE / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
