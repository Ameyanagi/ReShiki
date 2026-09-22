"""Independent native file-reader oracle for axial stereo before sanitization.

RDKit does not expose detectAtropisomerChirality in Python. Its unsanitized
MOL reader calls that exact pass. Copy the resulting graph/atom tags, erase
only inferred axial tags, and restore the authored directions cleared by the
reader. No ReShiki parser, geometry calculation or worker participates.
"""

import copy
import json
import math
import random
from itertools import product
from typing import TYPE_CHECKING

from rdkit import Chem, RDLogger, rdBase
from rdkit.Chem import rdDepictor

if TYPE_CHECKING or __package__:
    from .ranking_reference import metadata
    from .valence_reference import graph
else:
    from ranking_reference import metadata
    from valence_reference import graph


def block(symbols, positions, bonds, spatial=False):
    atoms = [
        f"M  V30 {i + 1} {symbol} {x:.12g} {y:.12g} {z:.12g} 0"
        for i, (symbol, (x, y, z)) in enumerate(zip(symbols, positions))
    ]
    edges = [
        f"M  V30 {i + 1} {order} {a + 1} {b + 1} CFG={cfg}"
        for i, (a, b, order, cfg) in enumerate(bonds)
    ]
    return "\n".join(
        (
            "axial stereo reference",
            "                    " + ("3D" if spatial else "2D"),
            "",
            "  0  0  0  0  0  0  0  0  0  0999 V3000",
            "M  V30 BEGIN CTAB",
            f"M  V30 COUNTS {len(atoms)} {len(edges)} 0 0 0",
            "M  V30 BEGIN ATOM",
            *atoms,
            "M  V30 END ATOM",
            "M  V30 BEGIN BOND",
            *edges,
            "M  V30 END BOND",
            "M  V30 END CTAB",
            "M  END",
            "",
        )
    )


def cases():
    rng = random.Random(284153)
    for smiles in (
        "Fc1cccc(F)c1-c1c(Cl)cccc1Cl",
        "c1ccccc1-c1ccccc1",
        "c1ccccc1-n1cccc1",
        "c1ccc2ccccc2c1",
        "O=C(Nc1ccccc1)c1ccccc1",
        "c1ccccc1/C=C/c1ccccc1",
    ):
        for aromatic, sample in product((False, True), range(40)):
            mol = Chem.MolFromSmiles(smiles)
            order = list(range(mol.GetNumAtoms()))
            rng.shuffle(order)
            mol = Chem.RenumberAtoms(mol, order)
            rdDepictor.Compute2DCoords(mol)
            if not aromatic:
                Chem.Kekulize(mol, clearAromaticFlags=True)
            conf = mol.GetConformer()
            points = [
                (p.x, p.y, rng.uniform(-1, 1) if sample % 2 else 0)
                for p in (conf.GetAtomPosition(i) for i in range(mol.GetNumAtoms()))
            ]
            bonds = []
            for bond in mol.GetBonds():
                a, b = bond.GetBeginAtomIdx(), bond.GetEndAtomIdx()
                if rng.randrange(2):
                    a, b = b, a
                kind = 4 if bond.GetIsAromatic() else int(bond.GetBondTypeAsDouble())
                bonds.append((a, b, kind, rng.choice((0, 0, 0, 1, 2, 3))))
            rng.shuffle(bonds)
            yield (
                f"rings/{smiles}/{aromatic}/{sample}",
                block([a.GetSymbol() for a in mol.GetAtoms()], points, bonds, sample % 2 == 1),
                bonds,
            )
    positions = [(0, 0, 0), (1.5, 0, 0), (-1, 1, 0), (-1, -1, 0), (2.5, 1, 0), (2.5, -1, 0)]
    base = [(0, 1, 1), (0, 2, 1), (0, 3, 2), (1, 4, 1), (1, 5, 2)]
    # Every wedge/hash/unknown combination, including wedges on the axis,
    # multiple wedges at one end, and backwards bond endpoints.
    for configs in product(range(4), repeat=5):
        for reversed_axis, spatial in product((False, True), repeat=2):
            bonds = [
                (b, a, order, cfg) if reversed_axis else (a, b, order, cfg)
                for (a, b, order), cfg in zip(base, configs)
            ]
            points = list(positions)
            if spatial:
                points[4], points[5] = (2.5, 0.4, 1), (2.5, -0.4, -1)
            yield (
                f"directions/{configs}/{reversed_axis}/{spatial}",
                block(["C"] * 6, points, bonds, spatial),
                bonds,
            )
    # Candidate hybridizations, endpoint degree, element and bond-class gates.
    for left, right, order, side, count in product(
        ("C", "N", "O", "P", "S", "Si", "R"),
        ("C", "N", "S"),
        (1, 2, 3, 4, 9),
        (1, 2, 3, 4, 9),
        (1, 2, 3),
    ):
        points = [(0, 0, 0), (1.5, 0, 0)]
        bonds = [(0, 1, order, 0)]
        for end in (0, 1):
            for i in range(count):
                angle = (i + 0.5) * 2 * math.pi / count
                points.append((end * 1.5 + math.cos(angle), math.sin(angle), 0))
                bonds.append(
                    (end, len(points) - 1, side if i == 0 else 1, 1 if i == count - 1 else 0)
                )
        yield (
            f"elements/{left}/{right}/{order}/{side}/{count}",
            block([left, right] + ["C"] * (count * 2), points, bonds),
            bonds,
        )
    for sample in range(5000):
        spatial = sample % 2 == 0
        points = [(x, y, rng.uniform(-1, 1) if spatial else 0) for x, y, _ in positions]
        # Degenerate frames, collinear/same-side end bonds and threshold cases.
        if sample % 7 == 0:
            points[1] = (rng.choice((0, 1e-8, 1e-7, 1.00001e-7, 1.5)), 0, 0)
        if sample % 11 == 0:
            points[2] = (-1, 0, 0)
        if sample % 13 == 0:
            points[3] = points[2]
        if sample % 17 == 0:
            points[4] = (2.5, 1, rng.choice((-1e-8, -1e-7, 0, 1e-7, 1e-8)))
        # Rotate axes through x/y/z and reflect one coordinate.
        shift = sample % 3
        points = [p[shift:] + p[:shift] if spatial else p for p in points]
        points = [(x, y * (-1 if sample % 5 == 0 else 1), z) for x, y, z in points]
        atom_order = list(range(6))
        rng.shuffle(atom_order)
        mapping = {old: new for new, old in enumerate(atom_order)}
        bonds = []
        for a, b, order in base:
            a, b = mapping[a], mapping[b]
            if rng.randrange(2):
                a, b = b, a
            bonds.append((a, b, order, rng.randrange(4)))
        rng.shuffle(bonds)
        yield (
            f"geometry/{sample}",
            block(["C"] * 6, [points[i] for i in atom_order], bonds, spatial),
            bonds,
        )


