"""Build and check the complete offline source archive shipped with xTB releases.

Run with ``uv run --no-project python scripts/xtb_source_archive.py build --output DIR``
or ``uv run --no-project python scripts/xtb_source_archive.py check ARCHIVE``.
"""

import argparse
import contextlib
import gzip
import hashlib
import io
import json
import os
import re
import shlex
import shutil
import subprocess
import tarfile
import tempfile
import time
import tomllib
from pathlib import Path

from build_release import checksum, version
from check_agent_dependencies import (
    AXTB_PACKAGES,
    AXTB_SOURCE,
    axtb_locked_source,
    axtb_revision,
)
from prepare_nightly import stamp

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = "SOURCE-MANIFEST.json"
FIXED_MTIME = 0
AXTB_CRATES = {
    "axtb": "axtb",
    "axtb-base": "base",
    "axtb-d4": "d4",
    "axtb-driver": "driver",
    "axtb-gfnff": "gfnff",
    "axtb-gxtb": "gxtb",
    "axtb-solvation": "solvation",
}
EXCLUDED_MEDIA = ("docs/images/", "website/public/media/")
DIST_ENV = {
    "macOS (Apple Silicon and Intel)": {
        "CARGO_PROFILE_DIST_LTO": "thin",
        "CARGO_PROFILE_DIST_CODEGEN_UNITS": "1",
        "CARGO_PROFILE_DIST_STRIP": "symbols",
        "CARGO_PROFILE_DIST_SPLIT_DEBUGINFO": "packed",
        "MACOSX_DEPLOYMENT_TARGET": "14.0",
    },
    "Linux arm64": {
        "CARGO_PROFILE_DIST_LTO": "false",
        "CARGO_PROFILE_DIST_CODEGEN_UNITS": "16",
        "CARGO_PROFILE_DIST_STRIP": "none",
        "CARGO_PROFILE_DIST_SPLIT_DEBUGINFO": "off",
    },
    "Linux x64": {
        "CARGO_PROFILE_DIST_LTO": "fat",
        "CARGO_PROFILE_DIST_CODEGEN_UNITS": "1",
        "CARGO_PROFILE_DIST_STRIP": "none",
        "CARGO_PROFILE_DIST_SPLIT_DEBUGINFO": "off",
    },
}


def run(command, **kwargs):
    return subprocess.run([str(part) for part in command], check=True, **kwargs)


def output(command, **kwargs):
    return run(command, stdout=subprocess.PIPE, text=True, **kwargs).stdout.strip()


def patch_arguments(source):
    """Validate the fork crates and return the exact offline rebuild overrides."""
    if AXTB_CRATES.keys() != AXTB_PACKAGES:
        raise ValueError("The source patches must cover exactly the audited axtb crates")
    arguments = []
    for name, directory in AXTB_CRATES.items():
        manifest = source / "crates" / directory / "Cargo.toml"
        if not manifest.is_file():
            raise ValueError(f"Missing axtb crate manifest: {manifest}")
        package = tomllib.loads(manifest.read_text(encoding="utf-8"))["package"]
        if package["name"] != name:
            raise ValueError(f"Expected crate {name} in {manifest}, found {package['name']}")
        arguments.extend(
            [
                "--config",
                f'patch."{AXTB_SOURCE}".{name}.path="axtb-source/crates/{directory}"',
            ]
        )
    return arguments


def modified_build_command(source, *, profile=None):
    command = ["cargo", "build", "--offline", "--features", "xtb", "--bin", "reshiki"]
    if profile:
        command.extend(["--profile", profile])
    return command + patch_arguments(source)


