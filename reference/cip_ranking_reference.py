"""Legacy stereo atom priorities from RDKit's direct API, independent of the worker."""

import json
import random
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDConfig, RDLogger, rdBase

if TYPE_CHECKING or __package__:
    from .ranking_reference import metadata
    from .valence_reference import ORDERS, graph
else:
    from ranking_reference import metadata
    from valence_reference import ORDERS, graph


def emit(name, original):
    mol = Chem.Mol(original)
    source, meta = graph(mol), metadata(mol)
    try:
        mol.UpdatePropertyCache(strict=False)
        expected = list(Chem.ComputeAtomCIPRanks(mol))
    except (ValueError, RuntimeError):
        expected = None
    if source != graph(mol) or meta != metadata(mol):
        raise AssertionError("Priority assignment changed source graph or stereo metadata")
    print(json.dumps(dict(name=name, graph=source, metadata=meta, expected=expected)))


def pair(number):
    mol = Chem.RWMol()
    for _ in range(2):
        a = Chem.Atom(number)
        a.SetNoImplicit(True)
        mol.AddAtom(a)
    return mol


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    rng = random.Random(619792)
    table = Chem.GetPeriodicTable()
    for number in range(119):
        for isotope in (
            0,
            1,
            2,
            12,
            13,
            511,
            512,
            1023,
            1024,
            65535,
            table.GetMostCommonIsotope(number),
        ):
            for mapping in (None, -1, 0, 1, 1022, 1023, 1024, 2147483646, 2147483647):
                mol = pair(number)
                mol.GetAtomWithIdx(0).SetIsotope(isotope)
                if mapping is not None:
                    mol.GetAtomWithIdx(0).SetIntProp("molAtomMapNumber", mapping)
                emit(f"isotope/map {number}/{isotope}/{mapping}", mol)
        for isotope in range(1, 401):
            if table.GetMassForIsotope(number, isotope):
                mol = pair(number)
                mol.GetAtomWithIdx(0).SetIsotope(isotope)
                emit(f"known isotope {number}/{isotope}", mol)
        for charge in (-128, -8, -2, -1, 0, 1, 2, 8, 127):
            for h in (0, 1, 2, 4, 8, 255):
                mol = pair(number)
                first = mol.GetAtomWithIdx(0)
                first.SetFormalCharge(charge)
                first.SetNumExplicitHs(h)
                first.SetNumRadicalElectrons(rng.randrange(3))
                first.SetNoImplicit(False)
                emit(f"charge/H {number}/{charge}/{h}", mol)
    for phosphorus_degree in (1, 2, 3, 4, 5, 6):
        for order in ORDERS:
            mol = Chem.RWMol()
            for number in (6, 15, 16, 6, 6, 6, 6, 6, 6):
                a = Chem.Atom(number)
                a.SetNoImplicit(True)
                mol.AddAtom(a)
            mol.AddBond(0, 1, ORDERS[order])
            mol.AddBond(0, 2, ORDERS[order])
            for i in range(phosphorus_degree - 1):
                mol.AddBond(1, 3 + i, Chem.BondType.SINGLE)
            emit(f"P special {phosphorus_degree}/{order}", mol)
    for sample in range(1500):
        n = rng.randrange(2, 15)
        mol = Chem.RWMol()
        for _ in range(n):
            a = Chem.Atom(rng.choice((0, 1, 5, 6, 6, 7, 8, 9, 14, 15, 16, 26, 78)))
            a.SetNoImplicit(True)
            a.SetNumExplicitHs(rng.randrange(4))
            a.SetFormalCharge(rng.randrange(-2, 3))
            a.SetIsAromatic(rng.random() < 0.1)
            a.SetNumRadicalElectrons(rng.randrange(3))
            mol.AddAtom(a)
        edges = set()
        for b in range(1, n):
            a = rng.randrange(b)
            edges.add((a, b))
        for _ in range(n):
            a, b = sorted(rng.sample(range(n), 2))
            edges.add((a, b))
        for a, b in sorted(edges):
            mol.AddBond(a, b, rng.choice(list(ORDERS.values())))
        emit(f"synthetic {sample}", mol)
    texts = [
        "OC[C@H](C)O",
        "N[C@@H](C)C(=O)O",
        "O=P(C)(C)C",
        "O=P(C)C",
        "F/C=C/C=C/Cl",
        "[2H]C([3H])(F)Cl",
        "[Cu]<-NCCN->[Cu]",
        "C1CC2CCC1C2",
        "C1=CC=[C@]=CC=C1",
        "[13CH3][C@H]([12CH3])F",
        "[NH4+].[Cl-]",
    ]
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
        emit(text, mol)
        order = list(range(mol.GetNumAtoms()))
        rng.shuffle(order)
        emit(text + " permuted", Chem.RenumberAtoms(mol, order))
        emit(text + " explicit H", Chem.AddHs(mol))
        for a in mol.GetAtoms():
            if rng.random() < 0.3:
                a.SetIntProp("molAtomMapNumber", rng.choice((-1, 0, 1, 3, 1023, 1024)))
            if rng.random() < 0.3:
                a.SetIsotope(rng.choice((0, 2, 13, 14, 18, 57, 999)))
            a.SetUnsignedProp("_CIPRank", rng.randrange(1000))
            a.SetProp("_CIPCode", rng.choice(("R", "S", "r", "s")))
        emit(text + " isotope/map/stale priorities", mol)


if __name__ == "__main__":
    main()
