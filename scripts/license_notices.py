"""Consolidate complete project/dependency notices without changing upstream terms."""

import argparse
import hashlib
import json
import shutil
import subprocess
from pathlib import Path

PROJECT_FILES = ("LICENSE", "LICENSE-MIT", "LICENSE-APACHE", "NOTICE")
THIRD_PARTY_FILE = "THIRD-PARTY-NOTICES.txt"
NOTICE_PREFIXES = ("LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT", "UNLICENSE", "AUTHORS")


def dependency_metadata(metadata):
    """Scope workspace metadata to the selected package's normal/build dependencies."""
    resolve = metadata["resolve"]
    root = resolve["root"]
    if root is None:
        raise ValueError("Select a package manifest, not a virtual workspace")
    nodes = {node["id"]: node for node in resolve["nodes"]}
    visited = set()
    pending = [root]
    while pending:
        package = pending.pop()
        if package in visited:
            continue
        visited.add(package)
        pending.extend(
            dependency["pkg"]
            for dependency in nodes[package]["deps"]
            if any(kind["kind"] != "dev" for kind in dependency["dep_kinds"])
        )
    # The selected project's terms are copied separately. Do not scan its build
    # staging directory or Python virtualenv as if they were dependency sources.
    visited.remove(root)
    return {
        **metadata,
        "packages": [package for package in metadata["packages"] if package["id"] in visited],
    }


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


def consolidated_notices(root, metadata):
    """Keep every attribution and complete text; share only byte-identical files."""
    index = [
        "ReShiki third-party notices\n==========================\n\n",
        "Third-party material retains its own terms. This file preserves source\n",
        "attributions, dependency declarations and complete license/notice texts.\n",
        "Each indexed file refers to its SHA-256 text identifier below. Identical\n",
        "texts are stored once; different copyright notices remain distinct.\n",
        "Cargo's inventory records resolved dependencies and their declarations.\n",
        "Original ReShiki workspace crates use the accompanying project licenses.\n\n",
    ]
    texts = {}

    def record(label, content):
        # Reject an unknown encoding instead of silently discarding characters.
        # Preserve the original UTF-8 bytes, including CRLF and trailing newlines.
        content.decode("utf-8")
        if not content.strip():
            raise ValueError(f"Empty license/attribution text: {label}")
        digest = hashlib.sha256(content).hexdigest()
        if digest not in texts:
            texts[digest] = content
        index.append(f"  {label}\n    Text SHA-256: {digest}\n")

    source_root = root / "licenses"
    index.append("SOURCE ATTRIBUTIONS\n-------------------\n")
    for source in sorted(source_root.rglob("*")):
        if source.is_file():
            record(source.relative_to(root).as_posix(), source.read_bytes())
    supplements_root = source_root / "rust"
    supplements = json.loads((supplements_root / "manifest.json").read_text(encoding="utf-8"))
    workspace = set(metadata.get("workspace_members", []))
    for package in sorted(
        metadata["packages"], key=lambda p: (p["name"], p["version"], p.get("id", ""))
    ):
        source = Path(package["manifest_path"]).parent
        key = f"{package['name']}@{package['version']}"
        index.append(f"\nDEPENDENCY: {key}\n")
        for field in ("license", "license_file", "source", "repository", "homepage", "authors"):
            if package.get(field):
                index.append(f"  {field}: {json.dumps(package[field], ensure_ascii=False)}\n")
        # The repository root uses the separately collected project/source terms.
        # Other workspace crates may carry incorporated material even when their
        # manifest still uses the original-code expression. Never skip those texts.
        project_root = source.resolve() == root.resolve()
        files = [] if project_root else list(notice_files(source))
        if (
            package.get("id") in workspace
            and package.get("license") == "MIT OR Apache-2.0"
            and not files
        ):
            index.append(
                "  Original workspace crate: see LICENSE, LICENSE-MIT, LICENSE-APACHE and NOTICE.\n"
            )
            continue
        if package.get("id") in workspace:
            index.append(
                "  Separately licensed workspace component: complete terms follow below.\n"
            )
        if project_root:
            files = list(notice_files(source))
        for candidate, relative in files:
            record(f"{key}/{relative.as_posix()}", candidate.read_bytes())
        supplement = supplements.get(key)
        if supplement:
            if supplement["license"] != package.get("license"):
                raise ValueError(f"License changed for {key}; review its notice supplement")
            for entry in supplement["files"]:
                candidate = (supplements_root / entry["path"]).resolve()
                if not candidate.is_relative_to(supplements_root.resolve()):
                    raise ValueError(f"Invalid notice path for {key}")
                content = candidate.read_bytes()
                if hashlib.sha256(content).hexdigest() != entry["sha256"]:
                    raise ValueError(f"License notice checksum mismatch for {key}")
                record(f"{key}/upstream/{candidate.name}", content)
            record(
                f"{key}/upstream-sources.json",
                (json.dumps(supplement, indent=2, ensure_ascii=False) + "\n").encode(),
            )
        if not files and not supplement:
            raise ValueError(f"Missing license text for {key}; add a reviewed notice supplement")
        # Retain original authors, license declarations, source links, comments
        # and any file-specific terms present in the published Cargo manifest.
        record(f"{key}/Cargo.toml", (source / "Cargo.toml").read_bytes())
    result = bytearray("".join(index).encode())
    result.extend(b"\nCOMPLETE TEXTS\n==============\n")
    for digest, content in sorted(texts.items()):
        result.extend(f"\n----- BEGIN TEXT SHA-256 {digest} -----\n".encode())
        result.extend(content)
        result.extend(f"\n----- END TEXT SHA-256 {digest} -----\n".encode())
    return bytes(result)


def write_notices(root, destination, metadata):
    # Finish validation before replacing a previous aggregate.
    content = consolidated_notices(root, metadata)
    destination.mkdir(parents=True, exist_ok=True)
    for name in PROJECT_FILES:
        shutil.copy2(root / name, destination / name)
    (destination / THIRD_PARTY_FILE).write_bytes(content)
    # These paths belong to older ReShiki packages. Retire only their license
    # payload; never traverse arbitrary application/user-data directories.
    for name in ("sources", "rust", "rust-dependencies.json"):
        old = destination / name
        if old.is_symlink() or old.is_file():
            old.unlink()
        elif old.is_dir():
            shutil.rmtree(old)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest-path", type=Path, default=Path("crates/python/Cargo.toml"))
    parser.add_argument("--destination", type=Path, default=Path("crates/python/wheel-licenses"))
    parser.add_argument("--target", help="Cargo target triple (defaults to the Rust host)")
    args = parser.parse_args()
    target = args.target
    if target is None:
        version = subprocess.check_output(["rustc", "-vV"], text=True)
        target = next(
            line.removeprefix("host: ")
            for line in version.splitlines()
            if line.startswith("host: ")
        )
    command = [
        "cargo",
        "metadata",
        "--format-version",
        "1",
        "--locked",
        "--manifest-path",
        str(args.manifest_path),
        "--filter-platform",
        target,
    ]
    metadata = json.loads(subprocess.check_output(command, encoding="utf-8"))
    scoped = dependency_metadata(metadata)
    write_notices(Path(metadata["workspace_root"]), args.destination, scoped)
    print(
        f"Wrote notices for {len(scoped['packages'])} dependencies ({target}) to {args.destination}"
    )


if __name__ == "__main__":
    main()
