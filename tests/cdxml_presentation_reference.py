"""Direct original style/text functions, without Rust-derived expectations."""

import copy
import inspect
import itertools
import json
import math
import random
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

from rdkit import rdBase

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from engine import drawing_styles, worker


def xml(attrs=None, runs=None, text_attrs=None, fonts=(), colors=None):
    root = ET.Element("CDXML", attrs or {})
    table = ET.SubElement(root, "fonttable")
    for font in fonts:
        ET.SubElement(table, "font", font)
    if colors is not None:
        table = ET.SubElement(root, "colortable")
        for color in colors:
            ET.SubElement(table, "color", color)
    text = ET.SubElement(ET.SubElement(root, "page"), "t", text_attrs or {})
    for value, style in runs if runs is not None else [("text", {})]:
        ET.SubElement(text, "s", style).text = value
    return ET.tostring(root, encoding="unicode")


def emit(name, source, mode="text", defaults=None, atom=False, element_xml=None):
    root = ET.fromstring(source)
    before = ET.tostring(root)
    palette = expected = error = error_type = None
    try:
        if mode == "style":
            expected = drawing_styles.from_cdxml(root)
        else:
            reader = worker.cdxml_text_reader(root)
            palette = inspect.getclosurevars(reader).nonlocals["colors"]
            if mode == "palette":
                expected = palette
            else:
                element = (
                    ET.fromstring(element_xml) if element_xml is not None else next(root.iter("t"))
                )
                before_element = ET.tostring(element)
                before_defaults = copy.deepcopy(defaults)
                value, format = reader(element, defaults, atom)
                expected = dict(text=value, format=format)
                assert before_element == ET.tostring(element)
                assert before_defaults == defaults
    except (ValueError, OverflowError) as exc:
        error, error_type = str(exc), type(exc).__name__
    assert before == ET.tostring(root)
    print(
        json.dumps(
            dict(
                name=name,
                xml=source,
                mode=mode,
                defaults=defaults,
                atom=atom,
                element_xml=element_xml,
                palette=palette,
                expected=expected,
                error=error,
                error_type=error_type,
            )
        )
    )


