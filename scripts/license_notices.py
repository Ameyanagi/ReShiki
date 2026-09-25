"""Collect project and dependency notices without changing upstream terms."""

import hashlib
import json
import shutil
from pathlib import Path

PROJECT_FILES = ("LICENSE", "LICENSE-MIT", "LICENSE-APACHE", "NOTICE")
NOTICE_PREFIXES = ("LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT", "UNLICENSE", "AUTHORS")


def notice_files(source):
    """Include nested terms, such as vendored C libraries and Unicode data."""
    for candidate in sorted(source.rglob("*")):
        relative = candidate.relative_to(source)
        if candidate.is_file() and (
            candidate.name.upper().startswith(NOTICE_PREFIXES)
            or any(
                part.upper() in {"LICENSE", "LICENSES", "LICENCES"} for part in relative.parts[:-1]
            )
        ):
            yield candidate, relative


def copy_notices(root, destination, metadata):
    for name in PROJECT_FILES:
        shutil.copy2(root / name, destination / name)
    supplements_root = root / "licenses/rust"
    supplements = json.loads((supplements_root / "manifest.json").read_text(encoding="utf-8"))
    workspace = set(metadata.get("workspace_members", []))
    for package in metadata["packages"]:
        source = Path(package["manifest_path"]).parent
        key = f"{package['name']}@{package['version']}"
        folder = destination / "rust" / f"{package['name']}-{package['version']}"
        folder.mkdir(parents=True, exist_ok=True)
        # Workspace crates use the project terms. Never traverse target/ or
        # vendor/ as if they were all part of an original-code workspace crate.
        files = (
            [(root / name, Path(name)) for name in PROJECT_FILES]
            if package.get("id") in workspace
            else list(notice_files(source))
        )
        for candidate, relative in files:
            target = folder / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(candidate, target)
        supplement = supplements.get(key)
        if supplement:
            if supplement["license"] != package.get("license"):
                raise ValueError(f"License changed for {key}; review its notice supplement")
            for record in supplement["files"]:
                candidate = (supplements_root / record["path"]).resolve()
                if not candidate.is_relative_to(supplements_root.resolve()):
                    raise ValueError(f"Invalid notice path for {key}")
                if hashlib.sha256(candidate.read_bytes()).hexdigest() != record["sha256"]:
                    raise ValueError(f"License notice checksum mismatch for {key}")
                target = folder / "upstream" / candidate.name
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(candidate, target)
            (folder / "upstream-sources.json").write_text(
                json.dumps(supplement, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
            )
        if not files and not supplement:
            raise ValueError(f"Missing license text for {key}; add a reviewed notice supplement")
        # Retain the original declaration and author attribution even when the
        # published archive only names a license instead of including its text.
        shutil.copy2(source / "Cargo.toml", folder / "Cargo.toml")
