#!/usr/bin/env python3
"""Independent synthetic page-boundary inputs; no ReShiki exporter is used."""

import copy
import importlib.util
import json
import math
import struct
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(ROOT / "reference/engine"))
spec = importlib.util.spec_from_file_location(
    "page_boundary_codec", ROOT / "reference/engine/cdx_exchange.py"
)
codec = importlib.util.module_from_spec(spec)
spec.loader.exec_module(codec)
encode_styled_text = codec.encode_text
encode_numeric = codec.numeric


def encode_text(element):
    spans = [span for span in element if span.tag == "s"]
    if spans and all(not span.attrib for span in spans):
        return b"\0\0" + "".join(span.text or "" for span in spans).encode("latin-1")
    return encode_styled_text(element)


codec.encode_text = encode_text


def numeric(name, kind, enum, value, encode=True):
    # Numeric line heights are twentieths of a point in native CDX/CDS.
    # Keep the reference codec's auto/variable sentinel handling unchanged.
    if encode and name in {"LineHeight", "LabelLineHeight", "CaptionLineHeight"}:
        try:
            number = float(value)
        except ValueError:
            pass
        else:
            if number not in {0, 1}:
                value = str(number * 20)
    return encode_numeric(name, kind, enum, value, encode)


codec.numeric = numeric
manifest = []


def root(**attrs):
    result = ET.Element(
        "CDXML",
        {
            "BondLength": "14.4",
            "LabelFont": "3",
            "LabelSize": "10",
            "LabelFace": "0",
            "CaptionFont": "3",
            "CaptionSize": "10",
            "CaptionFace": "0",
            **attrs,
        },
    )
    table = ET.SubElement(result, "fonttable")
    ET.SubElement(table, "font", id="3", name="Arial", charset="utf-8")
    return result


def save(name, document, description, atoms, captions):
    for pages in [1, 2]:
        source = copy.deepcopy(document)
        if pages == 2:
            page = copy.deepcopy(source.find("page"))
            parents = {child: parent for parent in page.iter() for child in parent}
            for element in page.iter():
                for key, value in list(element.attrib.items()):
                    if key in {"id", "B", "E"}:
                        element.set(key, str(int(value) + 100))
                if element.tag == "s" and parents[parents[element]].tag != "n":
                    element.text = "SECOND PAGE"
            source.append(page)
        ET.indent(source, space="  ")
        stem = f"{name}-{'single' if pages == 1 else 'two-pages'}"
        xml = (
            '<?xml version="1.0" encoding="UTF-8"?>\n'
            + ET.tostring(source, encoding="unicode")
            + "\n"
        )
        (HERE / f"{stem}.cdxml").write_text(xml)
        binary_source = copy.deepcopy(source)
        for node in binary_source.iter("n"):
            if "xyz" in node.attrib:
                # Preserve the native reader's signed16.16 values when the
                # XML spells a fractional fixed-point tick. The reference
                # encoder otherwise rounds rather than truncates this value.
                node.set(
                    "xyz",
                    " ".join(
                        str(math.trunc(float(v) * 65536) / 65536) for v in node.get("xyz").split()
                    ),
                )
        binary = codec.to_cdx(ET.tostring(binary_source, encoding="unicode"))
        defaults = b""
        for prefix, tag in [("Label", 0x080A), ("Caption", 0x080B)]:
            values = [
                round(float(source.get(prefix + suffix, fallback)) * scale)
                for suffix, fallback, scale in [
                    ("Font", "3", 1),
                    ("Face", "0", 1),
                    ("Size", "10", 20),
                    ("Color", "3", 1),
                ]
            ]
            defaults += codec.property_bytes(tag, struct.pack("<HHHH", *values))
        (HERE / f"{stem}.drawing.cds").write_bytes(binary[:28] + defaults + binary[28:])
        manifest.append(
            {
                "id": stem,
                "pages": pages,
                "atoms_per_page": atoms,
                "captions_per_page": captions,
                "description": description,
                "files": {"cdxml": f"{stem}.cdxml", "cds": f"{stem}.drawing.cds"},
            }
        )


document = root(CaptionSize="72", CaptionLineHeight="216")
page = ET.SubElement(document, "page", id="1")
text = ET.SubElement(page, "t", id="2", p="0 0")
ET.SubElement(text, "s").text = "first line\nsecond line\nthird line"
save(
    "line-height",
    document,
    "Box-less caption inherits 216pt line height with 72pt text; second page must follow all three lines.",
    0,
    1,
)

