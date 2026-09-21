"""Independent descriptor oracle: expectations come directly from RDKit."""

import json
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, rdBase
from rdkit.Chem import rdMolDescriptors

if TYPE_CHECKING or __package__:
    from .valence_reference import graph
else:
    from valence_reference import graph


def case(name, mol):
    return {
        "name": name,
        "graph": graph(mol),
        "atoms": [
            {
                "atomic_number": a.GetAtomicNum(),
                "isotope": a.GetIsotope(),
                "charge": a.GetFormalCharge(),
                "hydrogens": a.GetTotalNumHs(),
                "radical_electrons": a.GetNumRadicalElectrons(),
            }
            for a in mol.GetAtoms()
        ],
        "expected": {
            "formula": rdMolDescriptors.CalcMolFormula(mol),
            "mass": rdMolDescriptors._CalcMolWt(mol),
            "exact_mass": rdMolDescriptors.CalcExactMolWt(mol),
            "unpaired_electrons": sum(a.GetNumRadicalElectrons() for a in mol.GetAtoms()),
        },
    }


def corpus():
    table = Chem.GetPeriodicTable()
    cases = [case("empty", Chem.Mol())]
    for number in range(119):
        # Exhaust every isotope in RDKit's table, plus unknown isotope fallbacks.
        isotopes = [0, *(i for i in range(1, 401) if table.GetMassForIsotope(number, i)), 999]
        for isotope in isotopes:
            atom = Chem.Atom(number)
            atom.SetNoImplicit(True)
            atom.SetIsotope(isotope)
            molecule = Chem.RWMol()
            molecule.AddAtom(atom)
            molecule.UpdatePropertyCache(strict=False)
            cases.append(case(f"element {number}, isotope {isotope}", molecule))

    smiles = [
        "[H]",
        "[H][H]",
        "[2H]O[3H]",
        "[13CH3][NH3+]",
        "[14CH4]",
        "[999CH4]",
        "[Na+].[Cl-]",
        "[Ca+2].[Cl-].[Cl-]",
        "[NH4+]",
        "[Fe+3]",
        "[O-2]",
        "[CH3]",
        "[CH2]",
        "[O][O]",
        "*C",
        "[13*]C",
        "B",
        "[BH4-]",
        "[SiH4]",
        "[PH4+]",
        "[SeH2]",
        "N[C@@H](C)C(=O)O",
        "F/C=C/F",
        "F/C=C\\F",
        "c1cc[nH]c1",
        "c1ccc2occc2c1",
        "C1CC2CCC1C2",
        "O.O",
        "[Cu+2].O.O.[O-]S(=O)(=O)[O-]",
        "N->[Cu+2]<-N",
        "[He].[Ne].[Ar]",
        "[C-]#[O+]",
        "[O-][N+](=O)c1ccccc1",
        "[2H][C@]([3H])(F)Cl",
        "C" * 250,
        "[NH4+]." * 200 + "[Cl-]." * 199 + "[Cl-]",
    ]
    templates = json.loads(
        (Path(__file__).resolve().parents[1] / "assets/templates.json").read_text()
    )
    smiles.extend(item["smiles"] for item in templates if item.get("smiles"))
    for text in smiles:
        mol = Chem.MolFromSmiles(text)
        if mol is None:
            raise ValueError(f"Invalid oracle SMILES: {text}")
        # Keep explicit graph H, attached H and implicit H distinct. Reversing
        # atom order also catches changes to floating-point accumulation order.
        for name, variant in [(text, mol), (text + " + graph H", Chem.AddHs(mol))]:
            cases.append(case(name, variant))
            cases.append(
                case(
                    name + " reversed",
                    Chem.RenumberAtoms(variant, list(reversed(range(variant.GetNumAtoms())))),
                )
            )
    return {"rdkit_version": rdBase.rdkitVersion, "cases": cases}


if __name__ == "__main__":
    print(json.dumps(corpus(), allow_nan=False))
