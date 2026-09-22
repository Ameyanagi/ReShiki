"""Original abbreviation read() records with native dense atom IDs."""

import copy
import itertools
import json
import math
import random
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from rdkit import Chem, RDLogger, rdBase

from engine import abbreviations_exchange, cdx_exchange
from engine.abbreviations import PRESETS
from engine.worker import chemistry_xml, handle


def record(anchor="a", members=None, label="OMe", reverse="MeO"):
    return dict(
        label=label,
        reverse_label=reverse,
        anchor=anchor,
        members=[anchor] if members is None else members,
    )


def xml(nodes):
    root = ET.Element("CDXML")
    frag = ET.SubElement(ET.SubElement(root, "page"), "fragment", id="1")
    for node in nodes:
        ET.SubElement(frag, "n", {k: str(v) for k, v in node.items() if v is not None})
    return ET.tostring(root, encoding="unicode")


def emit(name, text, records, atoms, positions, *, scale=1.0, sizes=None, restriction=None):
    mol = Chem.RWMol()
    for number in atoms:
        mol.AddAtom(Chem.Atom(number))
    conformer = Chem.Conformer(len(atoms))
    for i, p in enumerate(positions):
        conformer.SetAtomPosition(i, p)
    mol.AddConformer(conformer)
    expected = failure = failure_type = None
    try:
        root = ET.fromstring(text)
        expected = abbreviations_exchange.read(records, root, mol, scale)
    except (ValueError, KeyError, TypeError, AttributeError) as exc:
        failure, failure_type = str(exc), type(exc).__name__
    print(
        json.dumps(
            dict(
                name=name,
                text=text,
                records=records,
                atoms=atoms,
                positions=[dict(zip(("x", "y", "z"), p, strict=True)) for p in positions],
                scale=scale,
                sizes=sizes if sizes is not None else [len(atoms)],
                expected=expected,
                failure=failure,
                failure_type=failure_type,
                restriction=restriction,
            )
        )
    )


def native_case(name, root):
    records = abbreviations_exchange.flatten(root)
    parts = Chem.MolsFromCDXML(
        ET.tostring(chemistry_xml(root), encoding="unicode"), sanitize=False, removeHs=False
    )
    factor = float(root.get("BondLength", "14.4")) / 14.4
    atoms = [a.GetAtomicNum() for part in parts for a in part.GetAtoms()]
    positions = []
    for part in parts:
        for i in range(part.GetNumAtoms()):
            p = part.GetConformer().GetAtomPosition(i)
            positions.append((p.x * factor, p.y * factor, p.z * factor))
    emit(
        name,
        ET.tostring(root, encoding="unicode"),
        records,
        atoms,
        positions,
        scale=1 / (14.4 / 42),
        sizes=[part.GetNumAtoms() for part in parts],
    )


