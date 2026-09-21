"""Native double-bond geometry and propagation, independent of the worker."""

import itertools
import json
import math
import random
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDConfig, RDLogger, rdBase
from rdkit.Chem import rdDepictor

if TYPE_CHECKING or __package__:
    from .drawn_stereo_reference import conformer
    from .kekulize_reference import DIRECTIONS, directions
    from .ranking_reference import metadata
    from .valence_reference import ORDERS, graph
else:
    from drawn_stereo_reference import conformer
    from kekulize_reference import DIRECTIONS, directions
    from ranking_reference import metadata
    from valence_reference import ORDERS, graph


def emit(name, original, operation="detect"):
    mol = Chem.Mol(original)
    mol.UpdatePropertyCache(strict=False)
    Chem.GetSymmSSSR(mol)
    source = graph(mol)
    before = dict(metadata=metadata(mol), directions=directions(mol))
    positions = None
    if mol.GetNumConformers():
        positions = [
            dict(x=p.x, y=p.y, z=p.z)
            for p in (mol.GetConformer().GetAtomPosition(i) for i in range(mol.GetNumAtoms()))
        ]
    rings = list(mol.GetRingInfo().AtomRings())
    try:
        if operation == "set":
            Chem.SetDoubleBondNeighborDirections(
                mol, mol.GetConformer() if positions is not None else None
            )
        else:
            Chem.DetectBondStereochemistry(mol)
            if operation == "perceive":
                Chem.SetBondStereoFromDirections(mol)
        expected = dict(metadata=metadata(mol), directions=directions(mol))
        if graph(mol) != source:
            raise AssertionError("Unexpected native graph mutation")
    except (ValueError, RuntimeError):
        expected = None
    print(
        json.dumps(
            dict(
                name=f"{name}/{operation}",
                operation=operation,
                graph=source,
                before=before,
                positions=positions,
                rings=rings,
                expected=expected,
            )
        )
    )


