#!/usr/bin/env python3
"""Verify retained Ctrl+J native data and evidence without building or opening an app."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import math
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
DONORS = {2, 5, 6, 9, 10, 13}
CARBONS = {3, 4, 7, 8, 11, 12}
NATIVE_ORDER = [5, 2, 13, 10, 9, 6]


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def read_json(path: Path) -> dict:
    return json.loads(path.read_text())


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def bond_fields(bond: dict) -> dict:
    # Fixture IDs are runtime-only and are not part of native persisted bond data.
    return {key: value for key, value in bond.items() if key != "id"}


def graph(before: dict, after: dict) -> None:
    require(len(before["atoms"]) == len(after["atoms"]) == 13, "atom count")
    require(len(before["bonds"]) == 9 and len(after["bonds"]) == 15, "bond count")
    require(
        {key: value for key, value in before.items() if key not in {"atoms", "bonds"}}
        == {key: value for key, value in after.items() if key not in {"atoms", "bonds"}},
        "document fields",
    )
    for original, saved in zip(before["atoms"], after["atoms"]):
        expected = copy.deepcopy(original)
        if expected["id"] in CARBONS:
            require(expected["label_h"] == 0, "original carbon cache")
            expected["label_h"] = 2
        require(expected == saved, f"atom metadata/XYZ at {original['id']}")
        require(all(math.isfinite(value) for value in saved["position"].values()), "finite XYZ")
    atoms = {atom["id"]: atom for atom in after["atoms"]}
    require(atoms[1]["element"] == "Co" and atoms[1]["charge"] == 3, "Co3+")
    require(
        all(atoms[donor]["element"] == "N" and atoms[donor]["label_h"] == 2 for donor in DONORS),
        "six NH2 donors",
    )
    require(
        list(map(bond_fields, before["bonds"])) == list(map(bond_fields, after["bonds"][:9])),
        "original nine covalent bonds",
    )
    contacts = after["bonds"][9:]
    require([bond["a"] for bond in contacts] == NATIVE_ORDER, "actual contact insertion order")
    require(
        {(bond["a"], bond["b"], bond["order"]) for bond in contacts}
        == {(donor, 1, 5) for donor in DONORS},
        "six directed donor-to-Co contacts",
    )
    require(
        all(
            bond["display"] == "plain"
            and not bond.get("projection", False)
            and bond.get("stereo") is None
            for bond in contacts
        ),
        "plain contacts without assigned projection/stereo",
    )


def negative_controls(before: dict, after: dict) -> list[str]:
    mutations = []
    changed = copy.deepcopy(after)
    changed["bonds"][9]["a"], changed["bonds"][9]["b"] = 1, 5
    mutations.append(("reversed donor direction", changed))
    changed = copy.deepcopy(after)
    changed["bonds"][9]["order"] = 1
    mutations.append(("covalent replacement", changed))
    changed = copy.deepcopy(after)
    changed["bonds"].pop()
    mutations.append(("missing sixth contact", changed))
    changed = copy.deepcopy(after)
    changed["atoms"][0]["charge"] = 0
    mutations.append(("lost cobalt charge", changed))
    changed = copy.deepcopy(after)
    changed["atoms"][1]["position"]["x"] += 1
    mutations.append(("moved donor", changed))
    changed = copy.deepcopy(after)
    changed["atoms"][1]["explicit_h"] = 1
    mutations.append(("changed chemical hydrogen field", changed))
    changed = copy.deepcopy(after)
    changed["bonds"][0]["order"] = 2
    mutations.append(("changed original ligand bond", changed))
    changed = copy.deepcopy(after)
    changed["bonds"][9]["display"] = "wedge"
    mutations.append(("unexpected projection paint", changed))
    rejected = []
    for label, changed in mutations:
        try:
            graph(before, changed)
        except ValueError:
            rejected.append(label)
        else:
            raise ValueError(f"negative control accepted: {label}")
    return rejected


def source_hashes(source_root: Path) -> None:
    frozen = read_json(HERE / "revised-frozen-source.json")
    bundle = read_json(HERE / "bundle-provenance.json")
    require(len(frozen["files"]) == frozen["source_entries"] == 1067, "source entry count")
    require(
        len(bundle["default_app_input_hashes"]) == bundle["default_app_input_count"] == 532,
        "default app input count",
    )
    for label, files in [
        ("frozen source", frozen["files"]),
        ("default app inputs", bundle["default_app_input_hashes"]),
    ]:
        for name, expected in files.items():
            require(digest(source_root / name) == expected, f"{label} mismatch: {name}")


def verify(source_root: Path | None) -> dict:
    manifest = read_json(HERE / "manifest.json")
    for entry in manifest["files"]:
        path = ROOT / entry["path"]
        require(
            path.stat().st_size == entry["bytes"] and digest(path) == entry["sha256"],
            f"copied evidence mismatch: {entry['path']}",
        )
    for name, entry in manifest["existing_reference_files"].items():
        require(digest(ROOT / name) == entry["sha256"], f"historical reference changed: {name}")
    before = read_json(ROOT / "tests/fixtures/coordination/co-en3-before.rsk")
    after = read_json(HERE / "co-en3-cmd-j-native.rsk")
    graph(before, after)
    prior = read_json(ROOT / "tests/fixtures/coordination/co-en3-desktop-assembled.rsk")
    require(prior["atoms"] == after["atoms"], "historical assembled atom records")
    require(
        {json.dumps(bond_fields(bond), sort_keys=True) for bond in prior["bonds"]}
        == {json.dumps(bond_fields(bond), sort_keys=True) for bond in after["bonds"]},
        "historical assembled graph, regardless of contact insertion order",
    )
    summary = read_json(HERE / "validation-summary.json")
    for check in summary["checks"] + summary["preserved_failed_or_stopped_attempts"]:
        receipt = read_json(HERE / Path(check["receipt"]).name)
        require(receipt["exit_code"] == check["exit_code"], f"exit receipt: {check['name']}")
        require(
            digest(HERE / Path(receipt["log"]).name)
            == receipt["log_sha256"]
            == check["log_sha256"],
            f"test log: {check['name']}",
        )
    bundle = read_json(HERE / "bundle-provenance.json")
    native = read_json(HERE / "native-verification.json")
    require(native["signed_sha256"] == bundle["signed_executable_sha256"], "native producer")
    require(
        native["native_saved_sha256"] == digest(HERE / "co-en3-cmd-j-native.rsk"),
        "native saved bytes",
    )
    require(
        bundle["source_manifest_sha256"] == digest(HERE / "revised-frozen-source.json"),
        "frozen source manifest bytes",
    )
    artifacts = bundle["compiler_artifacts"]
    require(
        len(artifacts) == 15 and all(not artifact["fresh"] for artifact in artifacts),
        "fresh own-worktree compiler artifacts",
    )
    undo = (HERE / "co-en3-cmd-j-one-contact-undo.ax.txt").read_text()
    reopened = (HERE / "co-en3-cmd-j-fresh-process-reopened.ax.txt").read_text()
    require("button (disabled) Undo" in undo and "button Redo" in undo, "native Undo control")
    require(
        "button (disabled) Undo" in reopened and "button (disabled) Redo" in reopened,
        "fresh-process native history controls",
    )
    controls = negative_controls(before, after)
    if source_root:
        source_hashes(source_root)
    return {
        "status": "PASS",
        "copied_files": len(manifest["files"]),
        "copied_bytes": manifest["copied_bytes"],
        "raw_jpegs": manifest["jpeg_count"],
        "native_graph": "13 atoms, original nine covalent bonds, six plain directed N-to-Co contacts",
        "negative_controls_rejected": controls,
        "source_comparison": str(source_root) if source_root else "not requested",
        "limits": "Does not replay GUI or prove six visible arrows; native Windows CtrlJ not recorded.",
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--source-root", type=Path, help="Compare all 1067 source and 532 app inputs"
    )
    args = parser.parse_args()
    print(json.dumps(verify(args.source_root), indent=2))
