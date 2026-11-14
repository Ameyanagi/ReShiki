"""Check that each crate in vendor/ is its crates.io source plus vendor/patches/<crate>.diff.

The upstream source comes from Cargo's registry when it is there. Otherwise the
crate file is downloaded from crates.io and checked against the sha256 in
vendor/README.md, because `cargo fetch` skips crates that [patch.crates-io] replaces.
`--write` regenerates the patches after a deliberate change to a vendored crate.
"""

import argparse
import filecmp
import hashlib
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import urllib.request
from pathlib import Path

from download_tls import verified_context

ROOT = Path(__file__).resolve().parents[1]
VENDOR = ROOT / "vendor"
PATCHES = VENDOR / "patches"
# Cargo's unpack marker, and the upstream lockfile: vendored crates omit it,
# because ReShiki's Cargo.lock owns the dependency graph.
IGNORED = (".cargo-ok", "Cargo.lock")


def checksums():
    """The crates.io sha256 of each vendored crate file, from the table in vendor/README.md."""
    readme = (VENDOR / "README.md").read_text(encoding="utf-8")
    return dict(re.findall(r"^\| `([^`]+)` .*`([0-9a-f]{64})`", readme, re.MULTILINE))


def upstream_source(name, version, checksum, temporary):
    home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
    for source in sorted(home.glob(f"registry/src/*/{name}-{version}")):
        return source
    package = f"{name}-{version}"
    for cached in sorted(home.glob(f"registry/cache/*/{package}.crate")):
        data = cached.read_bytes()
        break
    else:
        url = f"https://static.crates.io/crates/{name}/{package}.crate"
        with urllib.request.urlopen(url, timeout=60, context=verified_context()) as response:
            data = response.read()
    if hashlib.sha256(data).hexdigest() != checksum:
        raise ValueError(f"{package}.crate does not match the sha256 in vendor/README.md")
    archive = Path(temporary) / f"{package}.crate"
    archive.write_bytes(data)
    with tarfile.open(archive) as crate:
        crate.extractall(temporary, filter="data")
    return Path(temporary) / package


def copy(source, destination):
    shutil.copytree(source, destination, ignore=shutil.ignore_patterns(*IGNORED))


def tree(directory):
    return sorted(p.relative_to(directory) for p in directory.rglob("*") if p.is_file())


def check(crate, upstream, patch):
    """Apply patch to a copy of upstream and compare every file with vendor/<crate>."""
    with tempfile.TemporaryDirectory() as temporary:
        patched = Path(temporary) / "a"
        copy(upstream, patched)
        if patch.read_bytes():
            subprocess.run(["git", "apply", "-p1", patch], cwd=patched, check=True)
        vendored = VENDOR / crate
        files = tree(vendored)
        if tree(patched) != files:
            return [f"{crate}: the patched upstream has different files from vendor/{crate}"]
        return [
            f"{crate}: {file} differs from the patched upstream"
            for file in files
            if not filecmp.cmp(patched / file, vendored / file, shallow=False)
        ]


def write(crate, upstream, patch):
    with tempfile.TemporaryDirectory() as temporary:
        copy(upstream, Path(temporary) / "a")
        copy(VENDOR / crate, Path(temporary) / "b")
        diff = subprocess.run(
            ["git", "diff", "--no-index", "--no-ext-diff", "--no-prefix", "--binary", "a", "b"],
            cwd=temporary,
            capture_output=True,
        )
        if diff.returncode > 1:
            raise RuntimeError(diff.stderr.decode())
        PATCHES.mkdir(exist_ok=True)
        patch.write_bytes(diff.stdout)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true", help="regenerate vendor/patches")
    args = parser.parse_args()
    crates = sorted(path.name for path in VENDOR.iterdir() if (path / "Cargo.toml").is_file())
    recorded = checksums()
    errors = []
    for crate in crates:
        package = tomllib.loads((VENDOR / crate / "Cargo.toml").read_text(encoding="utf-8"))[
            "package"
        ]
        patch = PATCHES / f"{crate}.diff"
        if package["name"] not in recorded:
            errors.append(f"{crate}: no sha256 in vendor/README.md")
            continue
        with tempfile.TemporaryDirectory() as temporary:
            upstream = upstream_source(
                package["name"], package["version"], recorded[package["name"]], temporary
            )
            if args.write:
                write(crate, upstream, patch)
            elif not patch.is_file():
                errors.append(f"{crate}: no {patch.relative_to(ROOT)}")
            else:
                errors.extend(check(crate, upstream, patch))
    stale = {path.stem for path in PATCHES.glob("*.diff")} - set(crates)
    errors.extend(f"{name}: patch for a crate that is not vendored" for name in sorted(stale))
    for error in errors:
        print(error, file=sys.stderr)
    if not errors:
        print(f"{len(crates)} vendored crates match their upstream source and patch")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
