"""Repeat original native calls for known CX-number import platform boundaries."""

import hashlib
import json
import platform
import sys
from pathlib import Path

from rdkit import Chem, RDLogger, rdBase
from rdkit.Chem import rdChemReactions

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from engine.worker import handle


def parsed(case):
    if case["format"] == "smiles":
        molecule = Chem.MolFromSmiles(case["text"])
        if molecule is None:
            return None
        return dict(
            input_coordinates=[
                [v.hex() for v in conformer.GetAtomPosition(atom)]
                for conformer in molecule.GetConformers()
                for atom in range(molecule.GetNumAtoms())
            ]
        )
    reaction = rdChemReactions.ReactionFromRxnBlock(case["text"], sanitize=True, removeHs=False)
    return dict(
        participant_queries=[
            [atom.DescribeQuery() if atom.HasQuery() else None for atom in molecule.GetAtoms()]
            for row in (reaction.GetReactants(), reaction.GetAgents(), reaction.GetProducts())
            for molecule in row
        ]
    )


def main():
    RDLogger.DisableLog("rdApp.*")
    print(
        json.dumps(
            dict(
                system=platform.system(),
                machine=platform.machine(),
                python=platform.python_version(),
                rdkit=rdBase.rdkitVersion,
                boost=rdBase.boostVersion,
            )
        )
    )
    cases = json.loads((ROOT / "tests/fixtures/native-import-platform-cases.json").read_text())
    for case in cases:
        outcomes = []
        for _ in range(10):
            try:
                response = handle(
                    dict(protocol=1, operation="import", format=case["format"], text=case["text"])
                )
                outcome = dict(
                    accepted=True,
                    response_sha256=hashlib.sha256(
                        json.dumps(response, sort_keys=True, allow_nan=False).encode()
                    ).hexdigest(),
                )
            except (ValueError, RuntimeError, KeyError, AttributeError, OverflowError) as error:
                outcome = dict(accepted=False, failure=str(error))
            outcomes.append(outcome)
        if any(outcome != outcomes[0] for outcome in outcomes):
            raise ValueError(f"Nondeterministic original import: {case['name']}")
        print(
            json.dumps(
                dict(
                    name=case["name"],
                    format=case["format"],
                    input_sha256=hashlib.sha256(case["text"].encode()).hexdigest(),
                    repeats=len(outcomes),
                    outcome=outcomes[0],
                    parsed=parsed(case),
                )
            )
        )


if __name__ == "__main__":
    main()
