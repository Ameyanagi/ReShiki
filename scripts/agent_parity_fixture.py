"""Rewrite tests/fixtures/agent-api/parity.json from a built `reshiki`.

The fixture holds one molecule and one reaction with the answers every
surface must give for them: `reshiki --cli` (tests/cli.rs), the Python package
(crates/python/tests/test_parity.py) and the JavaScript package
(packages/headless/test/parity.test.ts). It keeps only values that are the same
on every platform: SMILES, InChI and rounded properties, not coordinates.

    uv run --locked python scripts/agent_parity_fixture.py target/debug/reshiki
"""

import json
import subprocess
import sys
from pathlib import Path

FIXTURE = Path(__file__).resolve().parent.parent / "tests/fixtures/agent-api/parity.json"
MOLECULE = "CC(=O)Oc1ccccc1C(=O)O"
REACTION = "CCO>>CC=O"
PROPERTIES = (
    "formula",
    "mass",
    "exact_mass",
    "logp",
    "tpsa",
    "donors",
    "acceptors",
    "rings",
    "inchikey",
)


def cli(binary: str, args: list[str], stdin: str | None = None) -> str:
    done = subprocess.run(
        [binary, "--cli", *args],
        input=None if stdin is None else stdin.encode(),
        capture_output=True,
        check=True,
    )
    return done.stdout.decode()


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("Usage: agent_parity_fixture.py RESHIKI")
    binary = sys.argv[1]
    analysis = json.loads(cli(binary, ["analyze", "--smiles", MOLECULE]))["value"]["analysis"]
    fixture = {
        "molecule": {
            "smiles": MOLECULE,
            "outputs": {
                format: cli(binary, ["convert", "--smiles", MOLECULE, "--to", format])
                for format in ("smiles", "inchi")
            },
            "properties": {name: analysis[name] for name in PROPERTIES},
        },
        "reaction": {
            "rsmi": REACTION,
            "outputs": {
                "rsmi": cli(binary, ["convert", "-", "--from", "rsmi", "--to", "rsmi"], REACTION)
            },
        },
    }
    FIXTURE.write_text(json.dumps(fixture, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