def emit_molecule(name, mol, directions):
    expected = metadata(mol)
    before = copy.deepcopy(expected)
    for bond in before["bonds"]:
        if bond["stereo"] in (6, 7):
            bond["stereo"] = 0
    conformer = None
    if mol.GetNumConformers():
        conf = mol.GetConformer()
        conformer = dict(
            is_3d=conf.Is3D(),
            positions=[
                dict(x=p.x, y=p.y, z=p.z)
                for p in (conf.GetAtomPosition(i) for i in range(mol.GetNumAtoms()))
            ],
        )
    print(
        json.dumps(
            dict(
                name=name,
                graph=graph(mol),
                metadata=before,
                directions=directions,
                conformer=conformer,
                expected=expected,
            )
        )
    )


def without_coordinates():
    # CXSMILES without coordinates calls the same detector with a null conformer.
    params = Chem.SmilesParserParams()
    params.sanitize = False
    params.removeHs = False
    rng = random.Random(943518)
    for smiles in (
        "CC(=C)C(C)=C",
        "Fc1cccc(F)c1-c1c(Cl)cccc1Cl",
        "CC(O)Cl",
        "C[S@](=O)CC",
        "CC(N)=C(C)O",
    ):
        base = Chem.MolFromSmiles(smiles)
        authored = Chem.MolToSmiles(base, canonical=False)
        atom_order = json.loads(base.GetProp("_smilesAtomOutputOrder"))
        bond_order = json.loads(base.GetProp("_smilesBondOutputOrder"))
        atom_index = {old: new for new, old in enumerate(atom_order)}
        for sample in range(500):
            records = []
            for index, bond_id in enumerate(bond_order):
                bond = base.GetBondWithIdx(bond_id)
                direction = rng.choice(("none", "wedge", "hash", "unknown"))
                if direction != "none":
                    endpoint = rng.choice((bond.GetBeginAtomIdx(), bond.GetEndAtomIdx()))
                    kind = {"wedge": "wU", "hash": "wD", "unknown": "w"}[direction]
                    records.append(f"{kind}:{atom_index[endpoint]}.{index}")
            text = authored + (" |" + ",".join(records) + "|" if records else "")
            mol = Chem.MolFromSmiles(text, params)
            if mol is None:
                raise ValueError(f"Invalid authored CXSMILES fixture: {text}")
            directions = [
                {0: "none", 1: "wedge", 2: "unknown", 3: "hash"}[
                    bond.GetIntProp("_MolFileBondCfg") if bond.HasProp("_MolFileBondCfg") else 0
                ]
                for bond in mol.GetBonds()
            ]
            emit_molecule(f"no coordinates/{smiles}/{sample}", mol, directions)


def emit(name, text, bonds):
    # The separate full import test exercises native rejections. Here, compare
    # exact axial labels even when the app's editable contract rejects them.
    try:
        mol = Chem.MolFromMolBlock(text, sanitize=False, removeHs=False, strictParsing=True)
    except (RuntimeError, ValueError):
        return
    if mol is None:
        return
    directions = []
    for _, _, order, cfg in bonds:
        directions.append(
            "wedge"
            if cfg == 1
            else "hash"
            if cfg == 3
            else "unknown"
            if cfg == 2 and order == 1
            else "either_double"
            if cfg == 2 and order == 2
            else "none"
        )
    emit_molecule(name, mol, directions)


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    without_coordinates()
    for name, text, bonds in cases():
        emit(name, text, bonds)


if __name__ == "__main__":
    main()
