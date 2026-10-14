"""Check documented presentation corrections against legacy output.

Chemistry and the rest of each response are still compared without modification.
Binary drawings use the independent Python codec, not the Rust implementation.
"""

import base64
import json
import math
import struct
import sys
import xml.etree.ElementTree as ET
from collections import Counter
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


def differences(actual, expected, path=""):
    """Report every XML difference, including attributes absent on either side."""
    path = path or f"/{expected.tag}"
    failures = []
    if actual.tag != expected.tag:
        failures.append(f"[tag] {path}: actual={actual.tag!r}, expected={expected.tag!r}")
    for name in sorted(actual.attrib.keys() | expected.attrib.keys()):
        a, e = actual.get(name), expected.get(name)
        if a != e:
            failures.append(f"[attribute] {path}@{name}: actual={a!r}, expected={e!r}")
    for name in ("text", "tail"):
        a, e = getattr(actual, name), getattr(expected, name)
        if a != e:
            failures.append(f"[{name}] {path}: actual={a!r}, expected={e!r}")
    if len(actual) != len(expected):
        failures.append(f"[children] {path}: actual={len(actual)}, expected={len(expected)}")
    for index in range(max(len(actual), len(expected))):
        a = actual[index] if index < len(actual) else None
        e = expected[index] if index < len(expected) else None
        if a is not None and e is not None:
            failures.extend(differences(a, e, f"{path}/{e.tag}[{index}]"))
        else:
            node = a if a is not None else e
            failures.extend(unmatched(node, f"{path}/{node.tag}[{index}]", a is not None))
    return failures


def unmatched(node, path, added):
    """Report every unmatched object and all its fields, including descendants."""
    category = "object-added" if added else "object-missing"
    failures = [
        f"[{category}] {path}: tag={node.tag!r}, attributes={node.attrib!r}, text={node.text!r}, tail={node.tail!r}"
    ]
    for index, child in enumerate(node):
        failures.extend(unmatched(child, f"{path}/{child.tag}[{index}]", added))
    return failures


def fail(failures):
    counts = Counter(item.split("]", 1)[0].removeprefix("[") for item in failures)
    summary = ", ".join(f"{name}={count}" for name, count in sorted(counts.items()))
    raise ValueError(
        f"Drawing comparison: {len(failures)} mismatches ({summary})\n" + "\n".join(failures)
    )


def dative_export_bonds(source_document):
    """Associate only the frozen writer's dashed order-5 bonds from source.

    worker.py assigns atoms index+3 and bonds atom_count+index+3. Persistent
    atom IDs establish directional endpoints; bonds have no persistent ID.
    Reject ambiguous or missing identities rather than using output chemistry.
    """
    if not isinstance(source_document, dict):
        return {}
    bonds = source_document.get("bonds")
    if not isinstance(bonds, list) or not any(
        isinstance(bond, dict)
        and type(bond.get("order")) is int
        and bond["order"] == 5
        and bond.get("display") == "dashed"
        for bond in bonds
    ):
        return {}
    atoms = source_document.get("atoms")
    if not isinstance(atoms, list):
        raise ValueError("[dative-source] Missing source atoms")

    def identifier(value):
        if type(value) is not int or not 0 < value <= 2**64 - 1:
            raise ValueError("[dative-source] Missing or invalid source atom identity")
        return value

    atom_ids = {}
    for index, atom in enumerate(atoms):
        if not isinstance(atom, dict):
            raise ValueError("[dative-source] Invalid source atom")
        identity = identifier(atom.get("id"))
        if identity in atom_ids:
            raise ValueError("[dative-source] Duplicate source atom identity")
        atom_ids[identity] = str(index + 3)
    pairs = set()
    result = {}
    for index, bond in enumerate(bonds):
        if not isinstance(bond, dict):
            raise ValueError("[dative-source] Invalid source bond")
        begin, end = identifier(bond.get("a")), identifier(bond.get("b"))
        if begin == end or begin not in atom_ids or end not in atom_ids:
            raise ValueError("[dative-source] Missing or self source bond endpoints")
        pair = tuple(sorted((begin, end)))
        if pair in pairs:
            raise ValueError("[dative-source] Duplicate source bond identity")
        pairs.add(pair)
        if (
            type(bond.get("order")) is int
            and bond["order"] == 5
            and bond.get("display") == "dashed"
        ):
            result[str(len(atoms) + index + 3)] = (atom_ids[begin], atom_ids[end])
    return result


