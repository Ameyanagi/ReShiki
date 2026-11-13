"""Run a complete, deterministic share of Cargo's optional reference tests."""

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
METADATA_COMMAND = ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"]
# Wall seconds per target in the 2026-11-13 live run on Windows x64, the slowest
# platform (Linux and macOS take 35-100% of these). Targets under 20 s and
# new/unmeasured targets cost DEFAULT_SECONDS. These affect assignment only.
# Longest-first scheduling avoids putting all expensive oracle suites together.
TARGET_SECONDS = {
    "abbreviation_detection": 3214,
    "smiles_traversal": 1699,
    "abbreviation_engine": 1532,
    "native_aromatic": 609,
    "engine_migration": 437,
    "inchi_generator": 342,
    "abbreviation_replacement": 200,
    "native_import": 181,
    "reaction_smiles": 165,
    "aromatic_display": 95,
    "reaction_import": 80,
    "wedging": 80,
    "inchi_reader": 75,
    "depict_pipeline": 70,
    "depict_expansion": 67,
    "kekulize": 63,
    "molfile_drawing": 63,
    "native_response": 58,
    "drawing_output": 55,
    "perception": 49,
    "sanitize": 49,
    "composed_native_roundtrip": 47,
    "drawn_stereo": 46,
    "metafile_worker": 46,
    "reaction_output": 45,
    "reaction_drawing": 41,
    "smiles_stereo": 40,
    "molfile_import": 39,
    "smiles_write": 38,
    "bond_geometry": 33,
    "cip_pairing": 31,
    "molfile": 31,
    "cip_rules": 29,
    "ranking": 29,
    "document_preparation": 28,
    "stereo": 27,
    "cip_ranking": 26,
    "stated_ez_wedges": 25,
    "cdxml_scene": 23,
    "cip_molecule": 21,
    "native_cleanup": 21,
    "smiles_read": 21,
    "haworth": 20,
    "native_routing": 20,
}
DEFAULT_SECONDS = 10
# Shard 0 also runs the workspace library, binary and doc tests and every
# WORKSPACE_TARGET_SHARDS target: about 1700 s on Windows x64 in the same run.
SHARD_SECONDS = {0: 1700}
# Non-root integration targets need a package-qualified command. Keep their
# assignment explicit so a new workspace test can never silently disappear.
# Shard 0 exists for every --count, so an assignment can never fall outside it.
WORKSPACE_TARGET_SHARDS = {
    ("reshiki-io", "path_heads"): 0,
    ("reshiki-io", "pdf_gradients"): 0,
    ("reshiki-io", "spacefill_seams"): 0,
    ("reshiki-macos", "native_print"): 0,
    ("reshiki-geometry", "c60"): 0,
    ("reshiki-geometry", "embedding"): 0,
    ("reshiki-agent", "contract"): 0,
    ("reshiki-agent", "headless"): 0,
    ("reshiki-mcp", "panic_hook"): 0,
    ("reshiki-mcp", "transcripts"): 0,
    ("reshiki-process-heap", "global_allocator_unwind"): 0,
}


def integration_targets(metadata: object) -> list[str]:
    """Read every integration target, including Cargo's implicit test targets."""
    if not isinstance(metadata, dict):
        raise ValueError("Cargo metadata must be an object")
    packages = metadata.get("packages")
    members = metadata.get("workspace_members")
    if not isinstance(packages, list) or not isinstance(members, list):
        raise ValueError("Cargo metadata is missing packages or workspace_members")
    matches = [p for p in packages if isinstance(p, dict) and p.get("name") == "reshiki"]
    if len(matches) != 1 or matches[0].get("id") not in members:
        raise ValueError("Cargo metadata must contain one reshiki workspace package")
    unassigned = sorted(
        f"{package.get('name')}::{target.get('name')}"
        for package in packages
        if isinstance(package, dict)
        and package.get("id") in members
        and package.get("name") != "reshiki"
        for target in package.get("targets", [])
        if "test" in target.get("kind", [])
        and (package.get("name"), target.get("name")) not in WORKSPACE_TARGET_SHARDS
    )
    if unassigned:
        raise ValueError(
            "New workspace integration tests need an explicit shard assignment: "
            + ", ".join(unassigned)
        )
    targets = matches[0].get("targets")
    if not isinstance(targets, list):
        raise ValueError("Cargo metadata is missing reshiki targets")
    names = []
    for target in targets:
        if not isinstance(target, dict) or not isinstance(target.get("kind"), list):
            raise ValueError("Invalid target in Cargo metadata")
        if "test" in target["kind"]:
            name = target.get("name")
            if not isinstance(name, str) or not name.strip():
                raise ValueError("Integration target has no name")
            names.append(name)
    if not names or len(names) != len(set(names)):
        raise ValueError("Cargo metadata must contain unique, nonempty integration targets")
    return sorted(names)


