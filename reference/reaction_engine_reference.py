"""Complete original RXN import responses, without the prepared transport."""

import json
import sys
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import RDLogger, rdBase

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from engine.worker import handle

if TYPE_CHECKING or __package__:
    from .reaction_drawing_reference import cases
else:
    from reaction_drawing_reference import cases


def emit(name, text):
    try:
        expected = handle(dict(protocol=1, operation="import", format="rxn", text=text))
        failure = None
    except (ValueError, RuntimeError, KeyError, AttributeError, OverflowError) as error:
        expected, failure = None, str(error)
    print(json.dumps(dict(name=name, text=text, expected=expected, failure=failure)))


if __name__ == "__main__":
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    cases(emit)
