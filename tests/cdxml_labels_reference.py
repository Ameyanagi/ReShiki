"""Direct original read_labels + original text callback, including callback order."""

import copy
import itertools
import json
import math
import random
import struct
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from rdkit import Chem, rdBase

from engine.labels_exchange import read_labels
from engine.worker import cdxml_text_reader


def emit(name, root, atoms=None, positions=None, bonds=None, scale=1, restriction=None):
    text = root if isinstance(root, str) else ET.tostring(root, encoding="unicode")
    atoms = atoms if atoms is not None else [6]
    positions = positions if positions is not None else [(0, 0, 0)] * len(atoms)
    bonds = bonds if bonds is not None else []
    mol = Chem.RWMol()
    for element in atoms:
        mol.AddAtom(Chem.Atom(element))
    conf = Chem.Conformer(len(atoms))
    for i, p in enumerate(positions):
        conf.SetAtomPosition(i, p)
    mol.AddConformer(conf)
    expected = failure = failure_type = None
    calls = []
    try:
        root = ET.fromstring(text)
        before = ET.tostring(root)
        ordinals = {node: i for i, node in enumerate(root.iter())}
        original = cdxml_text_reader(root)

        def read_text(node, defaults):
            calls.append(dict(source=ordinals[node], attributes=dict(defaults)))
            return original(node, defaults)

        base = dict(atoms=[], bonds=[dict(a=a, b=b, order=1, kept=True) for a, b in bonds])
        objects = {}
        read_labels(root, mol, base, scale, read_text, objects)
        assert ET.tostring(root) == before
        expected = dict(
            atom_labels=base["atom_labels"],
            atoms=base["atoms"],
            bonds=[
                dict(index=i, indicator=bond["indicator"])
                for i, bond in enumerate(base["bonds"])
                if "indicator" in bond
            ],
            objects=[dict(source=ordinals[node], atoms=ids) for node, ids in objects.items()],
        )
        # Patches are returned in first source encounter order, not base order.
        first = {}
        nodes = {node.get("id"): ids[0] for node, ids in objects.items()}
        for node in root.iter("b"):
            pair = {nodes.get(node.get("B")), nodes.get(node.get("E"))}
            index = next(i for i, (a, b) in enumerate(bonds) if {a, b} == pair)
            first.setdefault(index, len(first))
        expected["bonds"].sort(key=lambda patch: first[patch["index"]])
    except (ValueError, KeyError, IndexError, OverflowError, ET.ParseError) as exc:
        failure, failure_type = str(exc), type(exc).__name__
    print(
        json.dumps(
            dict(
                name=name,
                text=text,
                atoms=atoms,
                positions=[dict(zip(("x", "y", "z"), p, strict=True)) for p in positions],
                bonds=bonds,
                scale=scale,
                expected=expected,
                failure=failure,
                failure_type=failure_type,
                calls=calls,
                restriction=restriction,
            )
        )
    )


def drawing(root_attrs=None, node_attrs=None):
    root = ET.Element("CDXML", root_attrs or {})
    group = ET.SubElement(ET.SubElement(root, "page"), "group")
    fragment = ET.SubElement(group, "fragment")
    attrs = dict(id="a", p="0 0")
    attrs.update(node_attrs or {})
    node = ET.SubElement(fragment, "n", {k: v for k, v in attrs.items() if v is not None})
    return root, group, fragment, node


def indicator(owner, name="stereo", text="(R)", bounds="0 2 4 6", text_attrs=None, runs=None):
    tag = ET.SubElement(owner, "objecttag", {} if name is None else dict(Name=name))
    attrs = {} if bounds is None else dict(BoundingBox=bounds)
    attrs.update(text_attrs or {})
    t = ET.SubElement(tag, "t", attrs)
    for value, attrs in runs if runs is not None else [(text, {})]:
        s = ET.SubElement(t, "s", attrs)
        s.text = value
    return tag