def alkene(variant):
    mol = Chem.RWMol()
    for number in (9, 6, 17, 6, 35, 53):
        mol.AddAtom(Chem.Atom(number))
    edges = [(0, 1), (1, 2), (3, 4), (3, 5), (1, 3)]
    if variant & 1:
        edges.reverse()
    for a, b in edges:
        order = Chem.BondType.DOUBLE if {a, b} == {1, 3} else Chem.BondType.SINGLE
        if variant & 2:
            a, b = b, a
        mol.AddBond(a, b, order)
    conformer(mol, [(-1, 1, 0), (0, 0, 0), (-1, -1, 0), (1, 0, 0), (2, 1, 0), (2, -1, 0)])
    return mol


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    emit("empty", Chem.Mol())
    all_dirs = list(DIRECTIONS.values())
    rng = random.Random(581239)
    for dirs in itertools.product(
        (
            Chem.BondDir.NONE,
            Chem.BondDir.ENDUPRIGHT,
            Chem.BondDir.ENDDOWNRIGHT,
            Chem.BondDir.UNKNOWN,
        ),
        repeat=4,
    ):
        for variant in range(4):
            for old_stereo in (0, 1, 3):
                mol = alkene(variant)
                singles = [b for b in mol.GetBonds() if b.GetBondType() != Chem.BondType.DOUBLE]
                for bond, direction in zip(singles, dirs, strict=True):
                    bond.SetBondDir(direction)
                    if variant & 1:
                        bond.SetIntProp("_UnknownStereo", rng.choice((0, 0, 0, 1, -1)))
                double = mol.GetBondBetweenAtoms(1, 3)
                double.SetStereoAtoms(
                    0 if double.GetBeginAtomIdx() == 1 else 4,
                    4 if double.GetEndAtomIdx() == 3 else 0,
                )
                double.SetStereo(Chem.BondStereo.values[old_stereo])
                double.SetBondDir(rng.choice(all_dirs))
                emit(f"directions {list(map(int, dirs))}/{variant}/{old_stereo}", mol)
                emit(f"directions {list(map(int, dirs))}/{variant}/{old_stereo}", mol, "set")
                mol.RemoveAllConformers()
                emit(f"directions without coordinates {variant}/{old_stereo}", mol, "set")
    for angle in (
        0,
        0.001,
        0.1,
        1,
        1.9,
        2,
        2.1,
        30,
        89.999,
        90,
        90.001,
        120,
        177,
        178,
        178.1,
        179.999,
        180,
    ):
        theta = math.radians(angle)
        for scale in (1e-10, 0.001, 0.01, 1, 6, 1e20, 1e30):
            for variant in range(4):
                for shape in range(3):
                    mol = alkene(variant)
                    points = [
                        (-math.cos(theta), math.sin(theta), 0),
                        (0, 0, 0),
                        (-1, -1, 0),
                        (1, 0, 0),
                        (
                            1 + math.cos(theta),
                            (-1 if shape == 1 else 1) * math.sin(theta),
                            1 if shape == 2 else 0,
                        ),
                        (2, -1, 0),
                    ]
                    conformer(mol, [(x * scale, y * scale, z * scale) for x, y, z in points])
                    emit(f"angle {angle}/{scale}/{variant}/{shape}", mol)
    for size in range(3, 25):
        for pattern in ("one", "alternating"):
            mol = Chem.RWMol()
            for _ in range(size):
                mol.AddAtom(Chem.Atom(6))
            for i in range(size):
                mol.AddBond(
                    i,
                    (i + 1) % size,
                    Chem.BondType.DOUBLE
                    if i == 0 or pattern == "alternating" and i % 2 == 0
                    else Chem.BondType.SINGLE,
                )
            conformer(
                mol,
                [
                    (math.cos(2 * math.pi * i / size), math.sin(2 * math.pi * i / size), 0)
                    for i in range(size)
                ],
            )
            for direction in all_dirs:
                for unknown in (0, 1):
                    mol.GetBondWithIdx(1).SetBondDir(direction)
                    mol.GetBondWithIdx(1).SetIntProp("_UnknownStereo", unknown)
                    emit(f"ring {size}/{pattern}/{int(direction)}/{unknown}", mol)
    texts = [
        "CC=CC=CC=CC",
        "CC=C(C)C=C(C)C",
        "FC=C=CCl",
        "FC=C1CCCCC1",
        "C1=CCCCCCC1",
        "CC(=C)C(=C)C",
        "CC=C(C=C(C=C(C)C)C)C",
        "CC=C(C=C)C=CC",
        "FC=C(F)C(F)=CF",
        "F/C=C/C=C/C=C/Cl",
    ]
    for sample in range(3000):
        mol = Chem.MolFromSmiles(rng.choice(texts))
        rdDepictor.Compute2DCoords(mol)
        for bond in mol.GetBonds():
            bond.SetBondDir(rng.choice(all_dirs))
            if rng.random() < 0.3:
                bond.SetIntProp("_UnknownStereo", rng.choice((0, 1, -1)))
            if rng.random() < 0.1:
                bond.SetBondType(rng.choice(list(ORDERS.values())))
        for i in range(mol.GetNumAtoms()):
            p = mol.GetConformer().GetAtomPosition(i)
            mol.GetConformer().SetAtomPosition(
                i,
                (
                    p.x + rng.uniform(-0.5, 0.5),
                    p.y + rng.uniform(-0.5, 0.5),
                    rng.uniform(-0.5, 0.5),
                ),
            )
        order = list(range(mol.GetNumAtoms()))
        rng.shuffle(order)
        emit(f"branched random {sample}", Chem.RenumberAtoms(mol, order))
    templates = json.loads(
        (Path(__file__).resolve().parents[1] / "assets/templates.json").read_text()
    )
    texts += [t["smiles"] for t in templates if t.get("smiles")]
    texts += [
        line.split()[0]
        for line in (Path(RDConfig.RDDataDir) / "NCI/first_5K.smi").read_text().splitlines()
    ]
    for text in texts:
        mol = Chem.MolFromSmiles(text)
        if mol is None:
            continue
        emit(text + " existing", mol, "set")
        rdDepictor.Compute2DCoords(mol)
        emit(text + " drawn", mol)
        emit(text + " complete", mol, "perceive")
        for bond in mol.GetBonds():
            bond.SetBondDir(rng.choice(all_dirs))
            if rng.random() < 0.2:
                bond.SetIntProp("_UnknownStereo", rng.choice((0, 1, -1)))
        order = list(range(mol.GetNumAtoms()))
        rng.shuffle(order)
        mol = Chem.RenumberAtoms(mol, order)
        for i in range(mol.GetNumAtoms()):
            p = mol.GetConformer().GetAtomPosition(i)
            mol.GetConformer().SetAtomPosition(i, (-p.y * 6 + 10, -p.x * 6 + 13, p.z))
        emit(text + " permuted flags", mol)
        mol.RemoveAllConformers()
        emit(text + " absent conformer", mol)


if __name__ == "__main__":
    main()
