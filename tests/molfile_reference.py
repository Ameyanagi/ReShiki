"""Direct pinned MOL writers, independent of the application transport/writer."""

import copy
import json
from typing import TYPE_CHECKING

from rdkit import Chem, RDLogger, rdBase

if TYPE_CHECKING or __package__:
    from .document_preparation_reference import cases, drawing, molecule, prepare
else:
    from document_preparation_reference import cases, drawing, molecule, prepare


def emit(name, doc):
    try:
        mol = molecule(doc)
        source = prepare(doc)
    except (ValueError, RuntimeError, KeyError, OverflowError):
        # Document preparation has its own differential corpus.
        return
    for force_v3000 in (False, True):
        expected, failure = None, None
        try:
            if any(
                b.GetBondType()
                in (Chem.BondType.HYDROGEN, Chem.BondType.QUADRUPLE, Chem.BondType.ONEANDAHALF)
                for b in mol.GetBonds()
            ):
                raise ValueError("These bonds cannot be preserved by molecular export")
            expected = Chem.MolToMolBlock(mol, forceV3000=force_v3000)
        except (ValueError, RuntimeError, KeyError, OverflowError) as error:
            failure = str(error)
        print(
            json.dumps(
                dict(
                    name=name,
                    molecule=source,
                    force_v3000=force_v3000,
                    expected=expected,
                    failure=failure,
                )
            )
        )


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    cases(emit)
    for text in (
        "CC=CC",
        "CC=C(C)C",
        "CC(Cl)=C(F)C",
        "C/C=C/C",
        "C/C=C\\C",
        "C1=CCCCCC1",
        "C1=CCCCCCC1",
        "CC=C=CC",
        "[2H]C([H])=C([H])Cl",
    ):
        mol = Chem.MolFromSmiles(text)
        doc = drawing(mol)
        for i in range(len(doc["bonds"])):
            for display in ("plain", "wavy", "wedge", "hash"):
                for reverse in (False, True):
                    changed = copy.deepcopy(doc)
                    bond = changed["bonds"][i]
                    bond["display"] = display
                    if reverse:
                        bond["a"], bond["b"] = bond["b"], bond["a"]
                        bond["stereo_atoms"].reverse()
                    emit(f"stereo marker/{text}/{i}/{display}/{reverse}", changed)
    for size in (8, 9, 16, 17, 999, 1000):
        emit(
            f"property chunks and count boundary/{size}",
            dict(
                version=15,
                atoms=[
                    dict(
                        id=i + 1,
                        element="C",
                        charge=1,
                        isotope=13,
                        explicit_h=2,
                        no_implicit=True,
                        radical_electrons=1,
                        position=dict(x=0, y=0),
                    )
                    for i in range(size)
                ],
                bonds=[],
            ),
        )
    for x in (
        -1e30,
        -280001,
        -280000,
        -279999,
        -0.125,
        -0.0,
        0.0,
        0.125,
        2799999,
        2800000,
        2800001,
        1e30,
    ):
        emit(
            f"coordinate boundary/{x}",
            dict(
                version=15,
                atoms=[dict(id=1, element="C", position=dict(x=x, y=x))],
                bonds=[],
            ),
        )
    table = Chem.GetPeriodicTable()
    for number in range(119):
        for isotope in (1, 2, 3, 13, 15, 99, 200, 300, 999):
            emit(
                f"isotope/{number}/{isotope}",
                dict(
                    version=15,
                    atoms=[
                        dict(
                            id=1,
                            element=table.GetElementSymbol(number),
                            isotope=isotope,
                            no_implicit=True,
                            position=dict(x=0, y=0),
                        )
                    ],
                    bonds=[],
                ),
            )


if __name__ == "__main__":
    main()