def compare(actual, expected, format, source_atoms=None, source_document=None):
    changed = 0
    failures = []
    if format == "cdx":
        actual, changed = current_cdx(base64.b64decode(actual, validate=True))
        expected = from_cdx(base64.b64decode(expected, validate=True))
    actual = ET.fromstring(actual)
    expected = ET.fromstring(expected)
    source_dative = dative_export_bonds(source_document)
    # The old writer widens f32 before multiplying by 100, yielding values
    # such as 18.000000715255737 instead of 18. Only the document percentage
    # may differ, and both spellings must represent exactly the same f32.
    spacing = actual.get("BondSpacing")
    legacy_spacing = expected.get("BondSpacing")
    if spacing is not None and legacy_spacing is not None and spacing != legacy_spacing:
        try:
            a, e = float(spacing), float(legacy_spacing)
            same = (
                math.isfinite(a)
                and math.isfinite(e)
                and struct.pack("<f", a) == struct.pack("<f", e)
            )
        except (ValueError, OverflowError):
            same = False
        if same:
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
    actual_nodes = list(actual.iter())
    expected_nodes = list(expected.iter())
    if len(actual_nodes) != len(expected_nodes):
        failures.append(f"[objects] actual={len(actual_nodes)}, expected={len(expected_nodes)}")
    parents = {child: parent for parent in expected.iter() for child in parent}
    # The independent legacy writer assigns atom XML IDs from source order
    # (worker.py), not the document's persistent IDs or returned atom chemistry.
    actual_atom_ids = Counter(node.get("id") for node in actual.iter("n"))
    expected_atom_ids = Counter(node.get("id") for node in expected.iter("n"))
    actual_bond_ids = Counter(node.get("id") for node in actual.iter("b"))
    expected_bond_ids = Counter(node.get("id") for node in expected.iter("b"))
    zero_hydrogens = {
        str(index + 3)
        for index, atom in enumerate(source_atoms or [])
        if atom.get("no_implicit") is True
        and type(atom.get("explicit_h")) is int
        and atom["explicit_h"] == 0
        and type(atom.get("charge")) is int
        and isinstance(atom.get("element"), str)
        and (atom["charge"] != 0 or atom["element"] != "C")
    }
    for index, (a, e) in enumerate(zip(actual_nodes, expected_nodes)):
        location = f"object[{index}]/{e.tag}[id={e.get('id')!r}]"
        if (
            a.tag == e.tag == "b"
            and a.get("id") == e.get("id")
            and e.get("id") in source_dative
            and actual_bond_ids[e.get("id")] == expected_bond_ids[e.get("id")] == 1
            and (e.get("B"), e.get("E")) == source_dative[e.get("id")]
            and all(
                actual_atom_ids[endpoint] == expected_atom_ids[endpoint] == 1
                for endpoint in source_dative[e.get("id")]
            )
            and e.get("Order") == "1"
            and a.get("Order") == "dative"
            and a.get("Display") == e.get("Display") == "Dash"
            and {key: value for key, value in a.attrib.items() if key != "Order"}
            == {key: value for key, value in e.attrib.items() if key != "Order"}
        ):
            # #300 / 8fbf166c removes only the legacy dashed-dative Order=1
            # override. Preserve direction, Dash and every other attribute.
            e.set("Order", "dative")
            changed += 1
        if (
            a.tag == e.tag == "n"
            and a.get("id") == e.get("id")
            and e.get("id") in zero_hydrogens
            and actual_atom_ids[e.get("id")] == expected_atom_ids[e.get("id")] == 1
            and a.get("NumHydrogens") != e.get("NumHydrogens")
        ):
            # #319 adds only this exact attribute to qualifying source atoms.
            # A legacy value, a nonzero/current alternate spelling, or any
            # other atom, style, geometry, label or structure change still fails.
            if "NumHydrogens" in e.attrib or a.get("NumHydrogens") != "0":
                failures.append(
                    f"[zero-hydrogen] {location}: legacy NumHydrogens must be absent and actual exactly '0'"
                )
            else:
                e.set("NumHydrogens", "0")
                changed += 1
        if format == "cdxml" and a.tag == e.tag == "color":
            for name in {"r", "g", "b"} & a.attrib.keys() & e.attrib.keys():
                if a.get(name) == e.get(name):
                    continue
                try:
                    before, after = float(e.attrib[name]), float(a.attrib[name])
                except ValueError:
                    failures.append(f"[RGB] {location}@{name}: nonnumeric RGB component")
                    continue
                # ChemDraw truncates normalized RGB. The writer biases an
                # eight-place decimal by at most one unit to retain the same
                # 8-bit color, which must also encode to the same 16-bit CDX
                # channel. Match the independently checked drawing contract.
                if not (0 <= before <= 1 and 0 <= after <= 1):
                    failures.append(
                        f"[RGB] {location}@{name}: RGB component outside its finite range"
                    )
                    continue
                channel = round(before * 255)
                if (
                    math.trunc(after * 255) != channel
                    or round(after * 65535) != channel * 257
                    or abs(after - before) > 0.00000001000001
                ):
                    failures.append(
                        f"[RGB] {location}@{name}: RGB changed beyond the bounded precision correction"
                    )
                    continue
                e.set(name, a.attrib[name])
                changed += 1
        if (
            e.tag == "t"
            and e in parents
            and parents[e].tag in ("page", "group")
            and "CaptionLineHeight" in e.attrib
        ):
            if a.get("InterpretChemically") != "no" or "InterpretChemically" in e.attrib:
                failures.append(
                    f"[caption] {location}: caption must explicitly disable chemical interpretation"
                )
            else:
                e.set("InterpretChemically", "no")
                changed += 1
        if format == "cdx":
            for name in {"LineHeight", "CaptionLineHeight", "LabelLineHeight"} & e.attrib.keys():
                try:
                    legacy = float(e.attrib[name])
                    fixed = float(a.attrib[name])
                except (ValueError, KeyError):
                    continue  # Special variable/automatic spellings compare unchanged.
                # The legacy encoder rounded whole points, losing fractional
                # precision. The new unit is 1/20pt: it must land in exactly
                # the same legacy rounding bucket. Captured ChemDraw fixtures
                # and dedicated byte tests verify the finer precision itself.
                if not math.isfinite(fixed) or round(fixed / 20) != legacy:
                    failures.append(
                        f"[line-height] {location}@{name}: line height changed beyond the unit/precision correction"
                    )
                    continue
                e.set(name, a.attrib[name])
                changed += 1
    failures.extend(differences(actual, expected))
    if not changed:
        failures.append(
            "[contract] No documented hidden-dummy, zero-hydrogen, dative-order, bond-spacing, RGB, caption, or arrow correction"
        )
    if failures:
        fail(failures)


def main():
    request = json.load(sys.stdin)
    compare(
        request["actual"],
        request["expected"],
        request["format"],
        request.get("source_atoms"),
        request.get("source_document"),
    )


if __name__ == "__main__":
    main()
