"""Stream independent RDKit property-cache/radical expectations, not worker output."""

import json
from pathlib import Path

from rdkit import Chem, RDLogger, rdBase

ORDERS = {
    0: Chem.BondType.HYDROGEN,
    1: Chem.BondType.SINGLE,
    2: Chem.BondType.DOUBLE,
    3: Chem.BondType.TRIPLE,
    4: Chem.BondType.AROMATIC,
    5: Chem.BondType.DATIVE,
    6: Chem.BondType.QUADRUPLE,
    7: Chem.BondType.ONEANDAHALF,
}


def graph(mol):
    return {
        "atoms": [
            dict(
                atomic_number=a.GetAtomicNum(),
                isotope=a.GetIsotope(),
                charge=a.GetFormalCharge(),
                explicit_hydrogens=a.GetNumExplicitHs(),
                no_implicit=a.GetNoImplicit(),
                aromatic=a.GetIsAromatic(),
                radical_electrons=a.GetNumRadicalElectrons(),
            )
            for a in mol.GetAtoms()
        ],
        "bonds": [
            dict(
                a=b.GetBeginAtomIdx(),
                b=b.GetEndAtomIdx(),
                order=next(order for order, kind in ORDERS.items() if b.GetBondType() == kind),
                aromatic=b.GetIsAromatic(),
            )
            for b in mol.GetBonds()
        ],
    }


def emit(name, mol, operation="cache"):
    source = graph(mol)
    try:
        if operation == "radicals":
            Chem.SanitizeMol(mol, sanitizeOps=Chem.SanitizeFlags.SANITIZE_FINDRADICALS)
            expected = [a.GetNumRadicalElectrons() for a in mol.GetAtoms()]
        else:
            mol.UpdatePropertyCache(strict=operation != "provisional")
            expected = [
                dict(
                    explicit_valence=a.GetValence(Chem.ValenceType.EXPLICIT),
                    implicit_hydrogens=a.GetNumImplicitHs(),
                )
                for a in mol.GetAtoms()
            ]
    except (ValueError, RuntimeError):
        expected = None
    print(json.dumps(dict(name=name, operation=operation, graph=source, expected=expected)))


def atom_molecule(number, charge, hydrogens, radical, no_implicit, aromatic=False):
    mol = Chem.RWMol()
    atom = Chem.Atom(number)
    atom.SetFormalCharge(charge)
    atom.SetNumExplicitHs(hydrogens)
    atom.SetNumRadicalElectrons(radical)
    atom.SetNoImplicit(no_implicit)
    atom.SetIsAromatic(aromatic)
    mol.AddAtom(atom)
    return mol


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    emit("empty", Chem.RWMol())
    for number in range(119):
        for charge in (-128, -8, -4, -2, -1, 0, 1, 2, 4, 8, 127):
            for hydrogens in (0, 1, 2, 3, 4, 5, 6, 8):
                for radical in (0, 1, 2):
                    for no_implicit, aromatic in ((False, False), (True, False), (False, True)):
                        name = (
                            f"atom {number}/{charge}/{hydrogens}/{radical}/{no_implicit}/{aromatic}"
                        )
                        mol = atom_molecule(
                            number, charge, hydrogens, radical, no_implicit, aromatic
                        )
                        emit(name, mol)
                        emit(name, mol, "provisional")
                        if no_implicit:
                            # The radical pass reads the unrounded bond-order sum.
                            emit(name, mol, "radicals")
        for charge in (-2, 0, 2):
            for degree in (1, 2, 3, 4, 6, 8):
                for order in ORDERS:
                    for reverse in (False, True) if order == 5 else (False,):
                        mol = atom_molecule(number, charge, 0, 0, False)
                        for i in range(degree):
                            leaf = Chem.Atom(0)
                            leaf.SetNoImplicit(True)
                            mol.AddAtom(leaf)
                            a, b = (i + 1, 0) if reverse else (0, i + 1)
                            mol.AddBond(a, b, ORDERS[order])
                        name = f"star {number}/{charge}/{degree}/{order}/{reverse}"
                        emit(name, mol)
                        emit(name, mol, "provisional")
                        mol.GetAtomWithIdx(0).SetNoImplicit(True)
                        emit(name, mol, "radicals")

    templates = json.loads(
        (Path(__file__).resolve().parents[1] / "assets/templates.json").read_text()
    )
    smiles = [t["smiles"] for t in templates if t.get("smiles")]
    smiles += [
        "[H]",
        "[H-]",
        "[H+]",
        "[H][H]",
        "[2H]O[3H]",
        "[13CH3][NH3+]",
        "[CH3]",
        "[CH2]",
        "[O][O]",
        "[C-]#[O+]",
        "[NH4+]",
        "[BH4-]",
        "[PH6-]",
        "[SH5-]",
        "[SeH5-]",
        "[AsH6-]",
        "[SiH4]",
        "[Fe+3]",
        "[Ca+2].[Cl-].[Cl-]",
        "N->[Cu+2]<-N",
        "[O-][N+](=O)c1ccccc1",
        "c1cc[nH]c1",
        "c1cc[n+]([O-])cc1",
        "O=c1cc[nH]cc1",
        "C1ccccC1",
        "c1ccn2cncc2c1",
        "[N]1C=CC=C1",
        "[c]1ccccc1",
        "[13*]C",
        "*C",
        "[2H][C@]([3H])(F)Cl",
        "F/C=C/F",
        "F/C=C\\F",
        "C" * 250,
    ]
    for text in smiles:
        mol = Chem.MolFromSmiles(text)
        if mol is None:
            raise ValueError(f"Invalid oracle SMILES: {text}")
        kekule = Chem.Mol(mol)
        Chem.Kekulize(kekule, clearAromaticFlags=True)
        for suffix, variant in (("", mol), (" graph H", Chem.AddHs(mol)), (" kekule", kekule)):
            emit(text + suffix, Chem.Mol(variant))
            emit(text + suffix, Chem.Mol(variant), "radicals")
            emit(
                text + suffix + " reversed",
                Chem.RenumberAtoms(variant, list(reversed(range(variant.GetNumAtoms())))),
            )


if __name__ == "__main__":
    main()
