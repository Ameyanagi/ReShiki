"""Independent RDKit pi-electron, conjugation and hybridization expectations."""

import json
import random
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDConfig, RDLogger, rdBase

if TYPE_CHECKING or __package__:
    from .valence_reference import ORDERS, atom_molecule, graph
else:
    from valence_reference import ORDERS, atom_molecule, graph


def emit(name, original, assign_conjugation=True):
    mol = Chem.Mol(original)
    before = graph(mol)
    tags = [int(a.GetChiralTag()) for a in mol.GetAtoms()]
    # Check that assignment discards stale conjugation, and separately check
    # hybridization with a caller-supplied conjugation vector.
    for bond in mol.GetBonds():
        bond.SetIsConjugated(bond.GetIdx() % 2 == 0)
    flags = [b.GetIsConjugated() for b in mol.GetBonds()]
    try:
        mol.UpdatePropertyCache(strict=False)
        electrons = [Chem.CountAtomElec(a) for a in mol.GetAtoms()]
        if assign_conjugation:
            Chem.SetConjugation(mol)
        Chem.SetHybridization(mol)
        expected = dict(
            electrons=electrons,
            conjugated=[b.GetIsConjugated() for b in mol.GetBonds()],
            hybridization=[str(a.GetHybridization()) for a in mol.GetAtoms()],
            graph=graph(mol),
            tags=[int(a.GetChiralTag()) for a in mol.GetAtoms()],
        )
    except (ValueError, RuntimeError):
        expected = None
    print(
        json.dumps(
            dict(
                name=f"{name}/{assign_conjugation}",
                graph=before,
                tags=tags,
                flags=flags,
                assign_conjugation=assign_conjugation,
                expected=expected,
            )
        )
    )


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    emit("empty", Chem.Mol())
    for number in range(119):
        for charge in (-128, -4, -1, 0, 1, 4, 127):
            for hydrogens in (0, 1, 3, 6):
                for radical in (0, 1, 2):
                    for no_implicit in (False, True):
                        emit(
                            f"atom {number}/{charge}/{hydrogens}/{radical}/{no_implicit}",
                            atom_molecule(number, charge, hydrogens, radical, no_implicit),
                        )
        for charge in (-1, 0, 1):
            for degree in (1, 2, 3, 4, 6):
                for order in ORDERS:
                    for reverse in (False, True) if order == 5 else (False,):
                        mol = atom_molecule(number, charge, 0, 0, False)
                        for i in range(degree):
                            other = mol.AddAtom(Chem.Atom(6 if i % 2 == 0 else 8))
                            a, b = (other, 0) if reverse else (0, other)
                            mol.AddBond(a, b, ORDERS[order])
                        name = f"star {number}/{charge}/{degree}/{order}/{reverse}"
                        emit(name, mol)
                        if degree == 3:
                            emit(name, mol, False)
    # Valid and invalid coordination numbers for every supported chiral tag.
    for number in (0, 1, 6, 7, 15, 16, 26, 46, 78, 89, 118):
        for tag in range(9):
            for degree in range(8):
                for order in (1, 5):
                    for hydrogens in (0, 1, 2):
                        mol = atom_molecule(number, 0, hydrogens, 0, True)
                        mol.GetAtomWithIdx(0).SetChiralTag(Chem.ChiralType.values[tag])
                        for _ in range(degree):
                            other = mol.AddAtom(Chem.Atom(7))
                            mol.AddBond(other, 0, ORDERS[order])
                        emit(f"coordination {number}/{tag}/{degree}/{order}/{hydrogens}", mol)
    # Two bonds meeting at a possible pi center, including noncontributing bonds.
    for number in (0, 5, 6, 7, 8, 9, 14, 15, 16, 33, 34, 52):
        for other in (6, 7, 8, 15, 16):
            for first in (1, 2, 3, 4, 5, 7):
                for second in ORDERS:
                    mol = atom_molecule(number, 0, 0, 0, False)
                    mol.AddAtom(Chem.Atom(other))
                    mol.AddAtom(Chem.Atom(other))
                    mol.AddBond(0, 1, ORDERS[first])
                    mol.AddBond(2, 0, ORDERS[second])
                    emit(f"pair {number}/{other}/{first}/{second}", mol)
    # RDKit molopstest.cpp regressions, BSD-3-Clause (licenses/rdkit/).
    texts = [
        "Pc1ccccc1",
        "CP1(C)=CC=CN=C1C",
        "O=CO",
        "CCC",
        "CNC",
        "COC",
        "C[C-2]C",
        "C[CH-]C",
        "C[CH]C",
        "C[C]C",
        "C[C-]C",
        "C[CH+]C",
        "CC=C",
        "CN=C",
        "C[C-]=C",
        "C[C]=C",
        "C[N+]=C",
        "C#C",
        "C#[C-]",
        "C#[C]",
        "C[O]",
        "C[N-]",
        "C=C-[CH2+]",
        "C1=C[CH+]1",
        "C1=CC=C[CH+]C=C1",
        "c1c[cH+]1",
        "c1ccc[cH+]cc1",
        "C=C-C",
        "C=C-O",
        "C=C-N",
        "C=C-[NH3+]",
        "Cc1ccccc1",
        "Fc1c[nH]c(=O)[nH]c1=O",
        "[Pt@SP1](Cl)(F)(Br)I",
        "[P@TB1](F)(Cl)(Br)(I)N",
        "[Co@OH1](N)(O)(F)(Cl)(Br)I",
        "[2H][C@]([3H])(F)Cl",
        "N->[Cu+2]<-N",
        "[O-][N+](=O)c1ccccc1",
        "C12C3C4C1C5C2C3C45",
    ]
    templates = json.loads(
        (Path(__file__).resolve().parents[1] / "assets/templates.json").read_text()
    )
    texts += [t["smiles"] for t in templates if t.get("smiles")]
    texts += [
        line.split()[0]
        for line in (Path(RDConfig.RDDataDir) / "NCI/first_5K.smi").read_text().splitlines()
    ]
    rng = random.Random(79614)
    for i, text in enumerate(texts):
        raw = Chem.MolFromSmiles(text, sanitize=False)
        if raw is None:
            raise ValueError(f"Invalid reference syntax: {text}")
        emit(text + " raw", raw)
        mol = Chem.MolFromSmiles(text)
        if mol is None:
            continue  # Invalid chemistry remains covered by its raw graph.
        emit(text, mol)
        if i < 1000:
            order = list(range(mol.GetNumAtoms()))
            rng.shuffle(order)
            emit(text + " permuted", Chem.RenumberAtoms(mol, order))
            kekule = Chem.Mol(mol)
            Chem.Kekulize(kekule, clearAromaticFlags=True)
            emit(text + " kekule", kekule)
        if i < 200:
            emit(text + " explicit H", Chem.AddHs(mol))
        if i < 300:
            emit(text + " external conjugation", mol, False)


if __name__ == "__main__":
    main()
