#!/usr/bin/env python3
"""Independent synthetic clipboard geometry; never uses ReShiki's exporter."""

import copy
import hashlib
import importlib.util
import json
import struct
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(ROOT / "reference/engine"))
spec = importlib.util.spec_from_file_location(
    "clipboard_layout_codec", ROOT / "reference/engine/cdx_exchange.py"
)
assert spec is not None and spec.loader is not None
codec = importlib.util.module_from_spec(spec)
spec.loader.exec_module(codec)
# Native ChemDraw 26 uses 0x8021; the reference's published 0x8027 alias
# silently loses arrows in ChemDraw (see the main corpus provenance).
codec.OBJECTS.pop(0x8027, None)
codec.OBJECTS[0x8021] = "arrow"

document = ET.fromstring(
    """<CDXML BondLength="14.4" LabelFont="101" LabelSize="10"
      CaptionFont="101" CaptionSize="10">
      <fonttable><font id="101" charset="iso-8859-1" name="Arial"/></fonttable>
      <page id="10">
        <fragment id="20">
          <n id="1" p="10 12"/><n id="2" p="21 15"/>
          <n id="3" p="38 20" Element="8"/>
          <b id="4" B="1" E="2"/><b id="5" B="2" E="3"/>
        </fragment>
        <fragment id="30">
          <n id="11" p="113 47"/><n id="12" p="119 63"/>
          <n id="13" p="143 51" Element="7"/>
          <b id="14" B="11" E="12"/><b id="15" B="12" E="13"/>
        </fragment>
        <arrow id="40" Tail3D="55 22 0" Head3D="94 27 0"
          ArrowheadHead="Full" ArrowheadType="Solid"/>
        <t id="41" p="62 9"><s font="101" size="10">NaOH</s></t>
        <t id="42" p="68 42"><s font="101" size="10">rt, 5 h</s></t>
        <graphic id="43" BoundingBox="49 20 46 17"
          GraphicType="Symbol" SymbolType="Plus"/>
        <graphic id="44" BoundingBox="18 40 11 35" GraphicType="Rectangle"/>
      </page>
    </CDXML>"""
)
cases = []
for name, length in [
    ("valid-layout", "14.4"),
    ("explicit-30", "30"),
    ("missing-bond-length", None),
    ("unsupported-graphic", "14.4"),
]:
    source = copy.deepcopy(document)
    if length is None:
        del source.attrib["BondLength"]
    else:
        source.set("BondLength", length)
    if name == "unsupported-graphic":
        page = source.find("page")
        assert page is not None
        ET.SubElement(
            page,
            "graphic",
            id="45",
            BoundingBox="77 64 53 52",
            GraphicType="Rectangle",
            LineWidth="20",
        )
    ET.indent(source, space="  ")
    xml = (
        '<?xml version="1.0" encoding="UTF-8"?>\n' + ET.tostring(source, encoding="unicode") + "\n"
    )
    binary = codec.to_cdx(xml)
    # CDS stores the same drawing plus native packed default text styles.
    styles = b"".join(
        codec.property_bytes(tag, struct.pack("<HHHH", 101, 0, 200, 0)) for tag in [0x080A, 0x080B]
    )
    files = {
        "cdxml": (f"{name}.cdxml", xml.encode()),
        "cdx": (f"{name}.cdx", binary),
        "cds": (f"{name}.drawing.cds", binary[:28] + styles + binary[28:]),
    }
    for filename, data in files.values():
        (HERE / filename).write_bytes(data)
    cases.append(
        {
            "id": name,
            "bond_length": length,
            "expected": "import-with-notice"
            if name == "unsupported-graphic"
            else "preserve-layout",
            "files": {format_: filename for format_, (filename, _) in files.items()},
            "sha256": {
                format_: hashlib.sha256(data).hexdigest() for format_, (_, data) in files.items()
            },
        }
    )
(HERE / "misleading.smiles").write_text("CCO.CCN\n")
(HERE / "manifest.json").write_text(
    json.dumps(
        {
            "provenance": "Synthetic hand-authored asymmetric geometry, encoded independently by reference/engine/cdx_exchange.py; not native-authored",
            "geometry_contract": "All paper points convert by 42/14.4 and may receive only one shared translation, including across disconnected molecules and reaction stamps",
            "cases": cases,
        },
        indent=2,
    )
    + "\n"
)
