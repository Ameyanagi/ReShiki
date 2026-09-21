"""Independent bond-assignment oracle; canonical ranks are supplied, not migrated."""

import json
import random
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDConfig, RDLogger, rdBase

if TYPE_CHECKING or __package__:
    from .valence_reference import ORDERS, graph
else:
    from valence_reference import ORDERS, graph

DIRECTIONS = {
    "none": Chem.BondDir.NONE,
    "wedge": Chem.BondDir.BEGINWEDGE,
    "hash": Chem.BondDir.BEGINDASH,
    "down": Chem.BondDir.ENDDOWNRIGHT,
    "up": Chem.BondDir.ENDUPRIGHT,
    "either_double": Chem.BondDir.EITHERDOUBLE,
    "unknown": Chem.BondDir.UNKNOWN,
}


def directions(mol):
    return [next(n for n, d in DIRECTIONS.items() if d == b.GetBondDir()) for b in mol.GetBonds()]


def emit(name, original, clear=True, canonical=False):
    mol = Chem.Mol(original)
    mol.UpdatePropertyCache(strict=False)
    Chem.GetSymmSSSR(mol)
    ranks = None
    if canonical:
        # In the pinned rdmolfiles.cpp wrapper, includeChiralPresence is
        # forwarded into the C++ includeAtomMaps slot. True matches the
        # rankFragmentAtoms defaults used inside Kekulize (maps included,
        # chiral presence excluded). Keep mapped-atom regressions below.
        ranks = list(
            Chem.CanonicalRankAtomsInFragment(
                mol,
                atomsToUse=list(range(mol.GetNumAtoms())),
                bondsToUse=list(range(mol.GetNumBonds())),
                includeChiralPresence=True,
            )
        )
    source = graph(mol)
    rings = list(mol.GetRingInfo().AtomRings())
    dirs = directions(mol)
    optional = Chem.Mol(mol)
    try:
        Chem.Kekulize(mol, clearAromaticFlags=clear, canonical=canonical)
        expected = dict(graph=graph(mol), directions=directions(mol))
    except (ValueError, RuntimeError):
        expected = None
    try:
        Chem.KekulizeIfPossible(optional, clearAromaticFlags=clear, canonical=canonical)
        attempt = dict(
            assignment=dict(graph=graph(optional), directions=directions(optional)),
            success=expected is not None,
        )
    except (ValueError, RuntimeError):
        attempt = None
    print(
        json.dumps(
            dict(
                name=f"{name}/{clear}/{canonical}",
                graph=source,
                rings=rings,
                directions=dirs,
                clear=clear,
                ranks=ranks,
                expected=expected,
                attempt=attempt,
            )
        )
    )


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    emit("empty", Chem.RWMol())
    for size in range(3, 10):
        for number in range(119):
            for charge in (-1, 0, 1):
                mol = Chem.RWMol()
                for i in range(size):
                    atom = Chem.Atom(number if i == 0 else 6)
                    atom.SetIsAromatic(True)
                    if i == 0:
                        atom.SetFormalCharge(charge)
                    mol.AddAtom(atom)
                for i in range(size):
                    mol.AddBond(i, (i + 1) % size, Chem.BondType.AROMATIC)
                emit(f"ring {size}/{number}/{charge}", mol)
                emit(f"ring {size}/{number}/{charge}", mol, clear=False)
    texts = [
        "c1ccccc1",
        "c1cc[nH]c1",
        "[nH]1cccc1",
        "[pH]1cccc1",
        "[se]1cccc1",
        "c1ccc2occc2c1",
        "c1ccc2ccccc2c1",
        "c1cc2ccc3cccc4ccc(c1)c2c34",
        "C12C3C4C1C5C2C3C45",
        "*1ccccc1",
        "*1cccc1",
        "*1:*:*:*:*:*:1",
        "*1c*cc1",
        "*1c*c*1",
        "*1ccc2cccc*12",
        "c1cccc1",
        "c",
        "cc",
        "c1cccc[n+]1",
        "[n]1ccccc1",
        "[H]n1cccc1",
        "[cH-]1cccc1",
        "[13cH]1ccccc1",
        "[c:12]1ccccc1",
        "c1ccc([C@H](Cl)F)cc1",
        "C/C=C/c1ccccc1",
        # RDKit catch_graphmol.cpp #8403/#8606: fused systems requiring
        # backtracking or canonical retry (BSD-3-Clause; licenses/rdkit/).
        "c1cc2ccc3c4c(ccc(c1)c24)c1c2c4ccc5cccc6ccc(c4c65)c4c5cccc6c7cccc8c9cc"
        "cc%10c%11cccc%12c3c1c1c(c%11%12)c(c9%10)c(c87)c(c65)c1c42",
        "O=c1c2c3c(c4c(c2c(=O)c2c5c(c6c(c12)c1c2c6cccc2ccc1)c1c2c5cccc2ccc1)c1"
        "c2c4cccc2ccc1)c1c2c3cccc2ccc1",
    ]
    templates = json.loads(
        (Path(__file__).resolve().parents[1] / "assets/templates.json").read_text()
    )
    texts += [t["smiles"] for t in templates if t.get("smiles")]
    # Exercise bond direction preservation, including direction on bonds changed
    # by backtracking, and ring/neighbor traversal under atom permutations.
    rng = random.Random(4593)
    for text in texts:
        mol = Chem.MolFromSmiles(text, sanitize=False)
        if mol is None:
            raise ValueError(f"Invalid corpus syntax: {text}")
        for direction_name, direction in DIRECTIONS.items():
            for bond in mol.GetBonds():
                bond.SetBondDir(direction if bond.GetIdx() % 3 == 0 else Chem.BondDir.NONE)
            for clear in (False, True):
                emit(f"{text} {direction_name}", mol, clear=clear)
                emit(f"{text} {direction_name}", mol, clear=clear, canonical=True)
    for sample in range(3000):
        # Intermediate and malformed graphs: aromatic atom flags, bond flags,
        # bond orders, radicals and implicit-H policy can disagree on input.
        base = Chem.MolFromSmiles(rng.choice(texts[:20]), sanitize=False)
        mol = Chem.RWMol()
        for old in base.GetAtoms():
            atom = Chem.Atom(old.GetAtomicNum())
            if rng.random() < 0.35:
                atom.SetAtomicNum(rng.choice((0, 5, 6, 7, 8, 15, 16, 33)))
            atom.SetIsAromatic(rng.random() < 0.8)
            atom.SetNoImplicit(rng.random() < 0.4)
            atom.SetNumExplicitHs(rng.choice((0, 0, 0, 1, 2)))
            atom.SetNumRadicalElectrons(rng.choice((0, 0, 0, 1)))
            atom.SetFormalCharge(rng.choice((-1, 0, 0, 0, 1)))
            mol.AddAtom(atom)
        for old in base.GetBonds():
            a, b = old.GetBeginAtomIdx(), old.GetEndAtomIdx()
            if rng.random() < 0.5:
                a, b = b, a
            order = rng.choice((4, 4, 4, 1, 2, 3, 0, 5, 6, 7))
            mol.AddBond(a, b, ORDERS[order])
            bond = mol.GetBondBetweenAtoms(a, b)
            bond.SetIsAromatic(rng.random() < 0.8)
            bond.SetBondDir(rng.choice(list(DIRECTIONS.values())))
        emit(f"mixed intermediate graph {sample}", mol, clear=sample % 2 == 0)
    texts += [
        line.split()[0]
        for line in (Path(RDConfig.RDDataDir) / "NCI/first_5K.smi").read_text().splitlines()
    ]
    for text in texts:
        mol = Chem.MolFromSmiles(text, sanitize=False)
        if mol is None:
            raise ValueError(f"Invalid corpus syntax: {text}")
        for clear in (False, True):
            emit(text + " raw", mol, clear=clear)
            emit(text + " raw", mol, clear=clear, canonical=True)
        order = list(range(mol.GetNumAtoms()))
        rng.shuffle(order)
        emit(text + " permuted", Chem.RenumberAtoms(mol, order))
        try:
            Chem.SanitizeMol(mol)
        except (ValueError, RuntimeError):
            continue
        emit(text + " sanitized", mol)
        emit(text + " canonical", mol, canonical=True)
        emit(text + " graph H", Chem.AddHs(mol))


if __name__ == "__main__":
    main()