def exclude_axtb_source(root):
    """Keep path patches in the fork's workspace when it sits inside this archive."""
    manifest = root / "Cargo.toml"
    content = manifest.read_text(encoding="utf-8")
    workspace = tomllib.loads(content)["workspace"]
    if "axtb-source" in workspace.get("exclude", []):
        return
    if "exclude" in workspace:
        raise ValueError('Add "axtb-source" to the ReShiki workspace exclusions')
    updated, count = re.subn(
        r"^\[workspace\]$", '[workspace]\nexclude = ["axtb-source"]', content, flags=re.M
    )
    if count != 1 or tomllib.loads(updated)["workspace"]["exclude"] != ["axtb-source"]:
        raise ValueError("Could not exclude the fork from ReShiki's workspace")
    manifest.write_text(updated, encoding="utf-8", newline="\n")


def merge_vendor_config(path, vendor_config):
    """Append Cargo's source replacements while retaining the repository config."""
    existing = path.read_text(encoding="utf-8") if path.exists() else ""
    merged = existing.rstrip() + "\n\n" + vendor_config.strip() + "\n"
    tomllib.loads(merged)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(merged, encoding="utf-8", newline="\n")


def file_hashes(root):
    """Hash every file except the manifest itself; hash link targets for symlinks."""
    hashes = {}
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root).as_posix()
        if relative == MANIFEST:
            continue
        if path.is_symlink():
            digest = hashlib.sha256(os.fsencode(os.readlink(path))).hexdigest()
        elif path.is_file():
            with path.open("rb") as stream:
                digest = hashlib.file_digest(stream, "sha256").hexdigest()
        else:
            continue
        hashes[relative] = digest
    return hashes


def write_manifest(root, details):
    manifest = {**details, "files": file_hashes(root)}
    (root / MANIFEST).write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8", newline="\n"
    )
    return manifest


def verify_manifest(root):
    manifest = json.loads((root / MANIFEST).read_text(encoding="utf-8"))
    actual = file_hashes(root)
    expected = manifest["files"]
    if actual != expected:
        changed = sorted(
            name
            for name in actual.keys() | expected.keys()
            if actual.get(name) != expected.get(name)
        )
        raise ValueError("Source manifest SHA-256 mismatch: " + ", ".join(changed))
    return manifest


def write_archive(root: Path, archive: Path):
    """Write ordered tar entries and deterministic tar and gzip metadata."""
    archive.parent.mkdir(parents=True, exist_ok=True)
    with archive.open("wb") as destination:
        with gzip.GzipFile(
            filename="", fileobj=destination, mode="wb", mtime=FIXED_MTIME, compresslevel=6
        ) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT) as stream:
                for path in [root, *sorted(root.rglob("*"), key=lambda path: path.as_posix())]:
                    entry = stream.gettarinfo(
                        path, arcname=path.relative_to(root.parent).as_posix()
                    )
                    entry.mtime = FIXED_MTIME
                    entry.uid = entry.gid = 0
                    entry.uname = entry.gname = ""
                    entry.pax_headers = {}
                    if entry.isdir():
                        entry.mode = 0o755
                    elif entry.isfile():
                        entry.mode = 0o755 if entry.mode & 0o111 else 0o644
                    if entry.isfile():
                        with path.open("rb") as contents:
                            stream.addfile(entry, contents)
                    else:
                        stream.addfile(entry)
    checksum(archive)


def git_archive(repository, revision, destination):
    """Stage only files tracked at a revision, never checkout-local cache files."""
    destination.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryFile() as archive:
        run(["git", "archive", "--format=tar", revision], cwd=repository, stdout=archive)
        archive.seek(0)
        with tarfile.open(fileobj=archive) as stream:
            stream.extractall(destination, filter="data")


def evidence_media(repository):
    """Keep media read by the standalone evidence verifiers under tests/fixtures."""
    fixtures = repository / "tests/fixtures"
    coordination = json.loads(
        (fixtures / "coordination/join-shortcut/manifest.json").read_text(encoding="utf-8")
    )
    nmr = fixtures / "nmr/mock-alignment/evidence"
    nmr_index = json.loads((nmr / "index.json").read_text(encoding="utf-8"))
    paths = (
        set(coordination["existing_reference_files"])
        | {entry["path"] for entry in coordination["files"]}
        | nmr_index["files"].keys()
        | nmr_index["authored_sha256"].keys()
        | json.loads((nmr / "build/source-manifest.json").read_text(encoding="utf-8")).keys()
        | json.loads(
            (fixtures / "retained-plus-lactic-acid/sha256.json").read_text(encoding="utf-8")
        ).keys()
    )
    return {path for path in paths if path.startswith(EXCLUDED_MEDIA)}


