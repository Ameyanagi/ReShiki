"""Stage, repackage, and verify qualified Windows builds around SignPath requests.

The workflow submits the staged files using SignPath's GitHub action. This script
never receives a signing API token, rebuilds application code, or publishes files.
"""

import argparse
import hashlib
import json
import os
import re
import shutil
import stat
import struct
import sys
import zipfile
from pathlib import Path, PurePosixPath

from build_release import ROOT, archive, checksum, run, verify_archive, verify_binary, version
from installers import verify_windows_installer, verify_windows_upgrade_with_running_agent
from installers import windows_installer as build_installer

POLICIES = {"test-signing", "release-signing"}
ARCHITECTURES = {"x64", "arm64"}
VERSION = r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?"


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def validate_settings(architecture, policy, certificate_sha256):
    if architecture not in ARCHITECTURES or policy not in POLICIES:
        raise ValueError("Expected a Windows architecture and test/release signing policy")
    if not re.fullmatch(r"[0-9a-fA-F]{64}", certificate_sha256):
        raise ValueError("Configure the expected signing certificate's DER SHA-256")
    if not re.fullmatch(VERSION, version()):
        raise ValueError("Unsupported signing package version")


def pe_fields(data):
    """Offsets excluded by Authenticode; reject malformed or unsupported headers."""
    if len(data) < 64 or data[:2] != b"MZ":
        raise ValueError("Expected a PE executable")
    pe = struct.unpack_from("<I", data, 60)[0]
    if pe + 24 > len(data) or data[pe : pe + 4] != b"PE\0\0":
        raise ValueError("Invalid PE header")
    optional = pe + 24
    optional_size = struct.unpack_from("<H", data, pe + 20)[0]
    if optional + optional_size > len(data) or optional_size < 2:
        raise ValueError("Invalid PE optional header")
    magic = struct.unpack_from("<H", data, optional)[0]
    if magic not in {0x10B, 0x20B}:
        raise ValueError("Unsupported PE optional header")
    directory = optional + (96 if magic == 0x10B else 112)
    security = directory + 4 * 8
    if security + 8 > optional + optional_size:
        raise ValueError("Missing PE certificate directory")
    if struct.unpack_from("<I", data, directory - 4)[0] < 5:
        raise ValueError("Missing PE certificate directory")
    offset, size = struct.unpack_from("<II", data, security)
    return optional + 64, security, offset, size


def verify_signature_only_change(unsigned, signed):
    """A signer may change the checksum/certificate fields and append a signature."""
    original = unsigned.read_bytes()
    result = signed.read_bytes()
    checksum_offset, directory_offset, old_offset, old_size = pe_fields(original)
    checksum_after, directory_after, certificate_offset, certificate_size = pe_fields(result)
    if old_offset or old_size:
        raise ValueError("The qualified input already has a certificate table")
    if (checksum_after, directory_after) != (checksum_offset, directory_offset):
        raise ValueError("Signing changed the PE header layout")
    if (
        certificate_offset < len(original)
        or certificate_offset - len(original) > 7
        or certificate_offset % 8
        or certificate_size < 8
        or certificate_offset + certificate_size != len(result)
        or any(result[len(original) : certificate_offset])
    ):
        raise ValueError("Signing changed bytes outside the appended certificate table")
    before = bytearray(original)
    after = bytearray(result[: len(original)])
    for offset, size in ((checksum_offset, 4), (directory_offset, 8)):
        before[offset : offset + size] = bytes(size)
        after[offset : offset + size] = bytes(size)
    if before != after:
        raise ValueError("Signing changed the qualified executable's contents")


def extract_qualified(source, destination, expected_folder):
    """Only ordinary files in the named package may enter the signing staging area."""
    with zipfile.ZipFile(source) as stream:
        names = set()
        for entry in stream.infolist():
            path = PurePosixPath(entry.filename)
            mode = entry.external_attr >> 16
            if (
                "\\" in entry.filename
                or path.is_absolute()
                or ".." in path.parts
                or not path.parts
                or path.parts[0] != expected_folder
                or ":" in entry.filename
                or stat.S_ISLNK(mode)
                or entry.filename.casefold() in names
            ):
                raise ValueError("Unsafe or unexpected Windows archive entry")
            names.add(entry.filename.casefold())
        stream.extractall(destination)
    folder = destination / expected_folder
    if not folder.is_dir():
        raise ValueError("Missing qualified application directory")
    return folder


def verify_signature(path, policy, certificate_sha256, package_version):
    result = run(
        [
            "pwsh",
            "-NoLogo",
            "-NoProfile",
            "-File",
            ROOT / "scripts/windows_signatures.ps1",
            "-Path",
            path.resolve(),
            "-Policy",
            policy,
            "-CertificateSha256",
            certificate_sha256,
            "-ProductVersion",
            package_version,
        ],
        capture_output=True,
        text=True,
    )
    evidence = json.loads(result.stdout)
    if evidence.get("certificate_sha256", "").lower() != certificate_sha256.lower():
        raise ValueError("Signature verification returned an unexpected certificate")
    return evidence


def load_state(root, architecture, policy, certificate_sha256):
    state = json.loads((root / "windows-signing/state.json").read_text(encoding="utf-8"))
    expected = {
        "version": version(),
        "architecture": architecture,
        "policy": policy,
        "certificate_sha256": certificate_sha256.lower(),
        "commit": os.environ["GITHUB_SHA"],
    }
    if any(state.get(key) != value for key, value in expected.items()):
        raise ValueError("Signing staging identity does not match this workflow")
    return state


