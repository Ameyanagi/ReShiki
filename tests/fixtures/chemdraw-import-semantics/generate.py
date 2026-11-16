#!/usr/bin/env python3
"""Small SDK-based semantic probes; never constructs inputs with ReShiki export."""

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
    "adversarial_cdx", ROOT / "reference/engine/cdx_exchange.py"
)
codec = importlib.util.module_from_spec(spec)
spec.loader.exec_module(codec)
# ChemDraw's native arrow object is 0x8021. The published SDK/reference
# 0x8027 alias opens without an arrow in the native ChemDraw spot-check.
codec.OBJECTS.pop(0x8027, None)
codec.OBJECTS[0x8021] = "arrow"
original_encode_text = codec.encode_text


def encode_text(element):
    spans = [s for s in element if s.tag == "s"]
    if spans and all(not span.attrib for span in spans):
        # SDK CDXString explicitly defines zero style runs as unspecified
        # font/size, allowing the containing object's defaults to apply.
        return b"\0\0" + "".join(span.text or "" for span in spans).encode("latin-1")
    return original_encode_text(element)


codec.encode_text = encode_text
cases = []
case_metadata = {
    "01-hidden-graphic": {
        "expected_semantics": {
            "visible_graphics": 1,
            "hidden_graphics": 1,
            "hidden_ink_rgb": [255, 0, 0],
        },
        "known_issue": "Visible=no becomes visible without warning",
    },
    "02-hidden-caption": {
        "expected_semantics": {"visible_text": ["visible control"], "hidden_text": ["HIDDEN TEXT"]},
        "known_issue": "Visible=no becomes visible without warning",
    },
    "03-hidden-arrow": {
        "expected_semantics": {
            "visible_arrows": 1,
            "hidden_arrows": 1,
            "hidden_ink_rgb": [255, 0, 0],
        },
        "known_issue": "Visible=no becomes visible without warning",
    },
    "04-hidden-modern-spline": {
        "expected_semantics": {
            "visible_text": ["visible control"],
            "hidden_graphics": 1,
            "hidden_ink_rgb": [255, 0, 0],
        },
        "known_issue": "Visible=no becomes visible without warning",
    },
    "05-grouped-hidden-children": {
        "expected_semantics": {
            "visible_text": ["visible control"],
            "hidden_text": ["HIDDEN GROUP CHILD"],
            "hidden_graphics": 1,
            "group_integral": True,
        },
        "known_issue": "Visible=no becomes visible without warning",
    },
    "06-caption-defaults": {
        "expected_semantics": {
            "caption_style": {
                "family": "Times New Roman",
                "size_pt": 18,
                "bold": True,
                "italic": True,
            }
        }
    },
    "07-label-defaults-no-text": {
        "expected_semantics": {
            "oxygen_label_style": {
                "family": "Times New Roman",
                "size_pt": 18,
                "bold": True,
                "italic": True,
            }
        },
        "known_issue": "Root LabelFace ignored when node has no t child or node-level style",
    },
    "08-label-defaults-with-text": {
        "expected_semantics": {
            "oxygen_label_style": {
                "family": "Times New Roman",
                "size_pt": 18,
                "bold": True,
                "italic": True,
            }
        }
    },
    "09-document-line-width": {
        "expected_semantics": {
            "visible_graphics": 2,
            "stroke_widths_pt": [2, 0.6],
            "stroke_rgb": [[255, 0, 0], [0, 0, 255]],
        }
    },
    "10-page-bounds-margins": {
        "expected_semantics": {
            "source_page_bounds_pt": [0, 0, 432, 288],
            "source_print_margins": [12, 18, 24, 30],
            "documented_limit": "Publication-page layout is not retained by molecular interchange.",
        }
    },
    "11-modern-multisegment-heads": {
        "expected_semantics": {
            "cubic_segments": 2,
            "head": "full",
            "tail": "right",
            "head_shape": "hollow",
        }
    },
    "12-legacy-multisegment-heads": {
        "expected_semantics": {
            "cubic_segments": 2,
            "CurveType": 8,
            "head": "full",
            "tail": "none",
            "support_note": "Valid SDK legacy representation; both cubic segments and the full end head must be retained.",
        }
    },
}


