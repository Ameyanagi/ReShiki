"""Independent graph transformations from the pinned RDKit sanitization passes."""

import itertools
import json
import random
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDConfig, RDLogger, rdBase

if TYPE_CHECKING or __package__:
    from .valence_reference import ORDERS, atom_molecule, graph
else:
    from valence_reference import ORDERS, atom_molecule, graph


def emit(name, mol):
    before = graph(mol)
    Chem.Cleanup(mol)
    print(json.dumps(dict(name=name, graph=before, expected=graph(mol))))


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    emit("empty", Chem.RWMol())
    for number in (7, 15, 17, 35, 53):
        for charge in (-1, 0, 1):
            for orders in ((2, 3), (1, 2, 2), (1, 1, 3), (1, 2), (1, 2, 2, 2)):
                for elements in itertools.product((6, 7, 8), repeat=len(orders)):
                    for neighbor_charge in (0, -1, 1):
                        mol = atom_molecule(number, charge, 0, 0, True)
                        for other, order in zip(elements, orders, strict=True):
                            atom = Chem.Atom(other)
                            atom.SetFormalCharge(neighbor_charge)
                            atom.SetNoImplicit(True)
                            idx = mol.AddAtom(atom)
                            mol.AddBond(0, idx, ORDERS[order])
                            # P cleanup requires the double-bonded C/N to have
                            # another neighbor, not an isolated terminal atom.
                            if other in (6, 7):
                                branch = mol.AddAtom(Chem.Atom(6))
                                mol.AddBond(idx, branch, Chem.BondType.SINGLE)
                        name = f"group {number}/{charge}/{orders}/{elements}/{neighbor_charge}"
                        emit(name, mol)
    examples = [
        "CN(=O)=O",
        "C1=CC=CN(=O)=C1",
        "CN=N#N",
        "O=N#N",
        "N#N=N#N",
        "C[P](=O)=CC",
        "CP(=O)=NC",
        "CP(=O)=N",
        "OP(=O)=CC",
        "O=N(=O)ON(=O)=O",
        "O=[n+]1occcc1",
        "O=n1ccccc1",
        "O=N1C=CC=C1",
        "Cl(=O)(=O)(=O)O",
        "Br(=O)(=O)O",
        "I(=O)O",
        "Cl(=O)(=O)C",
        "[O-]=Cl(=O)O",
        "O=[Cl+](=O)O",
    ]
    templates = json.loads(
        (Path(__file__).resolve().parents[1] / "assets/templates.json").read_text()
    )
    examples += [t["smiles"] for t in templates if t.get("smiles")]
    examples += [
        line.split()[0]
        for line in (Path(RDConfig.RDDataDir) / "NCI/first_5K.smi").read_text().splitlines()
    ]
    rng = random.Random(82154)
    for text in examples:
        mol = Chem.MolFromSmiles(text, sanitize=False)
        if mol is None:
            raise ValueError(f"Invalid reference syntax: {text}")
        emit(text, Chem.Mol(mol))
        order = list(range(mol.GetNumAtoms()))
        rng.shuffle(order)
        emit(text + " permuted", Chem.RenumberAtoms(mol, order))
        # Atom renumbering keeps the relative bond insertion order. Rebuild
        # separately to exercise the rule's choice of first/last neighbor.
        reverse = Chem.RWMol(mol)
        bonds = [
            (b.GetBeginAtomIdx(), b.GetEndAtomIdx(), b.GetBondType(), b.GetIsAromatic())
            for b in mol.GetBonds()
        ]
        for a, b, _, _ in bonds:
            reverse.RemoveBond(a, b)
        for a, b, kind, aromatic in reversed(bonds):
            reverse.AddBond(a, b, kind)
            reverse.GetBondBetweenAtoms(a, b).SetIsAromatic(aromatic)
        emit(text + " reversed bonds", reverse)


if __name__ == "__main__":
    main()
