#!/usr/bin/env python3
"""Check reaction goldens with the optional RDKit and Indigo developer tools.

Run with: uv run --no-project --with rdkit==2026.9.2 --with epam.indigo==1.49.0
python scripts/check_chemdraw_reactions.py crates/io/tests/fixtures/reaction_steps
The reader quirks below were seen with those versions; neither is a project dependency.
Counts are reactants/products/agents (Indigo calls agents catalysts).
"""

import argparse
from functools import partial
from pathlib import Path

from indigo import Indigo  # ty: ignore[unresolved-import]  (uv run --with)
from rdkit.Chem import rdChemDraw, rdChemReactions

EXPECTED = {
    "simple": ((1, 1, 0),),
    "multi_reactant": ((2, 1, 0),),
    "multi_step": ((1, 1, 0), (1, 1, 0)),
    "conditions": ((2, 1, 1),),
    "mapped": ((2, 1, 0),),
}


def expected_counts(case, reader):
    # RDKit 2026.09 loses the shared intermediate's first product role;
    # Indigo 1.49 exposes one collapsed pathway through its count* methods.
    if case == "multi_step":
        return ((1, 1, 0),) if reader == "Indigo" else ((1, 0, 0), (1, 1, 0))
    # RDKit 2026.09 reports no agent here, with or without the below-arrow text.
    if case == "conditions" and reader != "Indigo":
        return ((2, 1, 0),)
    return EXPECTED[case]


def read_rdkit(parse, path):
    reactions = parse(str(path))
    counts = tuple(
        (r.GetNumReactantTemplates(), r.GetNumProductTemplates(), r.GetNumAgentTemplates())
        for r in reactions
    )
    mapped = tuple(
        tuple(
            sum(atom.GetAtomMapNum() > 0 for molecule in side for atom in molecule.GetAtoms())
            for side in (reaction.GetReactants(), reaction.GetProducts())
        )
        for reaction in reactions
    )
    return counts, mapped


def read_indigo(indigo, path):
    reaction = indigo.loadReactionFromFile(str(path))
    return ((reaction.countReactants(), reaction.countProducts(), reaction.countCatalysts()),), ()


def format_counts(counts):
    return "[" + ", ".join(f"{r}/{p}/{a}" for r, p, a in counts) + "]"


def check(path, reader, parse):
    expected = expected_counts(path.stem, reader)
    try:
        if not path.is_file():
            raise FileNotFoundError(path)
        counts, mapped = parse(path)
        passed = counts == expected
        detail = f"counts={format_counts(counts)}"
        if path.stem == "mapped" and reader != "Indigo":
            passed = passed and all(r > 0 and p > 0 for r, p in mapped)
            detail += f" mapped_atoms={list(mapped)}"
        if not passed:
            detail += f" expected={format_counts(expected)}"
    except Exception as error:
        passed = False
        detail = f"counts=unavailable expected={format_counts(expected)} error="
        detail += " ".join(str(error).split())
    print(f"{'PASS' if passed else 'FAIL'} {reader} {path.name} {detail}")
    return passed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "directory", type=Path, help="Directory containing the five reaction goldens"
    )
    args = parser.parse_args()
    indigo = Indigo()
    readers = (
        ("RDKit.CDXML", partial(read_rdkit, rdChemReactions.ReactionsFromCDXMLFile), (".cdxml",)),
        (
            "RDKit.ChemDraw",
            partial(read_rdkit, rdChemDraw.ReactionsFromChemDrawFile),
            (".cdx", ".cdxml"),
        ),
        ("Indigo", partial(read_indigo, indigo), (".cdx", ".cdxml")),
    )
    passed = True
    for case in EXPECTED:
        for extension in (".cdx", ".cdxml"):
            for reader, parse, extensions in readers:
                if extension in extensions:
                    passed = check(args.directory / f"{case}{extension}", reader, parse) and passed
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