class Drawing:
    def __init__(self, name, description, **defaults):
        self.name = name
        self.description = description
        self.next_id = 1
        attrs = dict(
            BondLength="24",
            BondSpacing="18",
            LineWidth="0.6",
            BoldWidth="2",
            MarginWidth="1.6",
            HashSpacing="2.5",
            LabelFont="3",
            CaptionFont="3",
            LabelSize="10",
            CaptionSize="10",
            LabelFace="0",
            CaptionFace="0",
        )
        attrs.update(defaults)
        self.root = ET.Element("CDXML", attrs)
        fonts = ET.SubElement(self.root, "fonttable")
        ET.SubElement(fonts, "font", id="3", name="Arial", charset="utf-8")
        ET.SubElement(fonts, "font", id="4", name="Times New Roman", charset="utf-8")
        colors = ET.SubElement(self.root, "colortable")
        for r, g, b in [(1, 1, 1), (0, 0, 0), (1, 0, 0), (0, 0, 1)]:
            ET.SubElement(colors, "color", r=str(r), g=str(g), b=str(b))
        self.page = self.add(self.root, "page", BoundingBox="0 0 360 240")

    def add(self, parent, tag, **attrs):
        element = ET.SubElement(parent, tag, id=str(self.next_id), **attrs)
        self.next_id += 1
        return element

    def graphic(self, parent=None, **attrs):
        return self.add(
            parent if parent is not None else self.page,
            "graphic",
            GraphicType="Rectangle",
            BoundingBox=attrs.pop("BoundingBox", "30 30 100 80"),
            RectangleType="Filled",
            **attrs,
        )

    def caption(self, text, parent=None, defaults=False, **attrs):
        t = self.add(
            parent if parent is not None else self.page, "t", p=attrs.pop("p", "30 120"), **attrs
        )
        run = {} if defaults else dict(font="3", size="10", face="0", color="3")
        ET.SubElement(t, "s", run).text = text
        return t

    def arrow(self, **attrs):
        return self.add(
            self.page,
            "arrow",
            Tail3D=attrs.pop("Tail3D", "30 50 0"),
            Head3D=attrs.pop("Head3D", "150 50 0"),
            ArrowheadHead="Full",
            ArrowheadTail="None",
            ArrowheadType="Solid",
            **attrs,
        )

    def methanol(self, nested=False):
        f = self.add(self.page, "fragment")
        c = self.add(f, "n", Element="6", p="50 70")
        o = self.add(f, "n", Element="8", p="74 70", NumHydrogens="1")
        self.add(f, "b", B=c.get("id"), E=o.get("id"), Order="1")
        if nested:
            t = self.add(o, "t", p="74 70")
            ET.SubElement(t, "s").text = "OH"

    def save(self):
        ET.indent(self.root, space="  ")
        xml = ET.tostring(self.root, encoding="unicode")
        path = HERE / "inputs" / self.name
        path.parent.mkdir(exist_ok=True)
        path.with_suffix(".cdxml").write_text('<?xml version="1.0"?>\n' + xml + "\n")
        binary = codec.to_cdx(xml)
        path.with_suffix(".cdx").write_bytes(binary)
        packed = b""
        for prefix, code in [("Label", 0x080A), ("Caption", 0x080B)]:
            defaults = [
                round(float(self.root.get(prefix + suffix, default)) * scale)
                for suffix, default, scale in [
                    ("Font", "3", 1),
                    ("Face", "0", 1),
                    ("Size", "10", 20),
                    ("Color", "3", 1),
                ]
            ]
            packed += codec.property_bytes(code, struct.pack("<HHHH", *defaults))
        stationery = binary[:28] + packed + binary[28:]
        decoded = ET.fromstring(codec.from_cdx(stationery))
        if [s.text for s in decoded.iter("s")] != [s.text for s in self.root.iter("s")]:
            raise ValueError(f"CDS changed source text: {self.name}")
        path.with_suffix(".cds").write_bytes(stationery)
        metadata = dict(case_metadata.get(self.name, {}))
        if "known_issue" in metadata:
            metadata["historical_failure"] = metadata.pop("known_issue")
        cases.append(
            dict(
                id=self.name,
                description=self.description,
                **metadata,
                paths=[
                    str(path.with_suffix(ext).relative_to(ROOT))
                    for ext in [".cdxml", ".cdx", ".cds"]
                ],
            )
        )


d = Drawing(
    "01-hidden-graphic", "Visible=no on a red rectangle; blue rectangle is a visible control"
)
d.graphic(Visible="no", color="4")
d.graphic(BoundingBox="130 30 200 80", color="5")
d.save()

d = Drawing(
    "02-hidden-caption", "Visible=no on caption; ordinary second caption is a visible control"
)
d.caption("HIDDEN TEXT", Visible="no", p="30 50")
d.caption("visible control", p="30 100")
d.save()

d = Drawing("03-hidden-arrow", "Visible=no on an ordinary modern full-head arrow")
d.arrow(Visible="no", color="4")
d.arrow(Tail3D="30 100 0", Head3D="150 100 0")
d.save()

