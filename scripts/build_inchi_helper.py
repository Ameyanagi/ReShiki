"""Build the pure Rust InChI helper from the locked Cargo dependency graph."""

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INCHI_VERSION = "1.07.5"

RELEASE_TARGETS = {
    "aarch64-apple-darwin": ("macos", "arm64"),
    "x86_64-apple-darwin": ("macos", "x64"),
    "x86_64-pc-windows-msvc": ("windows", "x64"),
    "aarch64-pc-windows-msvc": ("windows", "arm64"),
    "x86_64-unknown-linux-gnu": ("linux", "x64"),
    "aarch64-unknown-linux-gnu": ("linux", "arm64"),
}
SUPPORTED_TARGETS = tuple(RELEASE_TARGETS)


def verify_executable(path, target):
    with path.open("rb") as stream:
        header = stream.read(64)
        if header[:4] == b"\x7fELF" and header[4:6] == b"\x02\x01":
            machine = int.from_bytes(header[18:20], "little")
            actual = {62: "x86_64-unknown-linux-gnu", 183: "aarch64-unknown-linux-gnu"}.get(machine)
        elif header[:4] == b"\xcf\xfa\xed\xfe":
            machine = int.from_bytes(header[4:8], "little")
            actual = {0x100000C: "aarch64-apple-darwin", 0x1000007: "x86_64-apple-darwin"}.get(
                machine
            )
        elif header[:2] == b"MZ" and len(header) == 64:
            offset = int.from_bytes(header[60:64], "little")
            if offset > 1024 * 1024:
                raise ValueError("Invalid native PE header offset")
            stream.seek(offset)
            pe = stream.read(6)
            if pe[:4] != b"PE\0\0":
                raise ValueError("Invalid native PE header")
            machine = int.from_bytes(pe[4:6], "little")
            actual = {0x8664: "x86_64-pc-windows-msvc", 0xAA64: "aarch64-pc-windows-msvc"}.get(
                machine
            )
        else:
            raise ValueError("Unrecognized native helper executable")
    if actual is None or (target and target != actual):
        raise ValueError(f"Native helper architecture mismatch: expected {target}, found {actual}")
    return actual


def source_hashes(root=ROOT):
    paths = [
        root / name
        for name in (
            "Cargo.toml",
            "Cargo.lock",
            "build.rs",
            ".cargo/config.toml",
            "scripts/build_inchi_helper.py",
        )
    ]
    for folder in ("src", "crates", "native", "tools/inchi-helper"):
        paths.extend(
            p
            for p in (root / folder).rglob("*")
            if p.is_file() and (p.suffix in (".rs", ".swift") or p.name == "Cargo.toml")
        )
    return {
        p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
        for p in sorted(paths)
    }


def dependency(root=ROOT):
    lock = tomllib.loads((root / "Cargo.lock").read_text())
    package = next(p for p in lock["package"] if p["name"] == "cosmolkit-inchi")
    required = tomllib.loads((root / "Cargo.toml").read_text())["dependencies"]["cosmolkit-inchi"]
    if (
        not isinstance(required, dict)
        or required.get("version") != "=" + package["version"]
        or not re.fullmatch(r"[0-9a-f]{40}", required.get("rev", ""))
        or not required.get("git", "").startswith("https://github.com/")
    ):
        raise ValueError("InChI Cargo dependency must pin a full Git revision and exact version")
    revision = required["rev"]
    source = f"git+{required['git']}?rev={revision}#{revision}"
    if package.get("source") != source:
        raise ValueError("InChI Cargo dependency does not match the locked Git revision")
    return dict(name=package["name"], version=package["version"], source=source, revision=revision)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--jobs", type=int, default=4, choices=range(1, 5))
    parser.add_argument("--output", type=Path)
    parser.add_argument("--target", choices=SUPPORTED_TARGETS)
    parser.add_argument(
        "--production",
        action="store_true",
        help="Build the release helper, without test executables",
    )
    args = parser.parse_args()
    root = ROOT
    cargo = os.environ.get("CARGO", "cargo")
    rustc = os.environ.get("RUSTC", "rustc")
    compiler = subprocess.run([rustc, "-vV"], check=True, capture_output=True, text=True).stdout
    host = next(
        line.removeprefix("host: ") for line in compiler.splitlines() if line.startswith("host: ")
    )
    target = args.target or host
    if target not in SUPPORTED_TARGETS:
        raise ValueError(f"Unsupported helper target: {target}")
    output = (args.output or root / "artifacts/inchi-helper").resolve()
    output.mkdir(parents=True, exist_ok=True)
    command = [
        cargo,
        "build",
        "--locked",
        "--bin",
        "reshiki-inchi-helper",
        "--target",
        target,
        "--jobs",
        str(args.jobs),
    ]
    if args.production:
        command.append("--release")
    subprocess.run(command, cwd=root, check=True)
    metadata = json.loads(
        subprocess.run(
            [cargo, "metadata", "--no-deps", "--format-version", "1", "--locked"],
            cwd=root,
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    )
    suffix = ".exe" if "windows" in target else ""
    name = "reshiki-inchi-helper" + suffix
    built = (
        Path(metadata["target_directory"])
        / target
        / ("release" if args.production else "debug")
        / name
    )
    executable = output / name
    shutil.copy2(built, executable)
    verify_executable(executable, target)
    if not args.production:
        stub = output / ("inchi-helper-stub" + suffix)
        subprocess.run(
            [
                rustc,
                "--edition=2024",
                "--target",
                target,
                str(root / "tests/common/inchi_helper_stub.rs"),
                "-o",
                str(stub),
            ],
            check=True,
        )
        # Warm native test executables before fault tests copy/link them. A
        # cross-target executable can only be checked on its destination host.
        if target == host:
            subprocess.run([str(stub)], input=b"", capture_output=True, timeout=60, check=True)
    record = dict(
        inchi_version=INCHI_VERSION,
        dependency=dependency(root),
        source_hashes=source_hashes(root),
        executable_sha256=hashlib.sha256(executable.read_bytes()).hexdigest(),
        target=target,
        production=args.production,
        command=command,
        rust_compiler=compiler,
    )
    (output / "build.json").write_text(json.dumps(record, indent=2) + "\n")
    print(executable)


if __name__ == "__main__":
    main()