def assign_targets(targets: list[str], count: int) -> list[list[str]]:
    """Assign each target once, balancing estimated runtime with stable ties."""
    if count < 1 or count > len(targets):
        raise ValueError("Shard count must be positive and cannot exceed the target count")
    if len(targets) != len(set(targets)) or any(not name.strip() for name in targets):
        raise ValueError("Integration target names must be unique and nonempty")
    shards: list[list[str]] = [[] for _ in range(count)]
    loads = [SHARD_SECONDS.get(index, 0) for index in range(count)]
    for name in sorted(
        targets, key=lambda name: (-TARGET_SECONDS.get(name, DEFAULT_SECONDS), name)
    ):
        index = min(range(count), key=lambda index: (loads[index], index))
        shards[index].append(name)
        loads[index] += TARGET_SECONDS.get(name, DEFAULT_SECONDS)
    return [sorted(shard) for shard in shards]


def commands_for(targets: list[str], index: int) -> list[list[str]]:
    if not targets:
        raise ValueError("Refusing to run an empty integration shard")
    common = ["cargo", "test", "--locked", "--features", "rdkit-reference", "--no-fail-fast"]
    integration = common + ["--package", "reshiki"]
    for name in targets:
        integration.extend(["--test", name])
    commands = [integration]
    if index == 0:
        commands.extend(
            [common + ["--workspace", "--lib", "--bins"], common + ["--workspace", "--doc"]]
        )
    for (package, target), assigned in WORKSPACE_TARGET_SHARDS.items():
        if index == assigned:
            commands.append(["cargo", "test", "--locked", "--package", package, "--test", target])
    return commands


def exit_status(returncode: int) -> int:
    return returncode if returncode >= 0 else 128 - returncode


def run_shard(index: int, count: int, root: Path = ROOT) -> int:
    if count < 1 or not 0 <= index < count:
        raise ValueError("Shard index must satisfy 0 <= index < count, with a positive count")
    destination = root / "artifacts" / f"reference-shard-{index}.json"
    manifest: dict[str, Any] = {
        "index": index,
        "count": count,
        "metadata_command": METADATA_COMMAND,
        "targets": [],
        "commands": [],
    }

    def record() -> None:
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")

    record()
    try:
        metadata = subprocess.run(
            METADATA_COMMAND, cwd=root, capture_output=True, text=True, check=False
        )
        manifest["metadata_returncode"] = metadata.returncode
        if metadata.returncode:
            print(metadata.stderr, file=sys.stderr)
            manifest["error"] = "Cargo metadata failed"
            manifest["returncode"] = exit_status(metadata.returncode)
            record()
            return exit_status(metadata.returncode)
        targets = integration_targets(json.loads(metadata.stdout))
        selected = assign_targets(targets, count)[index]
    except (OSError, ValueError) as error:
        manifest["error"] = str(error)
        manifest["returncode"] = 2
        record()
        print(f"Cannot plan reference shard: {error}", file=sys.stderr)
        return 2
    print(
        f"Reference shard {index + 1}/{count}: {len(selected)} of {len(targets)} targets",
        flush=True,
    )
    print("\n".join(f"  {name}" for name in selected), flush=True)
    manifest["targets"] = selected
    manifest["total_targets"] = len(targets)
    manifest["estimated_seconds"] = SHARD_SECONDS.get(index, 0) + sum(
        TARGET_SECONDS.get(name, DEFAULT_SECONDS) for name in selected
    )
    batches: list[dict[str, Any]] = [
        {"command": command, "status": "pending", "returncode": None}
        for command in commands_for(selected, index)
    ]
    manifest["commands"] = batches
    record()
    status = 0
    for batch in batches:
        print(f"Running {json.dumps(batch['command'])}", flush=True)
        batch["status"] = "running"
        record()
        started = time.monotonic()
        try:
            code = subprocess.run(batch["command"], cwd=root, check=False).returncode
        except OSError as error:
            batch["error"] = str(error)
            print(error, file=sys.stderr)
            code = 127
        batch["returncode"] = code
        batch["seconds"] = round(time.monotonic() - started, 3)
        batch["status"] = "passed" if code == 0 else "failed"
        status = status or exit_status(code)
        record()
    manifest["returncode"] = status
    record()
    return status


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--index", type=int, required=True, help="Zero-based shard index")
    parser.add_argument("--count", type=int, default=4, help="Number of shards (default: 4)")
    args = parser.parse_args()
    try:
        return run_shard(args.index, args.count)
    except (OSError, ValueError) as error:
        parser.exit(2, f"{error}\n")


if __name__ == "__main__":
    sys.exit(main())
