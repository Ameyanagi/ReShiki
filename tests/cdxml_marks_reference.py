"""Direct original read_marks() oracle; no migrated worker is involved."""

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

from engine.marks_exchange import read_marks


def emit(name, text, atoms=None, positions=None, scale=1, restriction=None):
    atoms = atoms if atoms is not None else [(6, 1, 0)]
    positions = positions if positions is not None else [(0, 0, 0)] * len(atoms)
    mol = Chem.RWMol()
    for element, charge, radicals in atoms:
        atom = Chem.Atom(element)
        atom.SetFormalCharge(charge)
        atom.SetNumRadicalElectrons(radicals)
        mol.AddAtom(atom)
    conf = Chem.Conformer(len(atoms))
    for i, point in enumerate(positions):
        conf.SetAtomPosition(i, point)
    mol.AddConformer(conf)
    expected = failure = failure_type = None
    try:
        root = ET.fromstring(text)
        before = ET.tostring(root)
        base = dict(atoms=[])
        objects = {}
        read_marks(root, mol, base, scale, objects)
        assert ET.tostring(root) == before
        ordinals = {node: index for index, node in enumerate(root.iter())}
        expected = dict(
            atoms=base["atoms"],
            objects=[dict(source=ordinals[node], atoms=ids) for node, ids in objects.items()],
        )
    except (ValueError, KeyError, IndexError, OverflowError, ET.ParseError) as exc:
        failure, failure_type = str(exc), type(exc).__name__
    print(
        json.dumps(
            dict(
                name=name,
                text=text,
                atoms=[dict(atomic_number=e, charge=c, radical_electrons=r) for e, c, r in atoms],
                positions=[dict(zip(("x", "y", "z"), p, strict=True)) for p in positions],
                scale=scale,
                expected=expected,
                failure=failure,
                failure_type=failure_type,
                restriction=restriction,
            )
        )
    )


def xml(symbol="Plus", box="4 3 0 0", node=None, graphic=None, reps=None):
    root = ET.Element("CDXML")
    frag = ET.SubElement(ET.SubElement(root, "page"), "fragment")
    ET.SubElement(
        frag,
        "n",
        {
            k: str(v)
            for k, v in (node if node is not None else dict(id="a", p="0 0")).items()
            if v is not None
        },
    )
    attrs = dict(SymbolType=symbol, BoundingBox=box)
    attrs.update(graphic or {})
    el = ET.SubElement(frag, "graphic", {k: str(v) for k, v in attrs.items() if v is not None})
    for rep in reps if reps is not None else [dict(object="a", attribute="Charge")]:
        ET.SubElement(el, "represent", rep)
    return ET.tostring(root, encoding="unicode")