document = root(CaptionSize="36", CaptionLineHeight="108")
page = ET.SubElement(document, "page", id="1")
text = ET.SubElement(page, "t", id="2", p="0 0", WordWrapWidth="144")
ET.SubElement(text, "s").text = "alpha beta gamma delta epsilon zeta eta theta"
save(
    "wrapped-caption",
    document,
    "Box-less caption wraps without explicit newlines; inherited paragraph metrics determine its full bounds.",
    0,
    1,
)

for label in ["default", "explicit", "above", "below"]:
    document = root(LabelSize="10" if label == "explicit" else "72")
    page = ET.SubElement(document, "page", id="1")
    fragment = ET.SubElement(page, "fragment", id="2")
    atom = ET.SubElement(fragment, "n", id="3", Element="8", p="0 0")
    if label == "explicit":
        text = ET.SubElement(atom, "t", id="4")
        ET.SubElement(text, "s", font="3", size="72", face="0", color="3").text = "OH2"
    if label in {"above", "below"}:
        atom.set("LabelDisplay", label.title())
    save(
        f"large-label-{label}",
        document,
        "Box-less isolated oxygen has a72pt atom label, including automatic or explicitly stacked hydrogens; source atom text is optional.",
        1,
        0,
    )

document = root()
page = ET.SubElement(document, "page", id="1", BoundingBox="0 0 20 20")
fragment = ET.SubElement(page, "fragment", id="2")
ET.SubElement(fragment, "n", id="3", Element="8", p="0 0", xyz="0 0 1")
save(
    "xyz-origin",
    document,
    "Minimal single atom XYZ control, duplicated with independent IDs on the second page.",
    1,
    0,
)

for mixed in [False, True]:
    document = root(BondLength="24")
    page = ET.SubElement(document, "page", id="1", BoundingBox="-32 -16 32 32")
    fragment = ET.SubElement(page, "fragment", id="2")
    ET.SubElement(
        fragment,
        "n",
        id="3",
        Element="8",
        p="-16 0",
        NumHydrogens="1",
        xyz="-0.00002899169921875 0 0.0000152587890625",
    )
    carbon = ET.SubElement(fragment, "n", id="4", Element="6", p="0 0")
    if not mixed:
        carbon.set("xyz", "0 0 0")
    ET.SubElement(fragment, "b", id="5", B="3", E="4", Order="1")
    text = ET.SubElement(page, "t", id="6", p="-16 16")
    ET.SubElement(text, "s").text = "XYZ control"
    save(
        "mixed-xyz" if mixed else "fractional-xyz",
        document,
        "Nondefault24pt scale, negative fractional XYZ tick crossing zero after page placement; "
        + (
            "the same native3D fragment contains one p-only atom."
            if mixed
            else "both atoms carry explicit XYZ."
        ),
        2,
        1,
    )
    if mixed:
        page.set("BoundingBox", "-32 -16 64 80")
        ordinary = ET.SubElement(page, "fragment", id="7")
        ET.SubElement(ordinary, "n", id="8", Element="6", p="0 40")
        ET.SubElement(ordinary, "n", id="9", Element="7", p="24 40")
        ET.SubElement(ordinary, "b", id="10", B="8", E="9", Order="1")
        save(
            "hybrid-fragments",
            document,
            "The mixed native3D fragment shares a page with an independent ordinary2D fragment; both coordinate domains must receive the same visible translation.",
            4,
            1,
        )

(HERE / "manifest.json").write_text(
    json.dumps(
        {
            "provenance": "Synthetic source XML and independent Python reference CDX packing; CDS retains drawing objects and packed font defaults.",
            "binary_notes": [
                "Unstyled s is a zero-run CDXString, retaining document defaults.",
                "Numeric LineHeight/CaptionLineHeight/LabelLineHeight uses native twentieths of a point; auto/variable sentinel values are unchanged.",
                "XYZ is canonicalized to trunc(value*65536)/65536 before binary packing so paired sources preserve the same native integer coordinates.",
            ],
            "encoder_baseline": "encoder-baseline/*.reference.cds retains the original legacy-reference numeric lineheight packing, which describes unsupported 0.15x paragraph spacing rather than the intended 3x. These are source-encoder diagnostic receipts, not valid counterparts of the intended XML controls.",
            "cases": manifest,
        },
        indent=2,
    )
    + "\n"
)