points = "30 80 30 80 55 30 80 30 105 80 130 130 155 130 180 80 180 80"
d = Drawing("04-hidden-modern-spline", "Visible=no on a two-segment curve with modern heads")
d.add(
    d.page,
    "curve",
    CurvePoints=points,
    ArrowheadHead="Full",
    ArrowheadTail="HalfRight",
    ArrowheadType="Hollow",
    Visible="no",
    color="4",
)
d.caption("visible control", p="30 160")
d.save()

d = Drawing(
    "05-grouped-hidden-children",
    "Group contains separately hidden graphic and text; no nonstandard properties on group",
)
g = d.add(d.page, "group", Integral="yes")
d.graphic(g, Visible="no", color="4")
d.caption("HIDDEN GROUP CHILD", g, Visible="no", p="30 110")
d.caption("visible control", p="30 170")
d.save()

d = Drawing(
    "06-caption-defaults",
    "Unstyled s inherits Times New Roman 18pt bold italic caption defaults",
    CaptionFont="4",
    CaptionSize="18",
    CaptionFace="3",
)
d.caption("Default caption", defaults=True, p="30 70")
d.save()

d = Drawing(
    "07-label-defaults-no-text",
    "Methanol oxygen has no t child and inherits Times New Roman 18pt bold italic atom defaults",
    LabelFont="4",
    LabelSize="18",
    LabelFace="3",
)
d.methanol()
d.save()

d = Drawing(
    "08-label-defaults-with-text",
    "Matched methanol control with an unstyled OH label t/s inheriting identical atom defaults",
    LabelFont="4",
    LabelSize="18",
    LabelFace="3",
)
d.methanol(nested=True)
d.save()

d = Drawing(
    "09-document-line-width",
    "Rectangle inherits 2pt document line width; second rectangle overrides to 0.6pt",
    LineWidth="2",
    BoldWidth="4",
)
d.graphic(color="4")
d.graphic(BoundingBox="130 30 200 80", color="5", LineWidth="0.6")
d.save()

d = Drawing(
    "10-page-bounds-margins",
    "Single page 432x288pt with nondefault document print margins; documented page-layout limitation",
    PrintMargins="12 18 24 30",
)
d.page.set("BoundingBox", "0 0 432 288")
d.graphic(color="5")
d.save()

d = Drawing(
    "11-modern-multisegment-heads",
    "Two-segment cubic path with hollow full head and half-right tail",
)
d.add(
    d.page,
    "curve",
    CurvePoints=points,
    ArrowheadHead="Full",
    ArrowheadTail="HalfRight",
    ArrowheadType="Hollow",
    color="5",
)
d.save()

d = Drawing(
    "12-legacy-multisegment-heads",
    "Same two-segment cubic using SDK CurveType=8 (full arrow at end); no conflicting modern attributes",
)
d.add(d.page, "curve", CurvePoints=points, CurveType="8", color="5")
d.save()

(HERE / "manifest.json").write_text(
    json.dumps(
        dict(
            cases=cases,
            generation_command="PYTHONDONTWRITEBYTECODE=1 python3 tests/fixtures/chemdraw-import-semantics/generate.py",
            binary_encoder="reference/engine/cdx_exchange.py; generator-local native arrow object 0x8021 and zero-run text following SDK CDXString; full CDS adds packed LabelStyle/CaptionStyle defaults without removing artwork",
            validity_notes=[
                "Group Visible/color/LineWidth were excluded: SDK object property lists do not permit them on Group.",
                "Arrow object code is 0x8021, matching actual native ChemDraw and crates/io/src/exchange/mod.rs; the published 0x8027 alias fails the native ChemDraw arrow-display spot-check.",
                "SDK CurveType=8 is a full arrow at end; 16 means full arrow at start, 32/64 mean half arrows.",
                "Modern ArrowheadHead/Tail spelling follows the current repository schema/native fixtures.",
            ],
            sources=[
                "https://iupac.github.io/IUPAC-FAIRSpec/cdx_sdk/properties/Visible.htm",
                "https://iupac.github.io/IUPAC-FAIRSpec/cdx_sdk/Group.htm",
                "https://iupac.github.io/IUPAC-FAIRSpec/cdx_sdk/properties/LineWidth.htm",
                "https://iupac.github.io/IUPAC-FAIRSpec/cdx_sdk/properties/ForegroundColor.htm",
                "https://iupac.github.io/IUPAC-FAIRSpec/cdx_sdk/DataType/CDXString.htm",
                "https://iupac.github.io/IUPAC-FAIRSpec/cdx_sdk/properties/LabelStyleFace.htm",
                "https://iupac.github.io/IUPAC-FAIRSpec/cdx_sdk/properties/Curve_Type.htm",
                "https://iupac.github.io/IUPAC-FAIRSpec/cdx_sdk/properties/PrintMargins.htm",
            ],
        ),
        indent=2,
    )
    + "\n"
)
