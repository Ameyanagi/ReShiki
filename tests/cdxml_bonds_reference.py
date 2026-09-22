"""Independent original read_bonds observations, including native partial reads."""

import copy
import itertools
import json
import math
import random
import sys
import unicodedata
import xml.etree.ElementTree as ET
from pathlib import Path

from rdkit import Chem, RDLogger, rdBase

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from engine import bonds_exchange

COLORS = [[255, 255, 255], [0, 0, 0], [20, 40, 60], [80, 100, 120], [140, 160, 180]]
SCALE = 42 / 14.4


def drawing(attrs=None, fragment_id="30"):
    root = ET.Element("CDXML", BondLength="14.4")
    fragment = ET.SubElement(ET.SubElement(root, "page"), "fragment", id=fragment_id)
    ET.SubElement(fragment, "n", id="101", p="0 0")
    ET.SubElement(fragment, "n", id="109", p="14.4 0", Element="7")
    ET.SubElement(fragment, "b", {"id": "113", "B": "101", "E": "109", **(attrs or {})})
    return root


def molecule(fragment_id=30, numbers=(6, 7), positions=((0, 0), (1.5, 0))):
    mol = Chem.RWMol()
    for number in numbers:
        mol.AddAtom(Chem.Atom(number))
    conf = Chem.Conformer(len(numbers))
    for index, p in enumerate(positions):
        conf.SetAtomPosition(index, (*p, 0) if len(p) == 2 else p)
    mol.AddConformer(conf)
    mol.SetIntProp("CDX_FRAG_ID", fragment_id)
    return mol.GetMol()


def snapshot(mol):
    return dict(
        id=mol.GetIntProp("CDX_FRAG_ID"),
        numbers=[a.GetAtomicNum() for a in mol.GetAtoms()],
        positions=[
            dict(x=p.x, y=p.y, z=p.z)
            for p in (mol.GetConformer().GetAtomPosition(i) for i in range(mol.GetNumAtoms()))
        ],
    )


def emit(name, root, parts=None, scale=SCALE, colors=COLORS, parsed_xml=None, source=None):
    parts = [molecule()] if parts is None else parts
    tree_text = ET.tostring(root, encoding="unicode")
    text = tree_text if source is None else source
    before = [snapshot(part) for part in parts]
    try:
        expected = bonds_exchange.read_bonds(root, parts, scale, colors)
        error = error_type = None
    except (ValueError, KeyError, RuntimeError) as exc:
        expected, error, error_type = None, str(exc), type(exc).__name__
    assert ET.tostring(root, encoding="unicode") == tree_text
    assert before == [snapshot(part) for part in parts]
    bounded = expected is not None and any(not -32768 <= b["z_order"] <= 32767 for b in expected)
    print(
        json.dumps(
            dict(
                name=name,
                text=text,
                parts=before,
                scale=scale,
                colors=colors,
                expected=expected,
                error=error,
                error_type=error_type,
                layer_bound=bounded,
                parsed_xml=parsed_xml,
            )
        )
    )


