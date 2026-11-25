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
# Canonical mapped reaction SMILES fixes all six pairs to their molecular endpoints.
EXPECTED_MAPPED_SMILES = (
    "O[C:5]([CH3:4])=[O:6].[CH3:1][CH2:2][OH:3]>>[CH3:1][CH2:2][O:3][C:5]([CH3:4])=[O:6]",
)


def expected_counts(case, reader):
    if case == "multi_step" and reader == "Indigo":
        return ((1, 1, 0),), "Indigo counts the pathway as one reaction."
    if case == "multi_step":
        return ((1, 0, 0), (1, 1, 0)), "RDKit omits the first step's product."
    if case == "conditions" and reader != "Indigo":
        return ((2, 1, 0),), "RDKit reports zero agents, with or without the below-arrow text."
    return EXPECTED[case], None


def read_rdkit(parse, path):
    reactions = parse(str(path))
    counts = tuple(
        (r.GetNumReactantTemplates(), r.GetNumProductTemplates(), r.GetNumAgentTemplates())
        for r in reactions
    )
    mapped = ()
    if path.stem == "mapped":
        mapped = tuple(rdChemReactions.ReactionToSmiles(r, canonical=True) for r in reactions)
    return counts, mapped


def read_indigo(indigo, path):
    reaction = indigo.loadReactionFromFile(str(path))
    return ((reaction.countReactants(), reaction.countProducts(), reaction.countCatalysts()),), ()


def format_counts(counts):
    return "[" + ", ".join(f"{r}/{p}/{a}" for r, p, a in counts) + "]"


def check(path, reader, parse):
    expected, note = expected_counts(path.stem, reader)
    try:
        if not path.is_file():
            raise FileNotFoundError(path)
        counts, mapped = parse(path)
        passed = counts == expected
        detail = f"counts={format_counts(counts)}"
        if path.stem == "mapped" and reader != "Indigo":
            passed = passed and mapped == EXPECTED_MAPPED_SMILES
            detail += f" mapped_reaction_smiles={list(mapped)}"
            if mapped != EXPECTED_MAPPED_SMILES:
                detail += f" expected_mapped_reaction_smiles={list(EXPECTED_MAPPED_SMILES)}"
        if not passed:
            detail += f" expected={format_counts(expected)}"
    except Exception as error:
        passed = False
        detail = f"counts=unavailable expected={format_counts(expected)} error="
        detail += " ".join(str(error).split())
    print(f"{'PASS' if passed else 'FAIL'} {reader} {path.name} {detail}")
    if passed and note:
        print(f"NOTE {reader} {path.name} {note}")
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
