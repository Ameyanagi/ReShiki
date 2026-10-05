"""Original SMILES import responses, including layout and full CIP labels."""

import json
import sys
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import RDLogger, rdBase

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from engine.worker import handle

if TYPE_CHECKING or __package__:
    from .smiles_read_reference import cases
else:
    from smiles_read_reference import cases


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    for index, (name, text) in enumerate(cases()):
        # Complete parser/state comparisons have their own unsampled suite.
        # Here keep all templates, CX interactions, import names and optional
        # source fixtures, plus a deterministic sample of the larger corpus.
        if not name.startswith(("template/", "interactions/", "names/", "source", "hydrogen/")):
            if index % 31:
                continue
        try:
            expected = handle(dict(protocol=1, operation="import", format="smiles", text=text))
            failure = None
        except (ValueError, RuntimeError, KeyError, OverflowError) as error:
            expected, failure = None, str(error)
        print(json.dumps(dict(name=name, text=text, expected=expected, failure=failure)))


if __name__ == "__main__":
    main()
