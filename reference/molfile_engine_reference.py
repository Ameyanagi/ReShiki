"""Complete original MOL import responses, without the prepared transport."""

import json
import os
import sys
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDLogger, rdBase

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from engine.worker import handle

if TYPE_CHECKING or __package__:
    from .molfile_display_reference import display_expectation
    from .molfile_generic_reference import generic_drawing_addition, generic_engine_extension
    from .molfile_groups_reference import cases as group_cases
    from .molfile_import_reference import annotation_cases, native_molecules, v3_block
    from .molfile_restrictions_reference import generic_restriction
else:
    from molfile_display_reference import display_expectation
    from molfile_generic_reference import generic_drawing_addition, generic_engine_extension
    from molfile_groups_reference import cases as group_cases
    from molfile_import_reference import annotation_cases, native_molecules, v3_block
    from molfile_restrictions_reference import generic_restriction


def emit(name, text):
    try:
        expected = handle(dict(protocol=1, operation="import", format="mol", text=text))
        failure = None
    except (ValueError, RuntimeError, KeyError, OverflowError) as error:
        expected, failure = None, str(error)
    restriction = generic_restriction(text) if expected is not None else None
    display = generic = extension = None
    if restriction is None:
        if expected is not None:
            display = display_expectation(text, expected)
            document = display["document"] if display else expected["document"]
            generic = generic_drawing_addition(text, document)
        else:
            extension = generic_engine_extension(text, failure)
    print(
        json.dumps(
            dict(
                name=name,
                text=text,
                expected=expected,
                failure=failure,
                restriction=restriction,
                display=display,
                generic=generic,
                generic_extension=extension,
            )
        )
    )


if __name__ == "__main__":
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    annotation_cases(emit)
    for name, mol in native_molecules():
        for v3000 in (False, True):
            emit(f"{name}/{v3000}", Chem.MolToMolBlock(mol, forceV3000=v3000))
    for i, (name, text) in enumerate(group_cases()):
        if i < 100 or i % 101 == 0:
            emit(f"substance group/{name}", text)
    for label in ("R", "R1", "R#", "Pol", "Mod", "R123", "Q"):
        emit(f"dummy/{label}", v3_block([f"1 {label} 0 0 0 0"]))
    for text in ("", " ", "invalid", "\0", "\n\n\n  1  0\n"):
        emit(f"invalid/{text!r}", text)
    if source := os.environ.get("RESHIKI_RDKIT_SOURCE"):
        for path in sorted((Path(source) / "Code/GraphMol/FileParsers/test_data").glob("*.mol")):
            emit(f"source/{path.name}", path.read_text(errors="replace"))