def native(name, root, reverse_parts=False):
    chemical = ET.tostring(bonds_exchange.chemistry_xml(root), encoding="unicode")
    parts = list(Chem.MolsFromCDXML(chemical, sanitize=False, removeHs=False))
    if reverse_parts:
        parts.reverse()
    emit(name, root, parts, parsed_xml=None if reverse_parts else chemical)


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    plain = ET.tostring(drawing(), encoding="unicode")
    for name, source in [
        ("comment", "<!-- <!ENTITY inert 'value'> -->" + plain),
        ("cdata", plain.replace("</CDXML>", "<![CDATA[<!ENTITY inert 'value'>]]></CDXML>")),
        ("processing-instruction", "<?example <!ENTITY inert 'value'> ?>" + plain),
        ("quoted-doctype", '<!DOCTYPE CDXML SYSTEM "unused<!ENTITY literal>">' + plain),
        ("doctype-comment", "<!DOCTYPE CDXML [<!-- <!ENTITY inert 'value'> -->]>" + plain),
    ]:
        emit(f"inert-entity/{name}", ET.fromstring(source), source=source)
    displays = list(bonds_exchange.DISPLAY)
    for order, primary, secondary, position in itertools.product(
        [None, "1", "2", "3", "1.5", "hydrogen", "dative", "4"],
        [None, *displays],
        [None, "", *displays, "DottedHydrogen"],
        ["auto", "Left", "RIGHT", "center"],
    ):
        attrs = {"DoublePosition": position}
        for key, value in [("Order", order), ("Display", primary), ("Display2", secondary)]:
            if value is not None:
                attrs[key] = value
        emit(f"appearance/{order}/{primary}/{secondary}/{position}", drawing(attrs))
    for field, values in [
        ("Order", ["", "0", "1.0", "1,2", "2.5", "any", "unknown"]),
        ("Display", ["", "Unknown", "Solid ", "unknownEnd"]),
        ("Display2", ["Unknown", "Solid ", "unknownEnd"]),
        ("DoublePosition", ["", "Automatic", " left", "right ", "RIGHT\u0130"]),
        ("B", ["", "999", "00101"]),
        ("E", ["", "999", "00109"]),
    ]:
        for value, hydrogen in itertools.product(values, [False, True]):
            attrs = {field: value}
            if hydrogen:
                attrs["Display2" if field != "Display2" else "Order"] = (
                    "DottedHydrogen" if field != "Display2" else "hydrogen"
                )
            emit(f"invalid/{field}/{value}/{hydrogen}", drawing(attrs))
    for field in ["B", "E"]:
        root = drawing()
        del root.find(".//b").attrib[field]
        emit(f"missing/{field}", root)
    # Both inheritance and error precedence are observable.
    for color_level, z_level in itertools.product(range(5), repeat=2):
        root = drawing(dict(Display="WedgeEnd", DoublePosition="left"))
        chain = [root, root.find("page"), root.find(".//fragment"), root.find(".//b")]
        for index, node in enumerate(chain):
            if index < color_level:
                node.set("color", str(index + 1))
            if index < z_level:
                node.set("Z", str(20 + index))
            node.set("Display2", "Bold") if index < 3 else None
            node.set("DoublePosition", "center") if index < 3 else None
        emit(f"inheritance/{color_level}/{z_level}", root)
    for field, values in [
        (
            "color",
            [
                "-1",
                "0",
                "4",
                "5",
                "999999999999999999999999999999999999999999",
                "",
                "bad",
                "+3",
                " 3 ",
                "0_3",
                "_3",
                "3_",
                "3__0",
                "3.0",
            ],
        ),
        (
            "Z",
            [
                "-32769",
                "-32768",
                "0",
                "32767",
                "32768",
                "999999999999999999999999999999999999999999",
                "",
                "bad",
                "+7",
                " -7 ",
                "1_2",
                "_3",
                "3_",
                "3__0",
                "3.0",
            ],
        ),
    ]:
        for value in values:
            emit(f"scalar/{field}/{value}", drawing({field: value}))
    for attrs in [
        dict(Order="bad", color="bad", DoublePosition="bad", B="bad", Display="bad", Z="bad"),
        dict(color="bad", DoublePosition="bad", B="bad", Display="bad", Z="bad"),
        dict(DoublePosition="bad", B="bad", Display="bad", Z="bad"),
        dict(B="bad", Display="bad", Z="bad"),
        dict(Display="bad", Display2="worse", Z="bad"),
        dict(Display="bad", Z="bad"),
    ]:
        emit(f"precedence/{attrs}", drawing(attrs))
    for value in [
        "",
        "0",
        "0 0 0",
        "0 0 bad",
        "bad 0",
        "nan 0",
        "inf 0",
        "-Infinity 0",
        "1e309 0",
        "0_0 0",
        "0__0 0",
    ]:
        root = drawing()
        root.find(".//n").set("p", value)
        emit(f"position/{value}", root)
    for value in [
        "6",
        "7",
        "0",
        "118",
        "999999999999999999999999999999999999999999",
        "-1",
        "+6",
        "0_6",
        "_6",
        "6_",
        "",
        "bad",
    ]:
        root = drawing()
        root.find(".//n").set("Element", value)
        emit(f"element/{value}", root)
    assert unicodedata.unidata_version == "15.0.0"
    for zero in range(0x110000):
        if unicodedata.decimal(chr(zero), None) != 0:
            continue
        root = drawing(dict(color=chr(zero + 3), Z=chr(zero + 7)))
        root.find(".//n").set("Element", chr(zero + 6))
        root.find(".//n").set("p", f"{chr(zero)}.{chr(zero)} {chr(zero)}")
        emit(f"unicode-decimal/{zero:x}", root)
    for value in ["bad'quote", 'bad"quote', "bad\\quote", "\u00a0bad\u00a0", "⑥", "Ⅵ"]:
        emit(f"integer-error-spelling/{value}", drawing(dict(Z=value)))
    for point in [0xAD, 0x200D, 0x2028, 0xE000, 0xFDD0, 0x10FFFF, 0x301, 0xFE0F, 0x1ACF, 0x1E6C0]:
        value = f"bad{chr(point)}"
        for field in ["B", "Display", "Z"]:
            emit(f"unicode-error-spelling/{point:x}/{field}", drawing({field: value}))
    for field in ["color", "Z"]:
        for name, value in [
            ("long-invalid", "x" * 300),
            ("long-unicode-invalid", "⑥" * 300),
            ("digit-limit", "0" * 4301),
            ("underscored-limit", "1_" * 4300 + "1"),
            ("lexical-before-limit", "1" * 4301 + "x"),
        ]:
            emit(f"integer-limit/{field}/{name}", drawing({field: value}))
    for fid in ["30", "030", "+30", "-1", "4294967295", "0", "", "bad"]:
        for part_id in [30, -1, 0]:
            emit(f"fragment-id/{fid}/{part_id}", drawing(fragment_id=fid), [molecule(part_id)])
    root = drawing()
    del root.find(".//fragment").attrib["id"]
    emit("missing-fragment-id", root)
    emit("missing-parts", drawing(), [])
    emit("empty-drawing", ET.Element("CDXML"), [])
    emit("missing-fragment", ET.Element("CDXML"))
    root = drawing()
    fragment = root.find(".//fragment")
    fragment.clear()
    fragment.set("id", "30")
    emit("empty-fragment/no-parts", root, [])
    emit("empty-fragment/empty-part", root, [molecule(numbers=(), positions=())])
    for missing_id in [False, True]:
        root = drawing()
        nodes = list(root.iter("n"))
        nodes[1].set("id", "101")
        root.find(".//b").set("E", "101")
        if missing_id:
            for node in nodes:
                del node.attrib["id"]
            for name in ["B", "E"]:
                del root.find(".//b").attrib[name]
        emit(f"duplicate-node-id/{missing_id}", root)
    for same_element in [False, True]:
        root = drawing()
        nodes = list(root.iter("n"))
        nodes[1].set("p", "0 0")
        if same_element:
            nodes[1].set("Element", "6")
        emit(
            f"duplicate-coordinate/{same_element}",
            root,
            [molecule(numbers=(6, 6 if same_element else 7), positions=((0, 0), (0, 0)))],
        )
    # Exercise both sides of strict tolerance and cell boundaries independently
    # of the molecular parser's signed fixed-point coordinate conversion.
    for axis, origin, delta in itertools.product(
        [0, 1],
        [-1.0, -0.001, 0.0, 0.999, 1.0, 1e20, -1e20],
        [-0.021, -0.02, math.nextafter(-0.02, 0), 0.0, math.nextafter(0.02, 0), 0.02, 0.021],
    ):
        root = drawing()
        xy = [0.0, 0.0]
        xy[axis] = origin
        root.find(".//n").set("p", " ".join(map(str, xy)))
        native_xy = list(xy)
        native_xy[axis] += delta
        emit(
            f"tolerance/{axis}/{origin}/{delta}",
            root,
            [molecule(positions=((native_xy[0] / 28, -native_xy[1] / 28), (14.4 / 28, 0)))],
            scale=1,
        )
    rng = random.Random(887)
    for index in range(1000):
        root = drawing(
            dict(Display=rng.choice(displays), DoublePosition=rng.choice(["left", "right"]))
        )
        mol = molecule()
        if index % 2:
            mol = Chem.RenumberAtoms(mol, [1, 0])
            mol.SetIntProp("CDX_FRAG_ID", 30)
        fragment = root.find(".//fragment")
        children = list(fragment)
        rng.shuffle(children)
        fragment[:] = children
        emit(f"reordered/{index}", root, [mol])
    for reverse_parts, duplicate_id in itertools.product([False, True], repeat=2):
        root = drawing()
        second = copy.deepcopy(root.find(".//fragment"))
        if not duplicate_id:
            second.set("id", "31")
        root.find("page").append(second)
        parts = [molecule(), molecule(30 if duplicate_id else 31)]
        if reverse_parts:
            parts.reverse()
        emit(f"reordered-fragments/{reverse_parts}/{duplicate_id}", root, parts)
    root = drawing()
    duplicate = copy.deepcopy(root.find(".//fragment"))
    duplicate.find("n").set("p", "100 0")
    duplicate.findall("n")[1].set("p", "114.4 0")
    duplicate.find("b").set("Display", "Bold")
    root.find("page").append(duplicate)
    emit(
        "duplicate-fragment-last-wins",
        root,
        [molecule(positions=((100 * SCALE / 28, 0), (114.4 * SCALE / 28, 0)))],
    )
    for target in ["fragment", "n", "b"]:
        root = drawing()
        root.find(f".//{target}").tag = f"{{urn:other}}{target}"
        emit(f"namespaced-element/{target}", root)
    root = drawing()
    root.find(".//b").set("{urn:other}color", "bad")
    emit("namespaced-attribute", root)
    root = drawing()
    del root.find(".//n").attrib["p"]
    emit("missing-atom-position", root)
    for order, primary in itertools.product(
        ["1", "2", "3", "1.5", "hydrogen", "dative", "4"], displays
    ):
        native(f"native/{order}/{primary}", drawing(dict(Order=order, Display=primary)))
    root = drawing()
    bad = copy.deepcopy(root.find(".//fragment"))
    bad.set("id", "31")
    bad.find("b").set("E", "999")
    root.find("page").append(bad)
    native("native-partial-import", root)
    for filename in [
        "reference-ethanol.cdxml",
        "bond-styles-chemdraw.cdxml",
        "grouped-aspirin-chemdraw.cdxml",
        "aromatic-circle-native.cdxml",
    ]:
        root = ET.parse(Path(__file__).parent / "fixtures" / filename).getroot()
        native(f"fixture/{filename}", root)
        native(f"fixture-reversed/{filename}", root, reverse_parts=True)


if __name__ == "__main__":
    main()
