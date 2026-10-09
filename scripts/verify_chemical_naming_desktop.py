#!/usr/bin/env python3
"""Verify original local-naming desktop saves and their image/hash manifest.

Maintainer evidence tool only: Python is not an application runtime dependency.
This checks saved data/history, not a new GUI replay or general chemical naming.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from collections import Counter
from pathlib import Path
from typing import Any

FIXTURES = Path("tests/fixtures/chemical-naming/local")
IMAGES = Path("docs/images/chemical-naming/local")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def read_document(root: Path, name: str) -> dict[str, Any]:
    document = json.loads((root / FIXTURES / name).read_bytes())
    require(document["version"] == 19, f"{name}: expected native version19")
    for key in ("arrows", "graphics", "groups"):
        require(not document[key], f"{name}: unexpected {key}")
    require(
        document["atom_labels"] == {"carbons": "skeletal", "hydrogens": True, "stereo": False},
        f"{name}: changed atom-label settings",
    )
    return document


def graph(document: dict[str, Any]) -> dict[str, Any]:
    return {key: value for key, value in document.items() if key != "annotations"}


def check_molecule(document: dict[str, Any], lactic: bool) -> str:
    """Independent fixture topology/H/stereo expectations, not SMILES parsing."""
    atoms = document["atoms"]
    bonds = document["bonds"]
    expected_elements = {1: "C", 2: "C", 3: "O"}
    expected_bonds = {(1, 2, 1), (1, 3, 1)}
    if lactic:
        expected_elements |= {4: "C", 5: "O", 6: "O"}
        expected_bonds = {(1, 2, 1), (2, 3, 1), (2, 4, 1), (1, 5, 2), (1, 6, 1)}
    require(
        len(atoms) == len(expected_elements)
        and {atom["id"]: atom["element"] for atom in atoms} == expected_elements,
        "Unexpected molecule atom topology",
    )
    require(
        len(bonds) == len(expected_bonds)
        and {(min(b["a"], b["b"]), max(b["a"], b["b"]), b["order"]) for b in bonds}
        == expected_bonds,
        "Unexpected molecule bond topology/order",
    )
    by_id = {atom["id"]: atom for atom in atoms}
    valence: Counter[int] = Counter()
    for bond in bonds:
        valence[bond["a"]] += bond["order"]
        valence[bond["b"]] += bond["order"]
        require(
            bond["stereo"] is None and not bond["stereo_atoms"],
            "Unexpected bond stereo",
        )
        expected_display = "wedge" if lactic and (bond["a"], bond["b"]) == (2, 4) else "plain"
        require(bond["display"] == expected_display, "Changed stereo projection")
    hydrogens = 0
    for atom in atoms:
        require(
            atom["charge"] == atom["isotope"] == atom["radical_electrons"] == 0,
            "Unexpected charge/isotope/radical",
        )
        if lactic and atom["id"] == 2:
            require(
                atom["stereo"] == {"winding": "ccw", "neighbors": [1, 3, 4]}
                and atom["cip_label"] == "R"
                and atom["explicit_h"] == 1
                and atom["no_implicit"],
                "Specified R-lactic stereo/hydrogen changed",
            )
        else:
            require(atom["stereo"] is None, "Unexpected tetrahedral stereo")
        expected_valence = {"C": 4, "O": 2}[atom["element"]]
        hydrogens += (
            atom["explicit_h"] if atom["no_implicit"] else expected_valence - valence[atom["id"]]
        )
        require(valence[atom["id"]] <= expected_valence, "Overvalent fixture atom")
    require(hydrogens == 6, "Changed molecular hydrogen total")
    require(by_id[3]["element"] == "O", "Missing hydroxy oxygen")
    return "C3H6O3" if lactic else "C2H6O"


def verify(root: Path) -> dict[str, Any]:
    provenance = json.loads((root / FIXTURES / "provenance.json").read_bytes())
    files = provenance["files"]
    require(len(files) == 23, "Expected12 screenshots and11 native saves")
    expected_paths = {str(path.relative_to(root)) for path in (root / IMAGES).glob("*.jpg")}
    expected_paths |= {str(path.relative_to(root)) for path in (root / FIXTURES).glob("*.rsk")}
    require({entry["path"] for entry in files} == expected_paths, "Manifest file set changed")
    for entry in files:
        data = (root / entry["path"]).read_bytes()
        require(len(data) == entry["bytes"], f"Length mismatch: {entry['path']}")
        require(
            hashlib.sha256(data).hexdigest() == entry["sha256"],
            f"Original-byte hash mismatch: {entry['path']}",
        )
        if entry["path"].endswith(".jpg"):
            require(data.startswith(b"\xff\xd8\xff"), f"Not original JPEG: {entry['path']}")

    groups = [
        ["ethanol-empty-before-insert.rsk", "ethanol-insert-undo.rsk"],
        ["ethanol-inserted.rsk", "ethanol-insert-redo.rsk", "ethanol-caption-undo.rsk"],
        ["ethanol-caption.rsk", "ethanol-caption-redo.rsk", "ethanol-caption-fresh-reopen.rsk"],
        ["r-lactic-inserted.rsk", "r-lactic-after-rejected-name.rsk", "r-lactic-fresh-reopen.rsk"],
    ]
    for group in groups:
        original = (root / FIXTURES / group[0]).read_bytes()
        for name in group[1:]:
            require((root / FIXTURES / name).read_bytes() == original, f"History raw delta: {name}")
    documents = {name: read_document(root, name) for group in groups for name in group}
    for name in groups[0]:
        require(not documents[name]["atoms"] and not documents[name]["bonds"], "Undo left graph")
        require(not documents[name]["annotations"], "Empty drawing contains a caption")
    for name in groups[1]:
        check_molecule(documents[name], lactic=False)
        require(not documents[name]["annotations"], "Unexpected pre-caption annotation")
    for name in groups[2]:
        check_molecule(documents[name], lactic=False)
        annotations = documents[name]["annotations"]
        require(
            len(annotations) == 1 and annotations[0]["text"] == "ethan-1-ol",
            "Caption is not exactly one locally generated name",
        )
        require(
            graph(documents[name]) == graph(documents["ethanol-inserted.rsk"]),
            "Caption changed the graph or another native field",
        )
    for name in groups[3]:
        check_molecule(documents[name], lactic=True)
        require(not documents[name]["annotations"], "Unexpected lactic annotation")
    return {
        "status": "PASS",
        "original_JPEG_hashes": 12,
        "native_RSK19_hashes": 11,
        "byte_identical_history_rejection_reopen_groups": groups,
        "caption_delta": "one ethan-1-ol annotation; every other native field unchanged",
        "ethanol": {"atoms": 3, "bonds": 2, "formula": "C2H6O"},
        "R_lactic_acid": {"atoms": 6, "bonds": 5, "formula": "C3H6O3", "stereo": "R"},
        "scope": "Saved data/identity/history and original bytes; not a new GUI replay or naming engine",
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    result = json.dumps(verify(args.root), indent=2) + "\n"
    if args.output:
        args.output.write_text(result, encoding="utf-8")
    print(result, end="")


if __name__ == "__main__":
    main()
