"""Build/prove the Rust geometry crate from the locked Cargo source graph."""

import argparse
import json
import os
import platform
import subprocess
import sys
from pathlib import Path

from build_inchi_helper import SUPPORTED_TARGETS
from geometry_source import ROOT, digest, verify


def build(output, target=None, jobs=2, tests=False, offline=False):
    provenance = verify()
    output = Path(output).resolve()
    cargo = os.environ.get("CARGO", "cargo")
    rustc = os.environ.get("RUSTC", "rustc")
    compiler = subprocess.run([rustc, "-vV"], check=True, capture_output=True, text=True).stdout
    host = next(
        line.removeprefix("host: ") for line in compiler.splitlines() if line.startswith("host: ")
    )
    target = target or host
    if target not in SUPPORTED_TARGETS:
        raise ValueError(f"Unsupported geometry target: {target}")
    if tests and target != host:
        raise ValueError("Run geometry tests on their matching native Rust host")
    command = [
        cargo,
        "build",
        "--locked",
        "--release",
        "--message-format=json-render-diagnostics",
        "-p",
        "reshiki-geometry",
        "--target",
        target,
        "--target-dir",
        str(output),
        "--jobs",
        str(jobs),
    ]
    if offline:
        command.append("--offline")
    completed = subprocess.run(command, cwd=ROOT, stdout=subprocess.PIPE, text=True)
    candidates = []
    for line in completed.stdout.splitlines():
        message = json.loads(line)
        if message.get("reason") == "compiler-message":
            rendered = message.get("message", {}).get("rendered")
            if rendered:
                print(rendered, end="", file=sys.stderr)
        if (
            message.get("reason") == "compiler-artifact"
            and message.get("target", {}).get("name") == "reshiki_geometry"
        ):
            candidates.extend(Path(name) for name in message["filenames"] if name.endswith(".rlib"))
    completed.check_returncode()
    if tests:
        test_command = command.copy()
        test_command[1] = "test"
        test_command.remove("--message-format=json-render-diagnostics")
        test_command.extend(["--features", "cosmolkit-core/op-contracts-strict"])
        subprocess.run(test_command, cwd=ROOT, check=True)
    if len(candidates) != 1:
        raise ValueError("Rust geometry build did not produce exactly one crate archive")
    archive = candidates[0]
    metadata_command = [cargo, "metadata", "--locked", "--offline", "--format-version", "1"]
    resolved = json.loads(
        subprocess.run(
            metadata_command, cwd=ROOT, check=True, capture_output=True, text=True
        ).stdout
    )
    verify(cargo_metadata=resolved)
    result = {
        **provenance,
        "target": target,
        "host": platform.platform(),
        "compiler": compiler,
        "rust_archive": str(archive),
        "rust_archive_sha256": digest(archive),
        "dependency_fetch": "disabled" if offline else "Cargo locked sources",
        "native_tests": tests,
        "test_features": ["cosmolkit-core/op-contracts-strict"] if tests else [],
    }
    (output / "build.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--target", choices=SUPPORTED_TARGETS)
    parser.add_argument("--jobs", type=int, default=2, choices=range(1, 5))
    parser.add_argument("--tests", action="store_true")
    parser.add_argument(
        "--offline",
        action="store_true",
        help="Require all locked dependencies to be present in Cargo's cache",
    )
    args = parser.parse_args()
    print(
        json.dumps(build(args.output, args.target, args.jobs, args.tests, args.offline), indent=2)
    )


if __name__ == "__main__":
    main()
