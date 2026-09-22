"""Default native SMILES chemistry, without layout or ReShiki's worker."""

import json
from typing import TYPE_CHECKING

from rdkit import Chem, RDLogger, rdBase

if TYPE_CHECKING or __package__:
    from .perception_reference import snapshot
    from .smiles_parse_reference import cases
else:
    from perception_reference import snapshot
    from smiles_parse_reference import cases


def all_cases():
    yield from cases()
    for text in (
        "[H]C",
        "[H][C@](F)(Cl)Br",
        "F[C@]([H])(Cl)Br",
        "F[C@](Cl)(Br)[H]",
        "[H]/N=C/F",
        "[H]/C(F)=C(Cl)/[H]",
        "[H]C([H])([H])[H]",
        "[H-]C",
        "[H+]C",
        "[2H]O[3H]",
        "[1H]N",
        "[H:6]C",
        "[H]*",
        "[H]C[H]",
        "[H][C@@]([H])(F)Cl",
        "[H][n+]1ccccc1",
        "[H][Pt@SP1](F)(Cl)Br",
        "[H][C@AL1](F)Cl",
        "C~[H]",
        "[H]~C",
        "[H]~C~[H]",
        "[H]~C~N",
        "C~[H-]",
        "C~[2H]",
        "C~[H+]",
        "[H]~[C@](F)(Cl)Br",
        "C~1CCCCC-1",
        "C-1CCCCC~1",
        "C~1CCCCC1",
        "C1CCCCC~1",
    ):
        yield f"hydrogen/{text}", text


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    params = Chem.SmilesParserParams()
    params.allowCXSMILES = False
    params.parseName = False
    for name, text in all_cases():
        try:
            mol = Chem.MolFromSmiles(text, params)
            if (
                mol is None
                or any(a.HasQuery() for a in mol.GetAtoms())
                or any(b.HasQuery() for b in mol.GetBonds())
            ):
                raise ValueError("Invalid or query SMILES")
            expected = dict(
                # Native stereo cleanup ensures this cache even for empty graphs.
                state=snapshot(mol, "symmetric"),
                dummy_labels=[
                    a.GetProp("dummyLabel") if a.HasProp("dummyLabel") else None
                    for a in mol.GetAtoms()
                ],
            )
            failure = None
        except (ValueError, RuntimeError, OverflowError) as error:
            expected, failure = None, str(error)
        print(json.dumps(dict(name=name, text=text, expected=expected, failure=failure)))


if __name__ == "__main__":
    main()