def copy_tracked_source(repository, destination, retained_media):
    """Stage tracked working-tree files, including the release job's nightly stamp."""
    tracked = run(["git", "ls-files", "-z"], cwd=repository, stdout=subprocess.PIPE).stdout
    kept_media = []
    for name in os.fsdecode(tracked).split("\0"):
        if not name:
            continue
        if name.startswith(EXCLUDED_MEDIA):
            if name not in retained_media:
                continue
            kept_media.append(name)
        staged = destination / name
        staged.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(repository / name, staged, follow_symlinks=False)
    return kept_media


def fork_checkout(root):
    metadata = json.loads(
        output(
            ["cargo", "metadata", "--locked", "--format-version", "1", "--features", "xtb"],
            cwd=root,
        )
    )
    revision = axtb_revision(root)
    expected = axtb_locked_source(revision)
    packages = [package for package in metadata["packages"] if package["name"] in AXTB_PACKAGES]
    if {package["name"] for package in packages} != AXTB_PACKAGES or any(
        package.get("source") != expected for package in packages
    ):
        raise ValueError(
            "Cargo metadata must contain all seven axtb crates at the audited revision"
        )
    locked = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))["package"]
    driver = [package for package in locked if package["name"] == "axtb-driver"]
    if len(driver) != 1 or driver[0].get("source") != expected:
        raise ValueError("Cargo.lock axtb-driver revision differs from the audited revision")
    package = next(package for package in packages if package["name"] == "axtb-driver")
    checkout = Path(package["manifest_path"]).parents[2]
    if Path(package["manifest_path"]) != checkout / "crates/driver/Cargo.toml":
        raise ValueError("Unexpected axtb-driver manifest location")
    if output(["git", "rev-parse", "HEAD"], cwd=checkout) != revision:
        raise ValueError("Cargo's axtb checkout is not at the pinned revision")
    patch_arguments(checkout)
    return checkout, revision, package["version"]


