"""Complete original RXN import responses, without the prepared transport."""

import json
import sys
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDLogger, rdBase

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import support.frozen_radicals  # noqa: F401  (before check_supported is imported)

from engine.worker import handle

if TYPE_CHECKING or __package__:
    from .molfile_restrictions_reference import generic_restriction
    from .reaction_drawing_reference import cases
else:
    from molfile_restrictions_reference import generic_restriction
    from reaction_drawing_reference import cases

# ReShiki keeps RDKit's legacy ring symmetrization; child processes inherit it.
Chem.SetUseLegacyRingFinding(True)


def emit(name, text):
    try:
        expected = handle(dict(protocol=1, operation="import", format="rxn", text=text))
        failure = None
    except (ValueError, RuntimeError, KeyError, AttributeError, OverflowError) as error:
        expected, failure = None, str(error)
    restriction = generic_restriction(text, reaction=True, original=expected is not None)
    print(
        json.dumps(
            dict(name=name, text=text, expected=expected, failure=failure, restriction=restriction)
        )
    )


if __name__ == "__main__":
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    cases(emit)