def main():
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    emit(
        "dense-order-not-source-id",
        xml([dict(id="a", p="0 0", Element="8"), dict(id="b", p="10 20")]),
        [record(members=["b", "a", "b"])],
        [6, 8],
        [(10 / 28, -20 / 28, 0), (0, 0, 5)],
        sizes=[1, 1],
    )
    emit(
        "record-order-and-typography",
        xml([dict(id="a", p="0 0")]),
        [
            record(label="custom α&β", reverse="β&α"),
            record(label="Me", reverse=""),
            record(members=[]),
        ],
        [6],
        [(0, 0, 0)],
    )
    emit("no-records-unused-invalid-node", xml([dict(id="a", Element="not parsed")]), [], [], [])
    emit("empty-molecule", xml([dict(id="a", p="0 0")]), [record()], [], [])
    emit(
        "empty-molecule-does-not-read-element",
        xml([dict(id="a", p="0 0", Element="bad")]),
        [record()],
        [],
        [],
    )
    emit("absent-id", xml([dict(p="0 0")]), [record(None)], [6], [(0, 0, 0)])
    for identifier in ("a", None):
        emit(
            f"last-id-{identifier}",
            xml([dict(id=identifier, p="0 0"), dict(id=identifier, p="1 0")]),
            [record(identifier)],
            [6, 6],
            [(0, 0, 0), (1 / 28, 0, 0)],
            sizes=[1, 1],
        )
    emit("missing-source-id", xml([dict(id="a", p="0 0")]), [record("missing")], [6], [(0, 0, 0)])
    emit(
        "missing-member-after-anchor",
        xml([dict(id="a", p="0 0")]),
        [record(members=["a", "missing"])],
        [6],
        [(0, 0, 0)],
    )
    emit(
        "same-element-overlap",
        xml([dict(id="a", p="0 0")]),
        [record()],
        [6, 6],
        [(0, 0, 0), (0, 0, 0)],
        sizes=[1, 1],
    )
    emit(
        "different-element-overlap",
        xml([dict(id="a", p="0 0")]),
        [record()],
        [8, 6],
        [(0, 0, 0), (0, 0, 0)],
        sizes=[1, 1],
    )
    for element in range(119):
        emit(
            f"element-{element}",
            xml([dict(id="a", p="0 0", Element=element)]),
            [record()],
            [element],
            [(0, 0, 0)],
        )
    for spelling in (
        None,
        "6",
        "+006",
        " 6 ",
        "0_6",
        "６",
        "٠٦",
        "6.0",
        "6e0",
        "C",
        "",
        "-1",
        "119",
        "65536",
        "99999999999999999999999999999",
        "-0",
    ):
        emit(
            f"element-spelling-{spelling}",
            xml([dict(id="a", p="0 0", Element=spelling)]),
            [record()],
            [6, 0],
            [(0, 0, 0), (0, 0, 0)],
        )
    for p in (None, "", "0", "0 0 0", "bad 0", "nan 0", "inf 0", "-inf 0", "0_0 ０", "-0 +0"):
        emit(f"source-position-{p}", xml([dict(id="a", p=p)]), [record()], [6], [(0, 0, 0)])
    deltas = (
        -0.020001,
        -0.02,
        math.nextafter(-0.02, 0),
        -0.019,
        0.0,
        0.019,
        math.nextafter(0.02, 0),
        0.02,
        0.020001,
    )
    for origin, dx, dy in itertools.product(
        (0.0, 0.02, -0.02, 100.0, 2**40, 2**47, -(2**47)), deltas, deltas
    ):
        emit(
            f"tolerance-{origin}-{dx}-{dy}",
            xml([dict(id="a", p=f"{origin!r} {origin!r}")]),
            [record()],
            [6],
            [((origin + dx) / 28, -(origin + dy) / 28, 0)],
        )
    for world in (1e100, -1e100, 1e300, -1e300, (2**63 - 4096) * 0.02, -(2**63 - 4096) * 0.02):
        point = world / 28
        world = point * 28
        emit(
            f"large-grid-{world}",
            xml([dict(id="a", p=f"{world!r} 0")]),
            [record()],
            [6],
            [(point, 0, 0)],
        )
    for scale in (0.0, -1.0, 0.1, 1.0, 42 / 14.4, 10.0, 1e6, 5e-324):
        emit(
            f"scaled-{scale}",
            xml([dict(id="a", p="10 20")]),
            [record()],
            [6],
            [(10 * scale / 28, -20 * scale / 28, 0)],
            scale=scale,
        )
    rng = random.Random(777845)
    for i in range(1000):
        bound = 0.019 if i % 2 == 0 else 0.025
        scale = rng.choice((0.5, 1.0, 42 / 14.4, 7.0))
        atoms = [rng.choice((6, 7, 8, 16)) for _ in range(12)]
        positions = [
            (rng.uniform(-100, 100), rng.uniform(-100, 100), rng.uniform(-10, 10)) for _ in atoms
        ]
        nodes = [
            dict(
                id=f"xml-{j * 3 + 100}",
                Element=number,
                p=f"{(p[0] * 28 + rng.uniform(-bound, bound)) / scale!r} {(-p[1] * 28 + rng.uniform(-bound, bound)) / scale!r}",
            )
            for j, (number, p) in enumerate(zip(atoms, positions, strict=True))
        ]
        ids = [n["id"] for n in nodes]
        rng.shuffle(ids)
        emit(
            f"random-{i}",
            xml(nodes),
            [record(ids[0], ids)],
            atoms,
            positions,
            scale=scale,
            sizes=[4, 0, 8],
        )
    original = handle(dict(protocol=1, operation="import", format="smiles", text="CCC"))["document"]
    for label in PRESETS:
        changed = handle(
            dict(
                protocol=1,
                operation="abbreviate",
                format="replace",
                document=original,
                selected_ids=[original["atoms"][-1]["id"]],
                text=label,
            )
        )["document"]
        text = handle(dict(protocol=1, operation="export", format="cdxml", document=changed))[
            "output"
        ]
        for length in (14.4, 25.0, 100.0):
            root = ET.fromstring(text)
            root.set("BondLength", str(length))
            native_case(f"native-export-{label}-{length}", root)
    root = ET.fromstring(
        cdx_exchange.from_cdx(
            (Path(__file__).parent / "fixtures/abbreviations-native.cdx").read_bytes()
        )
    )
    native_case("native-binary-abbreviations", root)
    # Native fragment ordering places groups before sibling fragments. Keep
    # the source record order while mapping into that combined dense order.
    root = ET.fromstring(text)
    page = root.find("page")
    assert page is not None
    fragment = page.find("fragment")
    assert fragment is not None
    other = copy.deepcopy(fragment)
    for element in other.iter():
        for key in ("id", "B", "E", "ConnectionOrder", "BondOrdering"):
            if key in element.attrib:
                element.set(key, str(int(element.get(key)) + 10000))
        if "p" in element.attrib:
            xy = list(map(float, element.get("p").split()))
            element.set("p", f"{xy[0] + 1000} {xy[1] + 1000}")
    ET.SubElement(page, "group", id="9000").append(other)
    native_case("native-group-fragment-order", root)
    for restriction, text in (
        ("namespace", '<CDXML xmlns="urn:example"/>'),
        ("DTD", '<!DOCTYPE CDXML [<!ATTLIST CDXML x CDATA "y">]><CDXML/>'),
    ):
        emit("restriction-" + restriction, text, [], [], [], restriction=restriction)


if __name__ == "__main__":
    RDLogger.DisableLog("rdApp.warning")
    main()
