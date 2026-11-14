"""Check that each crate in vendor/ is its crates.io source plus vendor/patches/<crate>.diff.

Run `cargo fetch --locked` first so Cargo's registry holds the upstream sources.
`--write` regenerates the patches after a deliberate change to a vendored crate.
"""

import argparse
import filecmp
import os
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
VENDOR = ROOT / "vendor"
PATCHES = VENDOR / "patches"
# Cargo's unpack marker, and the upstream lockfile: vendored crates omit it,
# because ReShiki's Cargo.lock owns the dependency graph.
IGNORED = (".cargo-ok", "Cargo.lock")


def registry_source(name, version):
    home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
    for source in sorted(home.glob(f"registry/src/*/{name}-{version}")):
        return source
    raise ValueError(f"No {name} {version} in {home}/registry/src; run cargo fetch --locked")


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
    errors = []
    for crate in crates:
        manifest = tomllib.loads((VENDOR / crate / "Cargo.toml").read_text(encoding="utf-8"))
        upstream = registry_source(manifest["package"]["name"], manifest["package"]["version"])
        patch = PATCHES / f"{crate}.diff"
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