def main():
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion, defaults=drawing_styles.DEFAULT)))
    emit("default-no-attributes", xml(), "style")
    base = {"BondLength": "14.4"}
    emit("exact-default", xml(base), "style")
    for name, source in [
        ("comment", "<!-- <!ENTITY inert 'value'> -->" + xml(base)),
        ("cdata", "<CDXML><t><s><![CDATA[<!ENTITY inert 'value'>]]></s></t></CDXML>"),
        ("processing-instruction", "<?example <!ENTITY inert 'value'> ?>" + xml(base)),
        ("quoted-doctype", '<!DOCTYPE CDXML SYSTEM "unused<!ENTITY literal>">' + xml(base)),
        ("doctype-comment", "<!DOCTYPE CDXML [<!-- <!ENTITY inert 'value'> -->]>" + xml(base)),
    ]:
        for mode in ["style", "text", "palette"]:
            emit(f"inert-entity/{name}/{mode}", source, mode)
    emit("style-does-not-read-palette", xml(base, colors=[dict(r="nan")]), "style")
    emit("text-does-not-read-drawing-style", xml({"BondLength": "bad", "LineWidth": "bad"}))
    for attribute, minimum, maximum, default in [
        ("BondLength", 5, 100, 14.4),
        ("LabelSize", 4, 144, 10),
        ("LineWidth", 0.1, 6, 0.6),
        ("BoldWidth", 0.1, 12, 2),
        ("MarginWidth", 0, 12, 1.6),
        ("HashSpacing", 0.3, 12, 2.5),
        ("BondSpacing", 5, 40, 18),
    ]:
        values = [
            minimum,
            maximum,
            default,
            math.nextafter(minimum, -math.inf),
            math.nextafter(minimum, math.inf),
            math.nextafter(maximum, -math.inf),
            math.nextafter(maximum, math.inf),
            math.nextafter(default, -math.inf),
            math.nextafter(default, math.inf),
            0,
            -1,
            1e300,
        ]
        for value in [
            *map(str, values),
            "nan",
            "inf",
            "-Infinity",
            "",
            "bad",
            " 10 ",
            "1_0",
            "١٠",
            "1__0",
        ]:
            emit(f"style/{attribute}/{value}", xml({**base, attribute: value}), "style")
    for name in [
        "Arial",
        "",
        " ",
        "日本語",
        "a" * 256,
        "a" * 257,
        "日" * 128,
        "日" * 256,
        "日" * 257,
    ]:
        emit(
            f"style-font/{name}",
            xml(base | {"LabelFont": "7"}, fonts=[dict(id="7", name=name)]),
            "style",
        )
    for fonts in [
        [],
        [dict(name="Missing ID")],
        [dict(id="")],
        [dict(id="7")],
        [dict(id="7", name="First"), dict(id="7", name="Last")],
        [dict(name="First"), dict(name="Last")],
    ]:
        for selected in [None, "", "7", "007", "missing"]:
            attrs = base if selected is None else base | {"LabelFont": selected}
            emit(f"font-lookup-style/{fonts}/{selected}", xml(attrs, fonts=fonts), "style")
            attrs = {} if selected is None else {"CaptionFont": selected}
            emit(f"font-lookup-text/{fonts}/{selected}", xml(attrs, fonts=fonts))
    for line, bold in [("6", "5.999999999999999"), ("0.6", "0.5999999999999999"), ("6", "12")]:
        emit(
            f"style-bold/{line}/{bold}", xml(base | {"LineWidth": line, "BoldWidth": bold}), "style"
        )
    emit("style-error-order", xml(base | {"BondLength": "0", "HashSpacing": "bad"}), "style")
    emit(
        "style-font-after-numbers",
        xml(base | {"LabelFont": "7", "LabelSize": "0"}, fonts=[dict(id="7", name="")]),
        "style",
    )
    for face, atom in itertools.product(range(-5, 256), [False, True]):
        emit(f"face/{face}/{atom}", xml(runs=[("H₂O α", dict(face=str(face)))]), atom=atom)
    for field, values in [
        ("face", ["", "bad", "1.0", "1_0", "١", "1" * 100, "0" * 4301]),
        (
            "size",
            [
                "",
                "bad",
                "nan",
                "inf",
                "-inf",
                "3.9999999999999996",
                "4",
                "144",
                "144.00000000000003",
                " 10 ",
                "١٠",
            ],
        ),
        ("color", ["", "bad", "-1", "0", "1", "2", "3", "4", "0_3", "٣", "1" * 100]),
    ]:
        for value in values:
            emit(f"run-scalar/{field}/{value[:40]}", xml(runs=[("A", {field: value})]))
    for alignment, atom in itertools.product(
        ["Left", "Center", "Right", "Full", "", "left", "Unknown"], [False, True]
    ):
        emit(
            f"alignment/{alignment}/{atom}", xml(text_attrs={"Justification": alignment}), atom=atom
        )
    for height in [
        "auto",
        "variable",
        "0",
        "1",
        "0.0",
        "1.0",
        "Auto",
        "",
        "bad",
        "nan",
        "inf",
        "7.999999999999999",
        "8",
        "30",
        "30.000000000000004",
    ]:
        emit(f"height/{height}", xml(text_attrs={"LineHeight": height}))
    for width in [
        "0",
        "-0.0",
        "9.999999999999998",
        "10",
        "2000",
        "2000.0000000000002",
        "nan",
        "inf",
        "bad",
        "-10",
    ]:
        emit(f"width/{width}", xml(text_attrs={"WordWrapWidth": width}))
    for rotation in ["0", "-0", "1e-300", "1", "360", "nan", "inf", "bad"]:
        emit(f"rotation/{rotation}", xml(text_attrs={"RotationAngle": rotation}))
    for atom in [False, True]:
        prefix = "Label" if atom else "Caption"
        for root_level, defaults_level, element_level, run_level in itertools.product(
            [False, True], repeat=4
        ):
            root = (
                {
                    prefix + "Size": "11",
                    prefix + "Face": "1",
                    prefix + "Font": "1",
                    prefix + "Color": "0",
                }
                if root_level
                else {}
            )
            defaults = (
                {
                    prefix + "Size": "12",
                    prefix + "Face": "2",
                    prefix + "Font": "2",
                    prefix + "Color": "1",
                }
                if defaults_level
                else {}
            )
            element = (
                {
                    prefix + "Size": "13",
                    prefix + "Face": "4",
                    prefix + "Font": "3",
                    prefix + "Color": "2",
                }
                if element_level
                else {}
            )
            run = dict(size="14", face="96", font="4", color="3") if run_level else {}
            emit(
                f"inheritance/{atom}/{root_level}/{defaults_level}/{element_level}/{run_level}",
                xml(
                    root,
                    [("A", run)],
                    element,
                    fonts=[dict(id=str(i), name=f"Font{i}") for i in range(1, 5)],
                ),
                defaults=defaults,
                atom=atom,
            )
        emit(
            f"synthetic-label/{atom}",
            xml({prefix + "Size": "11"}),
            defaults={prefix + "Face": "3"},
            atom=atom,
            element_xml="<t><s/></t>",
        )
    for attrs in [
        dict(CaptionLineHeight="auto", LineHeight="bad"),
        dict(CaptionJustification="Right", Justification="bad"),
        dict(LabelLineHeight="auto", LineHeight="bad"),
        dict(LabelJustification="Right", Justification="bad"),
    ]:
        for atom in [False, True]:
            emit(f"prefixed-paragraph/{attrs}/{atom}", xml(text_attrs=attrs), atom=atom)
    for content in [
        "",
        "<s/>",
        "<s></s><s face='1'/>",
        "<s>A</s><s face='1'>日β🧪</s><s>C</s>",
        "<s>A<![CDATA[B]]><!-- comment -->C<?test x?>D</s>",
        "<s>A<child>B</child>C</s>",
        "<s>&#13;&#10;A&#13;B</s><s face='1'>日</s>",
        "<s>A&#13;</s><s face='1'>&#10;B</s>",
        "<s>A</s><s size='10.000000000000002'>B</s>",
        "<s>A</s><s face='1'>B</s><s face='1'>C</s>",
        "<x:s xmlns:x='urn:other'>ignored</x:s>",
        "<s>A</s><group><s>ignored</s></group>",
    ]:
        emit(f"runs/{content}", f"<CDXML><page><t>{content}</t></page></CDXML>")
    for family in ["", " ", "日" * 100, "a" * 256, "a" * 257]:
        emit(
            f"text-document-font/{family}",
            xml({"CaptionFont": "1"}, fonts=[dict(id="1", name=family)]),
        )
    for integer in range(-2, 258):
        midpoint = (integer + 0.5) / 255
        for value in [
            math.nextafter(midpoint, -math.inf),
            midpoint,
            math.nextafter(midpoint, math.inf),
        ]:
            colors = [dict(r=repr(value), g="0.5", b="1"), dict(r="0", g="0", b="0")]
            for used in [False, True]:
                emit(
                    f"palette-round/{integer}/{value}/{used}",
                    xml(runs=[("A", dict(color="2" if used else "3"))], colors=colors),
                )
    for value in [
        "nan",
        "inf",
        "-inf",
        "1e308",
        "bad",
        "",
        "1e300",
        "-1e300",
        "1.0001",
        "-0.1",
        "-0.0",
        " 0.5 ",
    ]:
        colors = [dict(r=value), dict(r="0", g="0", b="0")]
        emit(f"palette-construction/{value}", xml(colors=colors), "palette")
        emit(f"palette-unused/{value}", xml(colors=colors))
        emit(f"palette-used/{value}", xml(runs=[("A", dict(color="2"))], colors=colors))
        emit(
            f"palette-empty-unused-run/{value}",
            xml(runs=[("A", {}), ("", dict(color="2"))], colors=colors),
        )
    for colors in [None, [], [{}], [{}, {}]]:
        emit(f"palette-fallback/{colors}", xml(colors=colors), "palette")
    for attrs in [
        dict(face="8", color="bad", size="bad"),
        dict(color="-1", size="bad"),
        dict(color="bad", size="bad"),
    ]:
        emit(f"run-error-precedence/{attrs}", xml(runs=[("", attrs)]))
    for attrs in [
        dict(Justification="bad", LineHeight="bad", WordWrapWidth="bad", RotationAngle="bad"),
        dict(LineHeight="0", WordWrapWidth="bad", RotationAngle="bad"),
        dict(LineHeight="4", WordWrapWidth="bad", RotationAngle="bad"),
        dict(WordWrapWidth="-1", RotationAngle="bad"),
    ]:
        emit(f"paragraph-error-precedence/{attrs}", xml(text_attrs=attrs))
    rng = random.Random(8291)
    for index in range(1000):
        size = rng.uniform(4, 144)
        spacing = rng.choice([0.8, 1.2, 3.0])
        height = math.nextafter(size * spacing, rng.choice([-math.inf, math.inf]))
        emit(
            f"paragraph-numeric/{index}",
            xml(runs=[("A", dict(size=repr(size)))], text_attrs={"LineHeight": repr(height)}),
        )
    for filename in [
        "formatted-label-chemdraw.cdxml",
        "atom-labels-chemdraw.cdxml",
        "grouped-aspirin-chemdraw.cdxml",
    ]:
        root = ET.parse(Path(__file__).parent / "fixtures" / filename).getroot()
        source = ET.tostring(root, encoding="unicode")
        emit(f"fixture-style/{filename}", source, "style")
        for index, node in enumerate(root.iter("t")):
            for atom in [False, True]:
                emit(
                    f"fixture-text/{filename}/{index}/{atom}",
                    source,
                    atom=atom,
                    element_xml=ET.tostring(node, encoding="unicode"),
                )


if __name__ == "__main__":
    main()
