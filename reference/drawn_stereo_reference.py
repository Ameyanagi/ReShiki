"""Independent wedge/dash perception through RDKit's public geometry operation."""

import itertools
import json
import math
import random
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDConfig, RDLogger, rdBase
from rdkit.Chem import rdDepictor

if TYPE_CHECKING or __package__:
    from .kekulize_reference import DIRECTIONS, directions
    from .ranking_reference import metadata
    from .stereo_reference import group
    from .valence_reference import ORDERS, atom_molecule, graph
else:
    from kekulize_reference import DIRECTIONS, directions
    from ranking_reference import metadata
    from stereo_reference import group
    from valence_reference import ORDERS, atom_molecule, graph


def state(mol):
    return dict(
        graph=graph(mol),
        metadata=metadata(mol),
        valences=[
            dict(
                explicit_valence=a.GetValence(Chem.ValenceType.EXPLICIT),
                implicit_hydrogens=a.GetNumImplicitHs(),
            )
            for a in mol.GetAtoms()
        ],
    )


def emit(name, original, replace=True, operation="atom"):
    mol = Chem.Mol(original)
    mol.UpdatePropertyCache(strict=False)
    before = state(mol)
    positions = None
    if mol.GetNumConformers():
        positions = [
            dict(x=p.x, y=p.y, z=p.z)
            for p in (mol.GetConformer().GetAtomPosition(i) for i in range(mol.GetNumAtoms()))
        ]
    dirs = directions(mol)
    try:
        if operation == "bond":
            Chem.SetBondStereoFromDirections(mol)
        else:
            Chem.AssignChiralTypesFromBondDirs(mol, replaceExistingTags=replace)
        expected = state(mol)
    except (ValueError, RuntimeError):
        expected = None
    print(
        json.dumps(
            dict(
                name=f"{name}/{replace}",
                before=before,
                positions=positions,
                directions=dirs,
                replace=replace,
                operation=operation,
                expected=expected,
            )
        )
    )


def conformer(mol, positions):
    mol.RemoveAllConformers()
    conf = Chem.Conformer(mol.GetNumAtoms())
    conf.Set3D(False)
    for i, point in enumerate(positions):
        conf.SetAtomPosition(i, point)
    mol.AddConformer(conf)