def prepare(root, architecture, policy, certificate_sha256):
    validate_settings(architecture, policy, certificate_sha256)
    stem = f"reshiki-{version()}-windows-{architecture}"
    inputs = list((root / "unsigned").glob("*.zip"))
    if len(inputs) != 1 or inputs[0].name != stem + ".zip":
        raise ValueError("Expected exactly the qualified Windows portable archive")
    source = inputs[0]
    source_digest = digest(source)
    if Path(str(source) + ".sha256").read_text(encoding="ascii").strip() != (
        f"{source_digest}  {source.name}"
    ):
        raise ValueError("Qualified archive checksum mismatch")
    work = root / "windows-signing"
    if work.exists() or (root / "signing-input").exists():
        raise ValueError("Windows signing staging already exists")
    folder = extract_qualified(source, work, stem)
    metadata = json.loads((folder / "build.json").read_text(encoding="utf-8"))
    expected = {
        "version": version(),
        "platform": "windows",
        "architecture": architecture,
        "commit": os.environ["GITHUB_SHA"],
        "signed": False,
        "notarized": False,
    }
    if any(metadata.get(key) != value for key, value in expected.items()):
        raise ValueError("Qualified Windows archive source, version, or architecture mismatch")
    binary = folder / "reshiki.exe"
    verify_binary(binary, "windows", architecture)
    _, _, certificate_offset, certificate_size = pe_fields(binary.read_bytes())
    if certificate_offset or certificate_size:
        raise ValueError("The qualified input already has a certificate table")
    app_input = root / "signing-input/app"
    app_input.mkdir(parents=True)
    shutil.copy2(binary, app_input / "reshiki.exe")
    state = {
        "version": version(),
        "architecture": architecture,
        "policy": policy,
        "certificate_sha256": certificate_sha256.lower(),
        "commit": os.environ["GITHUB_SHA"],
        "unsigned_sha256": source_digest,
        "unsigned_app_sha256": digest(binary),
    }
    (work / "state.json").write_text(json.dumps(state, indent=2) + "\n", encoding="utf-8")
    if output := os.environ.get("GITHUB_OUTPUT"):
        with Path(output).open("a", encoding="utf-8") as stream:
            stream.write(f"version={version()}\n")
    return state


def package(root, architecture, policy, certificate_sha256):
    validate_settings(architecture, policy, certificate_sha256)
    state = load_state(root, architecture, policy, certificate_sha256)
    stem = f"reshiki-{version()}-windows-{architecture}"
    folder = root / "windows-signing" / stem
    binary = folder / "reshiki.exe"
    if digest(binary) != state["unsigned_app_sha256"]:
        raise ValueError("Qualified application changed before signing")
    signed = root / "signed-app/reshiki.exe"
    verify_signature_only_change(binary, signed)
    evidence = verify_signature(signed, policy, certificate_sha256, version())
    shutil.copy2(signed, binary)
    metadata_path = folder / "build.json"
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    metadata.update(
        signed=True,
        notarized=False,
        unsigned_sha256=state["unsigned_sha256"],
        signing={
            "provider": "SignPath",
            "policy": policy,
            "certificate_sha256": certificate_sha256.lower(),
            "publicly_trusted": policy == "release-signing",
            "application": evidence,
        },
    )
    metadata_path.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    readme = folder / "README.txt"
    description = (
        "Windows application signed with SignPath Foundation."
        if policy == "release-signing"
        else "Internal SignPath test signature; certificate is not publicly trusted."
    )
    readme.write_text(
        readme.read_text(encoding="utf-8").replace(
            "This build has no publisher signature.", description
        ),
        encoding="utf-8",
    )
    portable = archive(folder, root / "dist/releases" / stem)
    verify_archive(portable, signed=True)
    checksum(portable)
    installer = build_installer(folder, root / "signing-input/installer")
    state["unsigned_installer_sha256"] = digest(installer)
    (root / "windows-signing/state.json").write_text(
        json.dumps(state, indent=2) + "\n", encoding="utf-8"
    )
    return installer


def verify(root, architecture, policy, certificate_sha256):
    validate_settings(architecture, policy, certificate_sha256)
    state = load_state(root, architecture, policy, certificate_sha256)
    stem = f"reshiki-{version()}-windows-{architecture}"
    folder = root / "windows-signing" / stem
    original = root / "signing-input/installer" / f"{stem}-setup.exe"
    signed = root / "signed-installer" / original.name
    if digest(original) != state["unsigned_installer_sha256"]:
        raise ValueError("Qualified installer changed before signing")
    verify_signature_only_change(original, signed)
    verify_signature(signed, policy, certificate_sha256, version())
    verify_signature(folder / "reshiki.exe", policy, certificate_sha256, version())
    verify_windows_installer(signed, folder)
    verify_windows_upgrade_with_running_agent(signed, folder)
    output = root / "dist/releases" / signed.name
    shutil.copy2(signed, output)
    checksum(output)
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("stage", choices=("prepare", "package", "verify"))
    parser.add_argument("--architecture", required=True, choices=sorted(ARCHITECTURES))
    parser.add_argument("--policy", required=True, choices=sorted(POLICIES))
    parser.add_argument("--certificate-sha256", required=True)
    args = parser.parse_args()
    if sys.platform != "win32":
        raise ValueError("Windows release signing stages require a Windows runner")
    globals()[args.stage](ROOT, args.architecture, args.policy, args.certificate_sha256)


if __name__ == "__main__":
    main()
