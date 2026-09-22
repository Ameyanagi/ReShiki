"""Independent standard-InChI results from the original pinned RDKit wrapper."""

import json
from typing import TYPE_CHECKING

from rdkit import Chem, RDLogger, rdBase
from rdkit.Chem import rdinchi

if TYPE_CHECKING or __package__:
    from .inchi_input_reference import corpus
    from .perception_reference import snapshot
else:
    from inchi_input_reference import corpus
    from perception_reference import snapshot


def generation_corpus():
    yield from corpus()
    for text in (
        "CC(=O)C",
        "CC(O)=C",
        "NC(=O)N",
        "N=C(O)N",
        "CC(=O)CC(C)=O",
        "CC(O)=CC(C)=O",
        "[2H]OC([2H])([2H])C",
        "[13CH3][C@@H]([18OH])C(=O)[O-]",
        "[NH4+].[Cl-]",
        "[CH2][CH2]",
        "[O-][N+](=O)c1ccccc1",
    ):
        mol = Chem.MolFromSmiles(text)
        assert mol is not None
        yield f"tautomer-isotope-charge/{text}", mol, "symmetric"
    for size in (1023, 1024):
        mol = Chem.RWMol()
        for _ in range(size):
            atom = Chem.Atom(2)
            atom.SetNoImplicit(True)
            mol.AddAtom(atom)
        mol.UpdatePropertyCache(strict=False)
        yield f"kernel-atom-limit/{size}", mol, "none"


def main():
    RDLogger.DisableLog("rdApp.*")
    assert rdBase.rdkitVersion == "2026.03.6"
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)), flush=True)
    for name, mol, rings in generation_corpus():
        if any(
            a.GetChiralTag()
            in (Chem.ChiralType.CHI_TETRAHEDRAL_CW, Chem.ChiralType.CHI_TETRAHEDRAL_CCW)
            and 3 <= a.GetTotalDegree() <= 4
            and a.GetDegree() < 3
            for a in mol.GetAtoms()
        ):
            continue
        state = snapshot(mol, rings)
        positions = (
            [
                dict(x=p.x, y=p.y, z=p.z)
                for p in (mol.GetConformer().GetAtomPosition(i) for i in range(mol.GetNumAtoms()))
            ]
            if mol.GetNumConformers()
            else None
        )
        try:
            text, status, message, log, auxiliary = rdinchi.MolToInchi(mol)
            expected = dict(
                inchi=text, status=status, message=message, log=log, auxiliary=auxiliary
            )
        except (ValueError, RuntimeError):
            expected = None
        print(
            json.dumps(
                dict(name=name, state=state, positions=positions, expected=expected),
                separators=(",", ":"),
            ),
            flush=True,
        )


if __name__ == "__main__":
    main()