def main():
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    flags = (
        "ShowTerminalCarbonLabels",
        "ShowNonTerminalCarbonLabels",
        "HideImplicitHydrogens",
        "ShowAtomStereo",
        "ShowAtomNumber",
    )
    for i, values in enumerate(itertools.product(("yes", "no", "bad"), repeat=len(flags))):
        root, _, _, node = drawing(dict(zip(flags, values, strict=True)), dict(AtomNumber="17"))
        indicator(node, "number", "42")
        indicator(node)
        emit(f"root-settings-{i}", root)
    for flag, root_value, group_value, node_value in itertools.product(
        flags, ("yes", "no", "bad"), (None, "yes", "no"), (None, "yes", "no")
    ):
        root, group, _, node = drawing({flag: root_value}, dict(AtomNumber="α"))
        if group_value is not None:
            group.set(flag, group_value)
        if node_value is not None:
            node.set(flag, node_value)
        emit(f"inherited-{flag}-{root_value}-{group_value}-{node_value}", root)
    for alignment, position in itertools.product(
        (None, "Left", "bad"), (None, "Auto", "Best", "Left", "Right", "Above", "Below", "bad", "")
    ):
        attrs = {
            k: v
            for k, v in dict(LabelAlignment=alignment, LabelDisplay=position).items()
            if v is not None
        }
        emit(f"placement-{alignment}-{position}", drawing(node_attrs=attrs)[0])
    for name, number, stereo, value in itertools.product(
        (None, "number", "stereo", "unknown"),
        ("yes", "no"),
        ("yes", "no"),
        ("", "R", "α" * 32, "α" * 33, "\n", "a\x7fb", "😀" * 32),
    ):
        root, _, _, node = drawing(dict(ShowAtomNumber=number, ShowAtomStereo=stereo))
        indicator(node, name, value)
        emit(f"indicator-{name}-{number}-{stereo}-{value!r}", root)
    for number in ("", "5", "0", "x" * 100, "a\nb", "😀" * 33):
        emit(
            f"bare-number-{number!r}",
            drawing(dict(ShowAtomNumber="yes"), dict(AtomNumber=number))[0],
        )
    for bounds, visible in itertools.product(
        (
            None,
            "",
            "1 2",
            "bad",
            "nan 0 1 2",
            "inf 0 1 2",
            "0 0 1 2 3",
            "0 0 1e300 2",
            "１ ٢ ٣ 4",
            "0_0 0 4 6",
        ),
        ("yes", "no"),
    ):
        root, _, _, node = drawing(dict(ShowAtomStereo=visible))
        indicator(node, bounds=bounds)
        emit(f"bounds-{bounds}-{visible}", root)
    for face, visible in itertools.product(
        ("0", "1", "2", "3", "4", "32", "64", "96", "8", "bad"), ("yes", "no")
    ):
        root, _, _, node = drawing(dict(ShowAtomStereo=visible))
        indicator(node, runs=[("R", dict(face=face))])
        emit(f"face-{face}-{visible}", root)
    for runs in (
        [],
        [("R", {}), ("S", {})],
        [("R", {}), ("S", dict(face="1"))],
        [("R", dict(face="1")), ("", {})],
    ):
        root, _, _, node = drawing(dict(ShowAtomStereo="yes"))
        indicator(node, runs=runs)
        emit(f"runs-{runs}", root)
    for level, repeats, count in itertools.product(("n", "b", "group", "t"), (1, 2), (0, 1, 2)):
        root, group, fragment, node = drawing(dict(ShowAtomStereo="yes", ShowBondStereo="yes"))
        ET.SubElement(fragment, "n", id="b", p="10 0")
        bond = ET.SubElement(fragment, "b", B="a", E="b")
        owner = {"n": node, "b": bond, "group": group, "t": ET.SubElement(node, "t")}[level]
        for _ in range(repeats):
            tag = indicator(owner)
            for child in list(tag):
                tag.remove(child)
            for _ in range(count):
                ET.SubElement(ET.SubElement(tag, "t"), "s").text = "R"
        emit(
            f"ownership-{level}-{repeats}-{count}",
            root,
            [6, 6],
            [(0, 0, 0), (10 / 28, 0, 0)],
            [[1, 2]],
        )
    for p in (None, "", "0", "0 0 1", "nan 0", "inf 0", "bad 0", "０ ٠", "0_0 -0"):
        emit(f"node-position-{p}", drawing(node_attrs=dict(p=p))[0])
    for element in (None, "６", "+006", "6.0", "6e0", "-1", "119", "bad"):
        emit(f"element-{element}", drawing(node_attrs=dict(Element=element))[0])
    for delta in (-0.02000001, -0.02, -0.01999999, 0, 0.01999999, 0.02, 0.02000001):
        emit(f"tolerance-{delta}", drawing()[0], positions=[(delta / 28, 0, 0)])
    emit("ambiguous-overlap", drawing()[0], [6, 6])
    emit("different-element-overlap", drawing()[0], [8, 6])
    emit("empty-molecule", drawing()[0], [])
    emit("absent-id", drawing(node_attrs=dict(id=None))[0])
    root, _, fragment, node = drawing(dict(ShowAtomStereo="yes", ShowAtomNumber="yes"))
    indicator(node, "number", "first", bounds="2 0 6 2")
    later = ET.SubElement(fragment, "n", id="a", p="10 0")
    indicator(later, "stereo", "S")
    emit("duplicate-id-last-target", root, [6, 6], [(0, 0, 0), (10 / 28, 0, 0)])
    for endpoints in (("a", "missing"), (None, None), ("a", "b"), ("b", "a")):
        root, _, fragment, _ = drawing(dict(ShowBondStereo="yes"))
        ET.SubElement(fragment, "n", id="b", p="10 0")
        ET.SubElement(
            fragment,
            "b",
            {k: v for k, v in zip(("B", "E"), endpoints, strict=True) if v is not None},
        )
        emit(
            f"bond-reference-{endpoints}",
            root,
            [6, 6],
            [(0, 0, 0), (10 / 28, 0, 0)],
            [[2, 1], [1, 2]],
        )
    root, _, fragment, node = drawing(dict(ShowAtomStereo="yes", ShowBondStereo="yes"))
    other = ET.SubElement(fragment, "n", id="b", p="10 0")
    first_bond = ET.SubElement(fragment, "b", B="b", E="a")
    second_bond = ET.SubElement(fragment, "b", B="a", E="b", ShowBondStereo="no")
    indicator(first_bond)
    indicator(node, text="first")
    indicator(other, text="second")
    indicator(second_bond, bounds="bad")
    emit(
        "node-callbacks-before-bonds-and-last-bond-reset",
        root,
        [6, 6],
        [(0, 0, 0), (10 / 28, 0, 0)],
        [[1, 2], [2, 1]],
    )
    rng = random.Random(312)
    for i in range(300):
        x, y = rng.uniform(-1000, 1000), rng.uniform(-1000, 1000)
        scale = rng.choice((1, 0.5, -2, 42 / 14.4))
        root, _, _, node = drawing(dict(ShowAtomStereo="yes"), dict(p=f"{x:.17g} {y:.17g}"))
        indicator(node, bounds=" ".join(f"{rng.uniform(-1000, 1000):.17g}" for _ in range(4)))
        emit(f"geometry-{i}", root, positions=[(x * scale / 28, -y * scale / 28, 0)], scale=scale)
    for i in range(250):
        bits = rng.randrange(0x3F000000, 0x461C0000)
        low = struct.unpack("!f", struct.pack("!I", bits))[0]
        high = struct.unpack("!f", struct.pack("!I", bits + 1))[0]
        midpoint = (low + high) / 2
        for toward in (-math.inf, None, math.inf):
            x = math.nextafter(midpoint, toward) if toward is not None else midpoint
            root, _, _, node = drawing(dict(ShowAtomNumber="yes"))
            indicator(node, "number", "7", bounds=f"{x:.17g} {-x:.17g} {x:.17g} {-x:.17g}")
            emit(f"f32-midpoint-{i}-{toward}", root)
    for red in ("-0.1", "1.1", "0.5"):
        root, _, _, node = drawing(dict(ShowAtomStereo="yes"))
        indicator(node, runs=[("R", dict(color="2"))])
        colors = ET.SubElement(root, "colortable")
        ET.SubElement(colors, "color", r=red, g="0", b="0")
        emit(f"palette-document-boundary-{red}", root)
    text = ET.tostring(drawing()[0], encoding="unicode")
    emit(
        "dtd-restriction",
        '<!DOCTYPE CDXML [<!ATTLIST n Element CDATA "6">]>' + text,
        restriction="Internal DTD",
    )
    emit(
        "namespace-restriction",
        text.replace("<CDXML>", '<CDXML xmlns:x="urn:x" x:unused="1">'),
        restriction="Namespace",
    )
    emit("external-dtd", '<!DOCTYPE CDXML SYSTEM "https://example.invalid/cdxml.dtd">' + text)
    root, _, _, node = drawing(dict(ShowAtomStereo="yes"))
    indicator(node)
    fonts = ET.SubElement(root, "fonttable")
    ET.SubElement(fonts, "font", id="3", name="Custom Font")
    emit("font-and-style", root)
    # A source copied before a deliberate error remains intact in the oracle.
    root = copy.deepcopy(root)
    copied_node = root.find(".//n")
    assert copied_node is not None
    copied_node.set("p", "bad")
    emit("source-copy", root)


if __name__ == "__main__":
    main()