def star(number, points, bond_directions, order=1, reverse=()):
    mol = atom_molecule(number, 0, 0, 0, False)
    for i, direction in enumerate(bond_directions):
        mol.AddAtom(Chem.Atom((9, 17, 35, 53, 6, 6)[i]))
        a, b = (i + 1, 0) if i in reverse else (0, i + 1)
        mol.AddBond(a, b, ORDERS[order] if i == 0 else Chem.BondType.SINGLE)
        mol.GetBondWithIdx(i).SetBondDir(direction)
    conformer(mol, [(0.0, 0.0, 0.0), *points])
    return mol


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    emit("empty", Chem.Mol())
    layouts = [
        [(0, 1, 0), (-1, -0.5, 0), (1, -0.5, 0)],
        [(0, 1, 0), (-1, 0, 0), (1, 0, 0)],
        [(0, 1, 0), (-1, -0.5, 0), (1, -0.5, 0), (0, -1, 0)],
        [(0, 1, 0), (-1, 0, 0), (0, -1, 0), (1, 0, 0)],
        [(0, 1, 0), (-1, 0, 0), (-1, 0, 0)],
        [(0, 0, 0), (-1, -0.5, 0), (1, -0.5, 0)],
    ]
    all_dirs = list(DIRECTIONS.values())
    for index, points in enumerate(layouts):
        for dirs in itertools.product(
            (
                Chem.BondDir.NONE,
                Chem.BondDir.BEGINWEDGE,
                Chem.BondDir.BEGINDASH,
                Chem.BondDir.UNKNOWN,
            ),
            repeat=len(points),
        ):
            for tag in (0, 1, 2, 6):
                mol = star(6, points, dirs)
                mol.GetAtomWithIdx(0).SetChiralTag(Chem.ChiralType.values[tag])
                mol.SetStereoGroups([group(mol, 1, [0], [], 17)])
                for replace in (False, True):
                    emit(f"layout {index}/{list(map(int, dirs))}/{tag}", mol, replace)
    # All elements, special bond orders, degree limits and explicit-H policy.
    for number in range(119):
        for degree in range(1, 7):
            points = [
                (math.cos(i * 2 * math.pi / degree), math.sin(i * 2 * math.pi / degree), 0.0)
                for i in range(degree)
            ]
            for order in ORDERS:
                mol = star(number, points, [Chem.BondDir.BEGINWEDGE] * degree, order)
                emit(f"element {number}/{degree}/{order}", mol)
                atom = mol.GetAtomWithIdx(0)
                atom.SetFormalCharge(1)
                atom.SetNumExplicitHs(1)
                atom.SetNoImplicit(True)
                emit(f"charged explicit {number}/{degree}/{order}", mol)
    # Near-linear and near-overlapping layouts exercise native tolerances.
    for angle in (
        0,
        0.001,
        0.01,
        0.02,
        0.1,
        1,
        1.8,
        2,
        2.1,
        30,
        90,
        120,
        177,
        178,
        178.2,
        179.9,
        180,
    ):
        for scale in (1e-18, 1e-10, 0.01, 0.1, 1, 6, 100, 1e20):
            theta = math.radians(angle)
            for degree in (3, 4):
                points = [(0, 1, 0), (-1, 0, 0), (math.cos(theta), math.sin(theta), 0)]
                if degree == 4:
                    points.append((0, -1, 0))
                points = [(x * scale, y * scale, z) for x, y, z in points]
                for ref in range(degree):
                    dirs = [Chem.BondDir.NONE] * degree
                    dirs[ref] = Chem.BondDir.BEGINWEDGE
                    emit(f"boundary {angle}/{scale}/{degree}/{ref}", star(6, points, dirs))
    rng = random.Random(634891)
    for dirs in itertools.product(
        (
            Chem.BondDir.NONE,
            Chem.BondDir.ENDUPRIGHT,
            Chem.BondDir.ENDDOWNRIGHT,
            Chem.BondDir.UNKNOWN,
        ),
        repeat=4,
    ):
        for old_stereo in range(8):
            for variant in range(4):
                mol = Chem.RWMol()
                for number in (9, 6, 17, 6, 35, 53):
                    mol.AddAtom(Chem.Atom(number))
                edges = [(0, 1), (1, 2), (3, 4), (3, 5)]
                if variant % 2:
                    edges.reverse()
                for i, ((a, b), direction) in enumerate(zip(edges, dirs, strict=True)):
                    if variant >= 2:
                        a, b = b, a
                    order = rng.choice((0, 1, 3, 4, 5, 6, 7)) if variant == 3 else 1
                    mol.AddBond(a, b, ORDERS[order])
                    mol.GetBondWithIdx(i).SetBondDir(direction)
                a, b = (3, 1) if variant >= 2 else (1, 3)
                mol.AddBond(a, b, Chem.BondType.DOUBLE)
                double = mol.GetBondWithIdx(4)
                double.SetStereoAtoms(4 if a == 3 else 0, 0 if b == 1 else 4)
                double.SetStereo(Chem.BondStereo.values[old_stereo])
                double.SetBondDir(rng.choice(all_dirs))
                emit(
                    f"bond directions {list(map(int, dirs))}/{old_stereo}/{variant}",
                    mol,
                    operation="bond",
                )
    for sample in range(5000):
        degree = rng.choice((2, 3, 3, 4, 4, 5))
        points = [
            (rng.uniform(-3, 3), rng.uniform(-3, 3), rng.choice((0.0, 0.0, 1.0)))
            for _ in range(degree)
        ]
        dirs = [rng.choice(all_dirs) for _ in range(degree)]
        mol = star(
            rng.choice((0, 6, 7, 8, 14, 15, 16, 26)),
            points,
            dirs,
            rng.choice(list(ORDERS)),
            reverse=[i for i in range(degree) if rng.random() < 0.3],
        )
        mol.GetAtomWithIdx(0).SetChiralTag(Chem.ChiralType.values[rng.randrange(9)])
        order = list(range(mol.GetNumAtoms()))
        rng.shuffle(order)
        emit(f"random {sample}", Chem.RenumberAtoms(mol, order), sample % 2 == 0)
    # Native #7509: preserve winding at unusual molecular scales.
    mol = Chem.MolFromSmiles("CCC(O)C")
    conformer(
        mol,
        [(-2.0785, 0, 0), (-0.7794, 0.75, 0), (0.5196, 0, 0), (0.5196, -1.5, 0), (1.8187, 0.75, 0)],
    )
    mol.GetBondBetweenAtoms(2, 4).SetBondDir(Chem.BondDir.BEGINWEDGE)
    for scale in (0.1, 1, 6, 1e10):
        copy = Chem.Mol(mol)
        for i in range(copy.GetNumAtoms()):
            p = mol.GetConformer().GetAtomPosition(i)
            copy.GetConformer().SetAtomPosition(i, (p.x * scale, p.y * scale, p.z))
        emit(f"native scale {scale}", copy)
    templates = json.loads(
        (Path(__file__).resolve().parents[1] / "assets/templates.json").read_text()
    )
    texts = [t["smiles"] for t in templates if t.get("smiles")]
    texts += [
        line.split()[0]
        for line in (Path(RDConfig.RDDataDir) / "NCI/first_5K.smi").read_text().splitlines()
    ]
    for text in texts:
        mol = Chem.MolFromSmiles(text)
        if mol is None:
            continue
        rdDepictor.Compute2DCoords(mol)
        Chem.WedgeMolBonds(mol, mol.GetConformer())
        emit(text + " preserved", mol, False)
        emit(text + " perceived", mol)
        for bond in mol.GetBonds():
            bond.SetBondDir(rng.choice(all_dirs))
        emit(text + " directions", mol)
        emit(text + " bond directions", mol, operation="bond")
        order = list(range(mol.GetNumAtoms()))
        rng.shuffle(order)
        mol = Chem.RenumberAtoms(mol, order)
        for i in range(mol.GetNumAtoms()):
            p = mol.GetConformer().GetAtomPosition(i)
            mol.GetConformer().SetAtomPosition(i, (-p.y * 6 + 10, -p.x * 6 + 13, p.z))
        emit(text + " reflected permuted", mol)
        mol.RemoveAllConformers()
        emit(text + " absent conformer", mol)


if __name__ == "__main__":
    main()