def build_instructions(root):
    toolchain = tomllib.loads((root / "rust-toolchain.toml").read_text(encoding="utf-8"))[
        "toolchain"
    ]["channel"]
    development = (root / "docs/development.md").read_text(encoding="utf-8")
    linux_rows = [
        line
        for line in development.splitlines()
        if line.startswith(("| C/C++ compiler", "| System libraries"))
    ]
    if len(linux_rows) != 2:
        raise ValueError("Cannot find the Linux compiler and system-library prerequisites")
    linux_packages = " ".join(re.findall(r"`([^`]+)`", row.split("|")[-2])[0] for row in linux_rows)
    arguments = modified_build_command(root / "axtb-source", profile="dist")
    first_patch = arguments.index("--config")
    lines = [shlex.join(arguments[:first_patch])]
    for index in range(first_patch, len(arguments), 2):
        lines.append(shlex.join(arguments[index : index + 2]))
    command = (" \\" + "\n  ").join(lines)
    settings = []
    for platform, environment in DIST_ENV.items():
        exports = "\n".join(
            f"export {name}={shlex.quote(value)}" for name, value in environment.items()
        )
        settings.append(f"**{platform}**\n\n```sh\n{exports}\n```")
    dist_environment = "\n\n".join(settings)
    exclusions = " and ".join(f"`{directory}**`" for directory in EXCLUDED_MEDIA)
    return f"""# Rebuilding ReShiki with xTB

This archive contains ReShiki, the pinned axtb fork, and all locked Cargo
sources. Cargo's source replacements in `.cargo/config.toml` let both builds
use `vendored-crates/` in a fresh, offline Cargo environment. Install the
prerequisites below and run the commands from this directory.

## Prerequisites

- Rust {toolchain}, as pinned by `rust-toolchain.toml` (install with rustup).
- On Linux (Ubuntu or Debian), install the packages listed in
  `docs/development.md`.

  ```sh
  sudo apt-get install {linux_packages}
  ```

- On macOS, run `xcode-select --install` to install Xcode Command Line Tools.

## Release profile settings

Run the exports for your platform in the same shell as either rebuild command.
These settings match the desktop release workflow.

{dist_environment}

## Rebuild as shipped

```sh
cargo build --profile dist --locked --offline --features xtb --bin reshiki
```

The executable is `target/dist/reshiki`. Check its application and engine
versions with `target/dist/reshiki --version`.

## Rebuild with a modified axtb

Edit the files in `axtb-source/`. This command patches all seven fork crates
to those local sources. The staged ReShiki manifest excludes `axtb-source/`
from the ReShiki workspace so the fork crates inherit their own workspace
settings. Cargo updates `Cargo.lock` for the path patches while offline.

```sh
{command}
```

Check the result with `target/dist/reshiki --version` (or
`target/debug/reshiki --version` if you omit `--profile dist`).

## macOS signing and app bundle

Sign a rebuilt executable ad hoc before running it.

```sh
codesign --force --sign - target/dist/reshiki
```

To wrap the rebuilt executable as `ReShiki.app`, use Python 3.12 and the
bundled metadata helper, copy the license texts, then sign the app ad hoc.

```sh
mkdir -p dist/ReShiki.app/Contents/MacOS
cp target/dist/reshiki dist/ReShiki.app/Contents/MacOS/reshiki
python3 -c 'import sys; from pathlib import Path; sys.path.insert(0, "scripts"); from build_release import bundle_metadata; bundle_metadata(Path("dist/ReShiki.app"))'
cp -R licenses dist/ReShiki.app/Contents/Resources/Licenses
cp LICENSE LICENSE-MIT LICENSE-APACHE NOTICE dist/ReShiki.app/Contents/Resources/Licenses/
codesign --force --deep --sign - dist/ReShiki.app
open dist/ReShiki.app
```

ReShiki's license texts are in `LICENSE` and `licenses/`. Look for axtb's license
and notice texts in `licenses/axtb/` and `axtb-source/` (including each fork
crate's `COPYING`, `COPYING.LESSER` and `NOTICE`). `SOURCE-MANIFEST.json`
records SHA-256 hashes of all files other than itself (symlinks hash their
target text), the commits, and the Cargo and rustc versions used to package
this archive.

The archive excludes promotional media and screenshot evidence under
{exclusions}. It retains tracked inputs read by the standalone NMR,
coordination shortcut, and retained lactic acid evidence verifiers. The
manifest records the exclusion patterns and the retained media paths.
"""


def build(destination, package_version=None):
    package_version = package_version or version()
    if not re.fullmatch(r"[0-9A-Za-z][0-9A-Za-z.+-]*", package_version):
        raise ValueError("Expected a package version without path separators")
    commit = output(["git", "rev-parse", "HEAD"], cwd=ROOT)
    with tempfile.TemporaryDirectory(prefix="ReShiki source archive ") as temporary:
        root = Path(temporary) / f"reshiki-{package_version}-source"
        retained_media = copy_tracked_source(ROOT, root, evidence_media(ROOT))
        staged_version = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))[
            "package"
        ]["version"]
        if staged_version != package_version:
            stamp(root, version=package_version)
        exclude_axtb_source(root)
        checkout, revision, engine_version = fork_checkout(root)
        git_archive(checkout, revision, root / "axtb-source")
        patch_arguments(root / "axtb-source")
        vendor_config = output(
            ["cargo", "vendor", "--locked", "--versioned-dirs", "vendored-crates"], cwd=root
        )
        merge_vendor_config(root / ".cargo/config.toml", vendor_config)
        (root / "BUILD-WITH-XTB.md").write_text(
            build_instructions(root), encoding="utf-8", newline="\n"
        )
        write_manifest(
            root,
            {
                "format": 1,
                "reshiki": {"commit": commit, "version": package_version},
                "axtb": {"source": AXTB_SOURCE, "rev": revision, "version": engine_version},
                "exclusions": [f"{directory}**" for directory in EXCLUDED_MEDIA],
                "retained_media": retained_media,
                "tools": {
                    "cargo": output(["cargo", "--version"], cwd=root),
                    "rustc": output(["rustc", "--version"], cwd=root),
                },
            },
        )
        archive = destination.resolve() / f"{root.name}.tar.gz"
        write_archive(root, archive)
    print(f"Built {archive} ({archive.stat().st_size:,} bytes)")
    return archive


