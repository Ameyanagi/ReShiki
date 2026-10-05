"""Independent RDKit descriptor oracle, including per-atom contributions."""

import json
import random
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDConfig, RDLogger, rdBase
from rdkit.Chem import rdMolDescriptors as descriptors

if TYPE_CHECKING or __package__:
    from .valence_reference import ORDERS, atom_molecule, graph
else:
    from valence_reference import ORDERS, atom_molecule, graph


def emit(name, mol):
    expanded = Chem.AddHs(mol)
    types = [0] * expanded.GetNumAtoms()
    labels = [""] * expanded.GetNumAtoms()
    contributions = descriptors._CalcCrippenContribs(expanded, True, types, labels)
    logp, mr = descriptors.CalcCrippenDescriptors(mol, True, True)
    result = dict(
        donors=descriptors.CalcNumHBD(mol),
        acceptors=descriptors.CalcNumHBA(mol),
        logp=logp,
        molar_refractivity=mr,
        tpsa=descriptors.CalcTPSA(mol, True, False),
        tpsa_with_s_p=descriptors.CalcTPSA(mol, True, True),
        tpsa_atoms=descriptors._CalcTPSAContribs(mol, True, False),
        tpsa_s_p_atoms=descriptors._CalcTPSAContribs(mol, True, True),
        crippen_atoms=contributions,
        crippen_types=[t if label else None for t, label in zip(types, labels, strict=True)],
    )
    print(
        json.dumps(
            dict(
                name=name,
                graph=graph(mol),
                rings=list(mol.GetRingInfo().AtomRings()),
                expected=result,
            )
        )
    )


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    emit("empty", Chem.Mol())
    # All supported atomic numbers, including unmatched descriptor types.
    for number in range(119):
        mol = Chem.RWMol()
        atom = Chem.Atom(number)
        atom.SetNoImplicit(True)
        mol.AddAtom(atom)
        Chem.SanitizeMol(mol)
        emit(f"element {number}", mol)
    # Exercise query degree/H/valence separately, including special bonds that
    # do not contribute ordinary valence, in both dative directions.
    for number in (0, 1, 5, 6, 7, 8, 15, 16):
        for charge in (-1, 0, 1):
            for h, no_implicit in ((0, False), (0, True), (1, True)):
                for order, kind in ORDERS.items():
                    for reverse in (False, True) if order == 5 else (False,):
                        for leaf_number in (0, 1, 6, 7, 8):
                            for degree in (1, 2, 3):
                                mol = atom_molecule(number, charge, h, 0, no_implicit)
                                for i in range(degree):
                                    leaf = Chem.Atom(leaf_number)
                                    leaf.SetNoImplicit(True)
                                    mol.AddAtom(leaf)
                                    a, b = (i + 1, 0) if reverse else (0, i + 1)
                                    mol.AddBond(a, b, kind)
                                try:
                                    mol.UpdatePropertyCache(strict=True)
                                except (ValueError, RuntimeError):
                                    continue
                                Chem.GetSymmSSSR(mol)
                                emit(
                                    f"star {number}/{charge}/{h}/{no_implicit}/"
                                    f"{order}/{reverse}/{leaf_number}/{degree}",
                                    mol,
                                )
    examples = [
        "C",
        "CC",
        "CCO",
        "O",
        "[2H]O[3H]",
        "[H][H]",
        "*C",
        "[Ne]",
        "[Na+].[Cl-]",
        "[13CH3][NH3+]",
        "[NH4+]",
        "[OH-]",
        "[CH3]",
        "[O][O]",
        "[NH2]",
        "[NH]",
        "CC(=O)O",
        "CC(=O)[O-]",
        "C(=O)O",
        "CC(=O)N",
        "N=C(O)N",
        "NC(=N)N",
        "ON=O",
        "ONO",
        "ON(=O)=O",
        "NO",
        "COO",
        "CSS",
        "C[S+](C)C",
        "C[Si](C)(C)C",
        "N1CC1",
        "O1CC1",
        "CN1CC1",
        "[NH2+]1CC1",
        "N1CCC1",
        "O1CCC1",
        "O=C1NCC1",
        "N1C(=N)CCC1",
        "N1C(=[N+]2CCC2)CCC1",
        "N1C(=NCC1)C",
        "c1cc[nH]c1",
        "c1ncc[nH]1",
        "c1ncncc1",
        "[nH+]1ccccc1",
        "c1ccsc1",
        "c1ccc2occc2c1",
        "O=c1[nH]cccc1",
        "[O-][n+]1ccccc1",
        "[O-][N+](=O)c1ccccc1",
        "N->[Cu+2]<-N",
        "P",
        "CP(C)C",
        "CP(=O)(O)O",
        "CS",
        "CS(C)=O",
        "CS(=O)(=O)C",
        "N[C@@H](C)C(=O)O",
        "F/C=C/F",
        "F:O=C/F",
        "[2H]:C",
        "N#C",
        "[C-]#[O+]",
        "C=[N+]=[N-]",
        "C12C3C4C1C5C2C3C45",
        "C1CCC2(CC1)CCCC2",
    ]
    templates = json.loads(
        (Path(__file__).resolve().parents[1] / "assets/templates.json").read_text()
    )
    examples += [t["smiles"] for t in templates if t.get("smiles")]
    rng = random.Random(2148)
    for smiles in examples:
        mol = Chem.MolFromSmiles(smiles)
        if mol is None:
            raise ValueError(f"Invalid reference example: {smiles}")
        emit(smiles, mol)
        emit(smiles + " graph H", Chem.AddHs(mol))
        order = list(range(mol.GetNumAtoms()))
        rng.shuffle(order)
        emit(smiles + " permuted", Chem.RenumberAtoms(mol, order))
    for line in (Path(RDConfig.RDDataDir) / "NCI/first_5K.smi").read_text().splitlines():
        smiles, name = line.split()
        mol = Chem.MolFromSmiles(smiles)
        if mol is None:  # NCI includes valences rejected by the pinned sanitizer.
            continue
        emit(f"NCI {name}", mol)
        emit(f"NCI {name} graph H", Chem.AddHs(mol))
    # Substructure enumeration has a legacy 1,000-match cap, even when different
    # matches start at the same atom. Preserve its effect on large drawings.
    for smiles, count in [
        ("CC", 600),
        ("CC(C)C", 400),
        ("N", 1200),
        ("CC(=O)N", 1100),
        ("O", 1100),
    ]:
        mol = Chem.MolFromSmiles(".".join([smiles] * count))
        emit(f"match cap {smiles} x {count}", mol)


if __name__ == "__main__":
    main()