def main():
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    for symbol, charge, radicals, attribute in itertools.product(
        (None, "Plus", "Minus", "CirclePlus", "CircleMinus", "Electron", "LonePair", "Radical", ""),
        (-2, -1, 0, 1, 2),
        range(4),
        ("Charge", "Radical", "bad"),
    ):
        emit(
            f"symbol-{symbol}-{charge}-{radicals}-{attribute}",
            xml(symbol, reps=[dict(object="a", attribute=attribute)]),
            [(6, charge, radicals)],
        )
    for p in (
        None,
        "",
        "0",
        "0 0 7",
        "0 0 nan",
        "0 0 bad",
        "nan 0",
        "inf 0",
        "0 bad",
        "０ ٠",
        "0_0 -0",
        "0x0 0",
        "0\u001c0",
    ):
        emit(f"node-p-{p!r}", xml(node=dict(id="a", p=p)))
    for element in (None, "６", "0_6", "+006", "6.0", "6e0", "bad", "-1", "119", "1" * 100):
        emit(f"element-{element}", xml(node=dict(id="a", p="0 0", Element=element)))
    for bounds in (
        None,
        "",
        "0 0",
        "0 0 0 0 0",
        "nan 0 1 0",
        "inf 0 1 0",
        "bad 0 1 0",
        "0 0 0.499999 0",
        "0 0 .5 0",
        "0 0 96 0",
        "0 0 96.000001 0",
        "０ ٠ ３ ٤",
        "0_0 0 3 4",
    ):
        for symbol in ("Plus", "Electron", "LonePair"):
            emit(f"bounds-{symbol}-{bounds}", xml(symbol, bounds), [(6, 1, 1)])
    for name, value in itertools.product(
        ("color", "LineWidth", "BoldWidth", "Color", "lineWidth"), ("", "0", "1", "bad")
    ):
        emit(f"override-{name}-{value}", xml(graphic={name: value}))
    for reps in (
        [],
        [dict(attribute="Charge")],
        [dict(object="missing", attribute="Charge")],
        [dict(object="a")],
        [dict(object="a", attribute="Charge")] * 2,
        [dict(object="a", attribute="Charge"), dict(object="a", attribute="Radical")],
        [dict(object="a", attribute="Charge"), dict(object="b", attribute="Charge")],
    ):
        emit(f"represent-{reps}", xml(reps=reps))
    emit("absent-source-id", xml(node=dict(p="0 0"), reps=[dict(attribute="Charge")]))
    for delta in (-0.02000001, -0.02, -0.01999999, 0, 0.01999999, 0.02, 0.02000001):
        emit(f"tolerance-{delta}", xml(), positions=[(delta / 28, 0, 0)])
    emit("ambiguous", xml(), [(6, 1, 0), (6, 1, 0)])
    emit("different-element-overlap", xml(), [(8, 1, 0), (6, 1, 0)])
    emit("empty-molecule", xml(node=dict(id="a", p="0 0", Element="bad")), [], [])
    for scale in (0, -2, 1 / 3, 42 / 14.4, 1e100):
        emit(f"scale-{scale}", xml(), scale=scale)
    for declaration in (
        '<!DOCTYPE CDXML [<!ENTITY p "Plus">]>',
        '<!DOCTYPE CDXML [<!ATTLIST n Element CDATA "6">]>',
    ):
        emit("dtd-restriction", declaration + xml(), restriction="Internal DTD declarations")
    emit(
        "namespace-restriction",
        xml(graphic={"xmlns:x": "urn:x", "x:test": "1"}),
        restriction="Namespaces",
    )
    emit(
        "external-dtd-ignored",
        '<!DOCTYPE CDXML SYSTEM "https://example.invalid/cdxml.dtd">' + xml(),
    )
    rng = random.Random(184)
    for i in range(500):
        angle = rng.uniform(-math.pi, math.pi)
        size = rng.uniform(0.6, 95)
        box = f"0 0 {size * math.cos(angle):.17g} {size * math.sin(angle):.17g}"
        emit(f"geometry-{i}", xml(box=box))
    for i in range(1000):
        bits = rng.randrange(0x3F000000, 0x42BFFFFF)
        low = struct.unpack("!f", struct.pack("!I", bits))[0]
        high = struct.unpack("!f", struct.pack("!I", bits + 1))[0]
        midpoint = (low + high) / 2
        angle = rng.uniform(-math.pi, math.pi)
        x, y = midpoint * math.cos(angle), midpoint * math.sin(angle)
        for direction in (-math.inf, None, math.inf):
            near = math.nextafter(x, direction) if direction is not None else x
            emit(f"f32-midpoint-{i}-{direction}", xml(box=f"0 0 {near:.17g} {y:.17g}"))
    for dx, dy in itertools.product(("0", "-0", "1", "-1"), repeat=2):
        emit(f"signed-axis-{dx}-{dy}", xml(box=f"{dx} {dy} 0 0"))
    source = '<CDXML><page><fragment><n id="a" p="0 0"/><n id="b" p="10 0"/><n id="a" p="20 0"/>'
    for owner in ("b", "a", "b", "a"):
        source += f'<graphic id="same" SymbolType="Plus" BoundingBox="4 3 0 0"><represent object="{owner}" attribute="Charge"/></graphic>'
    emit(
        "order-last-id-and-repeated-marks",
        source + "</fragment></page></CDXML>",
        [(6, 1, 0)] * 3,
        [(0, 0, 0), (10 / 28, 0, 0), (20 / 28, 0, 0)],
    )


if __name__ == "__main__":
    main()
