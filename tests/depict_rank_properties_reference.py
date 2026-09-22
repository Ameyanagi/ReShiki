"""Public pinned Compute2DCoords with literal CX properties, no eager getters."""

import json

from rdkit import Chem, RDLogger, rdBase
from rdkit.Chem import rdDepictor


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    for smiles in ("C", "CC", "CCC", "CC(C)C", "C1CCCCC1", "CC1CCCCC1", "F/C=C/F", "[H]CC"):
        for atom in range(min(3, Chem.MolFromSmiles(smiles, sanitize=False).GetNumAtoms())):
            for name in ("_CIPRank", "_chiralAtomRank"):
                for value in (
                    "bad",
                    "",
                    "0",
                    "+1",
                    "-1",
                    "4294967295",
                    "-4294967295",
                    "4294967296",
                    "-4294967296",
                    "01",
                    "1&#32;",
                    "&#32;1",
                    "1&#9;",
                    "1.0",
                    "1e0",
                    "1&#0;2",
                ):
                    text = f"{smiles} |atomProp:{atom}.{name}.{value}|"
                    for templates in (False, True):
                        mol = Chem.MolFromSmiles(text)
                        if mol is None:
                            raise ValueError(f"Corpus has invalid SMILES: {text}")
                        properties = [
                            {
                                key: a.GetProp(key)
                                for key in ("_CIPRank", "_chiralAtomRank")
                                if a.HasProp(key)
                            }
                            for a in mol.GetAtoms()
                        ]
                        try:
                            rdDepictor.Compute2DCoords(mol, useRingTemplates=templates)
                            expected = [
                                list(mol.GetConformer().GetAtomPosition(a))
                                for a in range(mol.GetNumAtoms())
                            ]
                            error = None
                        except (ValueError, RuntimeError, OverflowError) as exc:
                            expected, error = None, str(exc)
                        print(
                            json.dumps(
                                dict(
                                    text=text,
                                    templates=templates,
                                    properties=properties,
                                    expected=expected,
                                    error=error,
                                )
                            )
                        )


if __name__ == "__main__":
    main()
