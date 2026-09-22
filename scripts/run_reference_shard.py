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
# Approximate seconds from the Linux x64 Checks run, before native_aromatic
# concurrency. These affect assignment only; new/unmeasured targets cost 20 s.
# Longest-first scheduling avoids putting all expensive oracle suites together.
TARGET_SECONDS = {
    "native_aromatic": 1044,
    "inchi_generator": 231,
    "engine_migration": 230,
    "reaction_smiles": 167,
    "aromatic_display": 107,
    "wedging": 98,
    "native_import": 96,
    "depict_pipeline": 74,
    "kekulize": 66,
    "perception": 65,
    "sanitize": 65,
    "reaction_import": 61,
    "drawn_stereo": 60,
    "smiles_traversal": 58,
}
DEFAULT_SECONDS = 20


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
    loads = [0] * count
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
    manifest["estimated_seconds"] = sum(
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