def verify_checksum(archive):
    expected = Path(str(archive) + ".sha256").read_text(encoding="ascii")
    with archive.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    if expected != f"{digest}  {archive.name}\n":
        raise ValueError(f"Archive SHA-256 mismatch: {archive}")


def modify_driver_version(source, original):
    path = source / "crates/driver/src/response.rs"
    text = path.read_text(encoding="utf-8")
    modified = original + "+modified"
    text, count = re.subn(
        r'^pub const VERSION: &str = (?:env!\("CARGO_PKG_VERSION"\)|"[^"\n]*");$',
        f"pub const VERSION: &str = {json.dumps(modified)};",
        text,
        flags=re.M,
    )
    if count != 1:
        raise ValueError(f"Expected one driver VERSION constant in {path}")
    path.write_text(text, encoding="utf-8", newline="\n")
    return modified


def check(archive):
    from build_release import verify_xtb_worker

    started = time.monotonic()
    archive = archive.resolve()
    verify_checksum(archive)
    with tempfile.TemporaryDirectory(prefix="ReShiki source check ") as temporary:
        extracted = Path(temporary)
        with tarfile.open(archive) as stream:
            stream.extractall(extracted, filter="data")
        roots = list(extracted.iterdir())
        if len(roots) != 1 or not roots[0].is_dir():
            raise ValueError("Expected one source directory in the archive")
        root = roots[0]
        manifest = verify_manifest(root)
        source = root / "axtb-source"
        cargo_home = extracted / "empty-cargo-home"
        cargo_home.mkdir()
        environment = {
            **os.environ,
            "CARGO_HOME": str(cargo_home),
            "CARGO_NET_OFFLINE": "true",
            "CARGO_TARGET_DIR": str(root / "target"),
        }
        pristine_started = time.monotonic()
        run(
            ["cargo", "check", "--locked", "--offline", "--features", "xtb", "--bin", "reshiki"],
            cwd=root,
            env=environment,
        )
        print(
            f"Pristine offline cargo check passed in {time.monotonic() - pristine_started:.1f}s.",
            flush=True,
        )
        modified = modify_driver_version(source, manifest["axtb"]["version"])
        build_started = time.monotonic()
        run(modified_build_command(source), cwd=root, env=environment)
        print(f"Modified axtb build passed in {time.monotonic() - build_started:.1f}s.", flush=True)
        binary = root / "target/debug" / ("reshiki.exe" if os.name == "nt" else "reshiki")
        metadata = {"xtb": {"included": True, "rev": manifest["axtb"]["rev"], "version": modified}}
        summary = io.StringIO()
        with contextlib.redirect_stdout(summary):
            verify_xtb_worker(binary, metadata)
    elapsed = time.monotonic() - started
    print(f"Source archive check passed in {elapsed:.1f}s. {summary.getvalue().strip()}")
    return elapsed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    builder = commands.add_parser("build", help="Create a complete vendored source archive")
    builder.add_argument("--output", required=True, type=Path)
    builder.add_argument(
        "--version", help="Release version, or a nightly version to stamp in staging"
    )
    checker = commands.add_parser(
        "check",
        help="Check the shipped sources and exercise a rebuild with a modified fork offline",
    )
    checker.add_argument("archive", type=Path)
    args = parser.parse_args()
    if args.command == "build":
        build(args.output, args.version)
    else:
        check(args.archive)


if __name__ == "__main__":
    main()
