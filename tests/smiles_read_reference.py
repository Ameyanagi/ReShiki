"""Default native SMILES/CX chemistry and conformers, without worker code."""

import json
import math
import os
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDLogger, rdBase

if TYPE_CHECKING or __package__:
    from .cxsmarts_reference import cases as cx_cases
    from .cxsmiles_cases import cases as interaction_cases
    from .perception_reference import snapshot
    from .smiles_prepare_reference import all_cases as bare_cases
    from .valence_reference import ORDERS
else:
    from cxsmarts_reference import cases as cx_cases
    from cxsmiles_cases import cases as interaction_cases
    from perception_reference import snapshot
    from smiles_prepare_reference import all_cases as bare_cases
    from valence_reference import ORDERS


def cases():
    yield from bare_cases()
    for index, text in enumerate(cx_cases()):
        yield f"cx/{index}/{text}", text
    for index, text in enumerate(interaction_cases()):
        yield f"interactions/{index}/{text}", text
    for graph in ("C", "CCO", "[H]C", "F[C@](Cl)(Br)I", "[C:7]", "*", "[13*]"):
        for suffix in (" ethanol", "\tname", " | | named", " |$foo$|名前", "\nname", " \t酸\n"):
            yield f"names/{graph}{suffix!r}", graph + suffix
    if source := os.environ.get("RESHIKI_RDKIT_SOURCE"):
        directory = Path(source) / "Code/GraphMol/SmilesParse/test_data"
        for path in sorted(directory.glob("*.cxsmi")):
            for index, line in enumerate(path.read_text().splitlines()):
                if line:
                    yield f"source-cx/{path.name}/{index}", line


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    for name, text in cases():
        try:
            mol = Chem.MolFromSmiles(text)
            if (
                mol is None
                or any(a.HasQuery() for a in mol.GetAtoms())
                or any(b.HasQuery() for b in mol.GetBonds())
                or any(b.GetBondType() not in ORDERS.values() for b in mol.GetBonds())
            ):
                raise ValueError("Invalid or query SMILES")
            conformers = []
            for conf in mol.GetConformers():
                points = [
                    dict(zip(("x", "y", "z"), conf.GetAtomPosition(a.GetIdx()), strict=True))
                    for a in mol.GetAtoms()
                ]
                # JSON null represents nonfinite coordinates in the Rust serializer.
                points = [
                    {k: v if math.isfinite(v) else None for k, v in point.items()}
                    for point in points
                ]
                conformers.append(dict(positions=points, is_3d=conf.Is3D()))
            expected = dict(
                prepared=dict(
                    state=snapshot(mol, "symmetric"),
                    dummy_labels=[
                        a.GetProp("dummyLabel") if a.HasProp("dummyLabel") else None
                        for a in mol.GetAtoms()
                    ],
                ),
                conformers=conformers,
                name=mol.GetProp("_Name") if mol.HasProp("_Name") else None,
            )
            failure = None
        except (ValueError, RuntimeError, OverflowError) as error:
            expected, failure = None, str(error)
        print(json.dumps(dict(name=name, text=text, expected=expected, failure=failure)))


if __name__ == "__main__":
    main()
