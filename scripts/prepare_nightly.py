"""Stamp a nightly version into an ephemeral checkout's package and lockfile."""

import argparse
import re
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def stamp(root: Path, *, identifier: str | None = None, version: str | None = None) -> str:
    manifest_path = root / "Cargo.toml"
    lock_path = root / "Cargo.lock"
    manifest = manifest_path.read_text(encoding="utf-8")
    lock = lock_path.read_text(encoding="utf-8")
    package = tomllib.loads(manifest)["package"]
    previous = package["version"]
    base = re.split(r"[-+]", previous, maxsplit=1)[0]
    if identifier is not None:
        if version is not None or not re.fullmatch(
            r"[1-9][0-9]{7}\.[1-9][0-9]*\.[1-9][0-9]*", identifier
        ):
            raise ValueError("Use a date.run_id.run_attempt nightly identifier")
        version = f"{base}-nightly.{identifier}"
    if version is None or not re.fullmatch(
        rf"{re.escape(base)}-nightly\.[1-9][0-9]{{7}}\.[1-9][0-9]*\.[1-9][0-9]*", version
    ):
        raise ValueError("Nightly version must preserve this checkout's base package version")

    # Limit replacements to the root package, preserving all dependency versions.
    manifest_pattern = rf'(\[package\]\s*\n(?:(?!\[)[^\n]*\n)*?version = "){re.escape(previous)}(")'
    lock_pattern = (
        rf'(\[\[package\]\]\s*\nname = "{re.escape(package["name"])}"\s*\nversion = ")'
        rf'{re.escape(previous)}(")'
    )
    updated_manifest, manifest_count = re.subn(manifest_pattern, rf"\g<1>{version}\2", manifest)
    updated_lock, lock_count = re.subn(lock_pattern, rf"\g<1>{version}\2", lock)
    if manifest_count != 1 or lock_count != 1:
        raise ValueError("Expected one matching root package in Cargo.toml and Cargo.lock")
    # Parse both before writing so a malformed/mismatched lockfile changes neither.
    tomllib.loads(updated_manifest)
    tomllib.loads(updated_lock)
    manifest_path.write_text(updated_manifest, encoding="utf-8", newline="\n")
    lock_path.write_text(updated_lock, encoding="utf-8", newline="\n")
    return version


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--identifier", help="UTC YYYYMMDD.GitHub-run-id.run-attempt")
    mode.add_argument("--version", help="Exact nightly version supplied by the validation job")
    args = parser.parse_args()
    print(stamp(ROOT, identifier=args.identifier, version=args.version))


if __name__ == "__main__":
    main()
