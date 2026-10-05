"""Independent RDKit metal cleanup and exact fast-cycle traversal oracle."""

import json
import random
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDConfig, RDLogger, rdBase

if TYPE_CHECKING or __package__:
    from .kekulize_reference import directions
    from .ranking_reference import metadata
    from .valence_reference import ORDERS, atom_molecule, graph
else:
    from kekulize_reference import directions
    from ranking_reference import metadata
    from valence_reference import ORDERS, atom_molecule, graph


def emit(name, original, cache=None):
    mol = Chem.Mol(original)
    mol.ClearComputedProps(includeRings=True)
    fast = Chem.Mol(mol)
    Chem.FastFindRings(fast)
    fast_atoms = list(fast.GetRingInfo().AtomRings())
    fast_bonds = list(fast.GetRingInfo().BondRings())
    if cache == "symmetric":
        Chem.GetSymmSSSR(mol)
    elif cache == "basis":
        Chem.GetSSSR(mol)
    elif cache == "fast":
        Chem.FastFindRings(mol)
    cached_rings = list(mol.GetRingInfo().AtomRings()) if cache else None
    before, meta, dirs = graph(mol), metadata(mol), directions(mol)
    try:
        Chem.CleanupOrganometallics(mol)
        expected = dict(graph=graph(mol), metadata=metadata(mol), directions=directions(mol))
    except (ValueError, RuntimeError):
        expected = None
    print(
        json.dumps(
            dict(
                name=f"{name}/{cache}",
                graph=before,
                metadata=meta,
                directions=dirs,
                cached_rings=cached_rings,
                fast_atoms=fast_atoms,
                fast_bonds=fast_bonds,
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
            for degree in (0, 1, 4, 6):
                for other in (6, 26, 29):
                    mol = atom_molecule(number, charge, 0, 0, True)
                    for i in range(degree):
                        leaf = Chem.Atom(other)
                        leaf.SetNoImplicit(True)
                        mol.AddAtom(leaf)
                        mol.AddBond(0, i + 1, Chem.BondType.SINGLE)
                    emit(f"star {number}/{charge}/{degree}/{other}", mol)
    # Classify every possible metal endpoint as well as every donor element.
    for number in range(119):
        for donor in (0, 6, 7, 8, 15, 16, 17, 52, 85):
            for reverse in (False, True):
                mol = atom_molecule(donor, 0, 0, 0, False)
                for _ in range(4):
                    index = mol.AddAtom(Chem.Atom(6))
                    mol.AddBond(0, index, Chem.BondType.SINGLE)
                metal = mol.AddAtom(Chem.Atom(number))
                a, b = (metal, 0) if reverse else (0, metal)
                mol.AddBond(a, b, Chem.BondType.SINGLE)
                emit(f"endpoint {number}/{donor}/{reverse}", mol)
    # Source regressions from RDKit catch_graphmol.cpp, BSD-3-Clause (licenses/rdkit/).
    texts = [
        "CC1=C(CCC(O)=O)C2=[N]3C1=Cc1c(C)c(C=C)c4C=C5C(C)=C(C=C)C6=[N]5[Fe]3(n14)n1c(=C6)c(C)c(CCC(O)=O)c1=C2",
        "CC1=C(CCC([O-])=O)C2=[N+]3C1=Cc1c(C)c(C=C)c4C=C5C(C)=C(C=C)C6=[N+]5[Fe--]3(n14)n1c(=C6)c(C)c(CCC([O-])=O)c1=C2",
        "CCC1=[O+][Cu]2([O+]=C(CC)C1)[O+]=C(CC)CC(CC)=[O+]2",
        "F[Pd](Cl)(Cl1)Cl[Pd]1(Cl)Cl",
        "F[Pt]1(F)[35Cl][Pt]([Cl]1)(F)Br",
        "C[O](C)*",
        "c1ccccn1[Fe]",
        "c1cccc[n+]1[Fe]",
        "C[N](C)(C)[Cu]",
        "C[O](C)[Fe]",
        "C[C@H](N(C)(C)[Fe])O",
        "C[C@H](O[Fe])C[C@@H](O[Fe])C |&1:1,5|",
        "c1cccc1[Fe]",
        "[C-]1([Fe])=CC=CC1",
        "C12C3C4C1C5C2C3C45",
    ]
    rng = random.Random(4822)
    for text in texts:
        mol = Chem.MolFromSmiles(text, sanitize=False)
        if mol is None:
            raise ValueError(f"Invalid reference syntax: {text}")
        for cache in (None, "fast", "basis", "symmetric"):
            emit(text, mol, cache)
            for i in range(12):
                order = list(range(mol.GetNumAtoms()))
                rng.shuffle(order)
                emit(f"{text} permuted {i}", Chem.RenumberAtoms(mol, order), cache)
    for sample in range(2000):
        mol = Chem.RWMol()
        metals = [
            mol.AddAtom(Chem.Atom(rng.choice((3, 13, 26, 29, 30, 46, 78, 80))))
            for _ in range(rng.randrange(2, 5))
        ]
        for _ in range(rng.randrange(2, 7)):
            atom = Chem.Atom(rng.choice((0, 6, 7, 8, 9, 15, 16, 17)))
            atom.SetNoImplicit(rng.random() < 0.5)
            atom.SetNumExplicitHs(rng.randrange(3))
            atom.SetFormalCharge(rng.choice((-1, 0, 0, 0, 1)))
            if rng.random() < 0.3:
                atom.SetAtomMapNum(rng.randrange(1, 50))
            donor = mol.AddAtom(atom)
            for metal in rng.sample(metals, rng.randrange(1, len(metals) + 1)):
                a, b = (metal, donor) if rng.random() < 0.5 else (donor, metal)
                mol.AddBond(
                    a,
                    b,
                    rng.choice((Chem.BondType.SINGLE, Chem.BondType.SINGLE, Chem.BondType.DATIVE)),
                )
            for _ in range(rng.randrange(4)):
                atom = Chem.Atom(rng.choice((6, 7, 8)))
                if rng.random() < 0.2:
                    atom.SetIsotope(13)
                other = mol.AddAtom(atom)
                mol.AddBond(donor, other, rng.choice((Chem.BondType.SINGLE, Chem.BondType.DOUBLE)))
        emit(f"multiple donors/metals {sample}", mol, rng.choice((None, "fast", "symmetric")))
    for size in range(3, 16):
        for order in ORDERS:
            mol = Chem.RWMol()
            for _ in range(size):
                atom = Chem.Atom(6)
                atom.SetNoImplicit(True)
                mol.AddAtom(atom)
            for i in range(size):
                mol.AddBond(i, (i + 1) % size, ORDERS[order])
            emit(f"cycle {size}/{order}", mol)
    templates = json.loads(
        (Path(__file__).resolve().parents[1] / "assets/templates.json").read_text()
    )
    texts += [t["smiles"] for t in templates if t.get("smiles")]
    texts += [
        line.split()[0]
        for line in (Path(RDConfig.RDDataDir) / "NCI/first_5K.smi").read_text().splitlines()
    ]
    for text in texts:
        mol = Chem.MolFromSmiles(text, sanitize=False)
        if mol is None:
            raise ValueError(f"Invalid reference syntax: {text}")
        emit(text, mol)
        perm = list(range(mol.GetNumAtoms()))
        rng.shuffle(perm)
        emit(text + " permuted", Chem.RenumberAtoms(mol, perm), "symmetric")


if __name__ == "__main__":
    main()
