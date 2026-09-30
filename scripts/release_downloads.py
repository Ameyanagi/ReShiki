"""Verify the complete release asset set and produce its download table and checksums."""

import argparse
import hashlib
import json
import re
import zipfile
from pathlib import Path
from urllib.parse import quote

# Both publishing workflows use this exact package contract.
PLATFORMS = (
    ("macOS", "Apple Silicon (ARM64)", "macos-arm64", ".dmg", ".zip"),
    ("macOS", "Intel (x64)", "macos-x64", ".dmg", ".zip"),
    ("Windows", "Intel / AMD (x64)", "windows-x64", "-setup.exe", ".zip"),
    ("Windows", "ARM64", "windows-arm64", "-setup.exe", ".zip"),
    ("Linux", "Intel / AMD (x64)", "linux-x64", None, ".tar.gz"),
    ("Linux", "ARM64", "linux-arm64", None, ".tar.gz"),
)
VERSION = r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?"


def release_version(tag: str) -> str:
    if tag.startswith("nightly-"):
        version = tag.removeprefix("nightly-")
        if not re.fullmatch(
            r"[0-9]+\.[0-9]+\.[0-9]+-nightly\.[1-9][0-9]{7}\.[1-9][0-9]*\.[1-9][0-9]*",
            version,
        ):
            raise ValueError("Invalid nightly release tag")
    elif tag.startswith("v"):
        version = tag.removeprefix("v")
    else:
        raise ValueError("Expected a vVERSION or nightly-VERSION release tag")
    if not re.fullmatch(VERSION, version):
        raise ValueError("Invalid release version")
    return version


def filenames(version: str) -> list[str]:
    return [
        f"reshiki-{version}-{platform}{suffix}"
        for _, _, platform, installer, portable in PLATFORMS
        for suffix in (installer, portable)
        if suffix
    ]


def verify_assets(directory: Path, version: str) -> list[str]:
    expected = filenames(version)
    found = {path.name for path in directory.glob("reshiki-*")}
    required = set(expected) | {name + ".sha256" for name in expected}
    if found != required:
        raise ValueError(
            f"Release asset set mismatch: missing={sorted(required - found)}, "
            f"unexpected={sorted(found - required)}"
        )
    checksums = []
    for name in expected:
        path = directory / name
        if not path.is_file() or path.is_symlink():
            raise ValueError(f"Release asset is not a regular file: {name}")
        with path.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        checksum = f"{digest}  {name}"
        if Path(str(path) + ".sha256").read_text(encoding="ascii").strip() != checksum:
            raise ValueError(f"Release checksum mismatch: {name}")
        checksums.append(checksum)
    for architecture in ("arm64", "x64"):
        name = f"reshiki-{version}-macos-{architecture}"
        with zipfile.ZipFile(directory / f"{name}.zip") as archive:
            metadata = json.loads(archive.read(f"{name}/build.json"))
        expected_metadata = {
            "version": version,
            "platform": "macos",
            "architecture": architecture,
            "signed": True,
            "notarized": True,
        }
        if any(metadata.get(key) != value for key, value in expected_metadata.items()):
            raise ValueError(f"macOS archive is not the signed, notarized release: {name}")
    return checksums


def prepare(directory: Path, repository: str, tag: str) -> str:
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
        raise ValueError("Expected an owner/repository name")
    version = release_version(tag)
    checksums = verify_assets(directory, version)
    base = f"https://github.com/{repository}/releases/download/{quote(tag, safe='')}"

    def link(name: str, label: str) -> str:
        return f"[{label}]({base}/{quote(name, safe='')})"

    lines = [
        "## Downloads",
        "",
        "| Operating system | Architecture | Installer | Portable archive |",
        "| --- | --- | --- | --- |",
    ]
    for system, architecture, platform, installer, portable in PLATFORMS:
        stem = f"reshiki-{version}-{platform}"
        installation = (
            link(stem + installer, "DMG" if installer == ".dmg" else "Setup EXE")
            if installer
            else "—"
        )
        archive = link(stem + portable, "ZIP" if portable == ".zip" else "tar.gz")
        lines.append(f"| {system} | {architecture} | {installation} | {archive} |")
    lines.extend(
        [
            "",
            "macOS downloads are signed and notarized. Windows and Linux downloads are unsigned.",
            "Installers replace an existing ReShiki installation. To retain another version, extract a portable archive into a separate folder and keep its contents together.",
            "",
            f"Verify downloads with {link('SHA256SUMS', 'SHA256SUMS')}. "
            "[Latest stable release](https://github.com/" + repository + "/releases/latest).",
            "",
        ]
    )
    (directory / "SHA256SUMS").write_text(
        "\n".join(checksums) + "\n", encoding="ascii", newline="\n"
    )
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--notes", type=Path, required=True)
    parser.add_argument("--append", action="store_true")
    args = parser.parse_args()
    table = prepare(args.directory, args.repository, args.tag)
    prefix = args.notes.read_text(encoding="utf-8").rstrip() + "\n\n" if args.append else ""
    args.notes.write_text(prefix + table, encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main()
