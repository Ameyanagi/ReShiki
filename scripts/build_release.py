"""Build and verify a portable native ReShiki distribution."""

import argparse
import hashlib
import json
import os
import platform
import plistlib
import re
import shutil
import struct
import subprocess
import tarfile
import tempfile
import tomllib
import zipfile
from pathlib import Path

from check_runtime_dependencies import (
    verify_macos_workers,
    verify_runtime,
    verify_single_executable,
)
from license_notices import write_notices

ROOT = Path(__file__).resolve().parents[1]
RELEASE_TARGETS = {
    "aarch64-apple-darwin": ("macos", "arm64"),
    "x86_64-apple-darwin": ("macos", "x64"),
    "x86_64-pc-windows-msvc": ("windows", "x64"),
    "aarch64-pc-windows-msvc": ("windows", "arm64"),
    "x86_64-unknown-linux-gnu": ("linux", "x64"),
    "aarch64-unknown-linux-gnu": ("linux", "arm64"),
}


def run(command, **kwargs):
    return subprocess.run([str(value) for value in command], check=True, **kwargs)


def version():
    return tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]


def numeric_version(value):
    """Keep prerelease identifiers out of macOS and Windows numeric version fields."""
    base = re.split(r"[-+]", value, maxsplit=1)[0]
    if not re.fullmatch(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)", base):
        raise ValueError("Expected a three-component numeric package version")
    # Windows VERSIONINFO stores each component as an unsigned 16-bit integer.
    if any(int(component) > 65535 for component in base.split(".")):
        raise ValueError("Package version exceeds a Windows version component")
    return base


def check_tag(tag):
    if tag != f"v{version()}":
        raise ValueError(f"Tag {tag!r} must match Cargo.toml version v{version()}")


def release_platform(target):
    """Name the native application, independently of the packaging Python process."""
    try:
        return RELEASE_TARGETS[target]
    except KeyError as error:
        raise ValueError(f"Unsupported release target: {target}") from error


def host_target():
    result = run(["rustc", "-vV"], capture_output=True, text=True)
    for line in result.stdout.splitlines():
        if line.startswith("host: "):
            return line.removeprefix("host: ").strip()
    raise ValueError("rustc did not report its host target")


def target_directory():
    """Respect Cargo configuration, including a target directory on another drive."""
    result = run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    return Path(json.loads(result.stdout)["target_directory"])


def verify_binary(binary, system, architecture):
    """Reject an accidentally packaged host binary in an ARM release, or vice versa."""
    machine = None
    with Path(binary).open("rb") as stream:
        header = stream.read(64)
        if system == "macos" and header[:4] == b"\xcf\xfa\xed\xfe":
            machine = {0x0100000C: "arm64", 0x01000007: "x64"}.get(
                int.from_bytes(header[4:8], "little")
            )
        elif system == "linux" and header[:6] == b"\x7fELF\x02\x01":
            machine = {183: "arm64", 62: "x64"}.get(int.from_bytes(header[18:20], "little"))
        elif system == "windows" and len(header) == 64 and header[:2] == b"MZ":
            stream.seek(int.from_bytes(header[60:64], "little"))
            pe = stream.read(6)
            if pe[:4] == b"PE\0\0":
                machine = {0xAA64: "arm64", 0x8664: "x64"}.get(int.from_bytes(pe[4:6], "little"))
    if machine != architecture:
        raise ValueError(f"Expected {system} {architecture} executable, found {machine}: {binary}")


def verify_inchi_worker(binary, version):
    """Parse methane by relaunching the packaged application in Rust worker mode."""
    body = json.dumps(
        dict(
            heap_bytes=64 * 1024 * 1024,
            operation={
                "Read": {
                    "inchi": "InChI=1S/CH4/h1H4",
                    "options": {"sanitize": True, "remove_hydrogens": False},
                }
            },
        )
    ).encode()
    request = b"RSHINCHI" + struct.pack("<HHI", 3, 0, len(body)) + body
    response = run(
        [binary, "--inchi-worker"], input=request, capture_output=True, timeout=15
    ).stdout
    if (
        len(response) > 8 * 1024 * 1024
        or response[:12] != b"RSHINCHI\x03\x00\x00\x00"
        or len(response) < 16
    ):
        raise ValueError("Packaged InChI helper returned an incompatible protocol")
    if struct.unpack("<I", response[12:16])[0] != len(response) - 16:
        raise ValueError("Packaged InChI helper returned an invalid frame length")
    result = json.loads(response[16:])
    try:
        imported = result["result"]["Ok"]["Imported"]
        state = imported["state"]
        atoms = state["graph"]["atoms"]
        valid = (
            result["version"] == version
            and imported["status"] == 0
            and len(atoms) == 1
            and atoms[0]["atomic_number"] == 6
            and atoms[0]["charge"] == 0
            and not state["graph"]["bonds"]
            and atoms[0]["explicit_hydrogens"] + state["valences"][0]["implicit_hydrogens"] == 4
        )
    except (KeyError, TypeError, IndexError) as error:
        raise ValueError("Packaged InChI helper returned an invalid molecule") from error
    if not valid:
        raise ValueError("Packaged InChI helper did not read methane correctly")


def notices(destination):
    metadata = json.loads(
        run(
            ["cargo", "metadata", "--format-version", "1", "--locked"],
            cwd=ROOT,
            capture_output=True,
            text=True,
        ).stdout
    )
    write_notices(ROOT, destination, metadata)


def remove_owned_path(path):
    if path.is_symlink() or path.is_file():
        path.unlink()
    elif path.is_dir():
        shutil.rmtree(path)


def mac_bundle(destination, profile, *, target=None):
    executable = destination / "Contents/MacOS/reshiki"
    executable.parent.mkdir(parents=True, exist_ok=True)
    staged = executable.with_suffix(".new")
    build = target_directory()
    if target:
        build /= target
    shutil.copy2(build / profile / "reshiki", staged)
    staged.replace(executable)
    info = dict(
        CFBundleName="ReShiki",
        CFBundleDisplayName="ReShiki",
        CFBundleIdentifier="dev.reshiki.editor",
        CFBundleExecutable="reshiki",
        CFBundlePackageType="APPL",
        CFBundleShortVersionString=numeric_version(version()),
        CFBundleVersion=numeric_version(version()),
        ReShikiPackageVersion=version(),
        NSHighResolutionCapable=True,
        LSMinimumSystemVersion=os.environ.get("MACOSX_DEPLOYMENT_TARGET", "14.0"),
        CFBundleIconFile="ReShiki.icns",
        CFBundleDocumentTypes=[
            dict(
                CFBundleTypeName="ReShiki drawing",
                CFBundleTypeRole="Editor",
                LSHandlerRank="Owner",
                CFBundleTypeExtensions=["rsk", "reshiki", "moruno"],
                LSItemContentTypes=["dev.reshiki.drawing"],
                CFBundleTypeIconFile="ReShiki.icns",
            )
        ],
        UTExportedTypeDeclarations=[
            dict(
                UTTypeIdentifier="dev.reshiki.drawing",
                UTTypeDescription="ReShiki drawing",
                UTTypeConformsTo=["public.json"],
                UTTypeTagSpecification={"public.filename-extension": ["rsk", "reshiki", "moruno"]},
            )
        ],
    )
    resources = destination / "Contents/Resources"
    resources.mkdir(parents=True, exist_ok=True)
    shutil.copy2(ROOT / "assets/branding/reshiki.icns", resources / "ReShiki.icns")
    with (destination / "Contents/Info.plist").open("wb") as stream:
        plistlib.dump(info, stream)
    # A rebuilt development bundle must retire only the former app-owned payload.
    for old in (
        "Contents/Resources/chemistry",
        "Contents/MacOS/reshiki-inchi-helper",
        "Contents/MacOS/reshiki-clipboard",
        "Contents/Helpers/ReShiki Print.app",
    ):
        remove_owned_path(destination / old)
    notices(resources / "Licenses")
    # Development/manual builds; release signing replaces the ad-hoc signature.
    run(["codesign", "--force", "--deep", "--sign", "-", destination])
    return destination


def archive(folder, output):
    output.parent.mkdir(parents=True, exist_ok=True)
    if platform.system() == "Darwin":
        output = Path(str(output) + ".zip")
        run(["ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", folder, output])
    elif platform.system() == "Windows":
        output = Path(str(output) + ".zip")
        # Source files may have reproducible 1970 timestamps; ZIP starts in 1980.
        with zipfile.ZipFile(output, "w", zipfile.ZIP_DEFLATED, strict_timestamps=False) as stream:
            for entry in sorted(folder.rglob("*")):
                if entry.is_file():
                    stream.write(entry, entry.relative_to(folder.parent))
    else:
        output = Path(str(output) + ".tar.gz")
        with tarfile.open(output, "w:gz") as stream:
            stream.add(folder, arcname=folder.name)
    return output


def verify_archive(archive_path, signed=False):
    with tempfile.TemporaryDirectory(prefix="ReShiki package check ") as temporary:
        extracted = Path(temporary)
        if platform.system() == "Darwin":
            run(["ditto", "-x", "-k", archive_path, extracted])
        elif archive_path.name.endswith(".tar.gz"):
            with tarfile.open(archive_path) as stream:
                stream.extractall(extracted, filter="data")
        else:
            with zipfile.ZipFile(archive_path) as stream:
                stream.extractall(extracted)
        folders = [p for p in extracted.iterdir() if p.is_dir() and p.name != "__MACOSX"]
        if len(folders) != 1:
            raise ValueError("Expected one application directory")
        folder = folders[0]
        metadata = json.loads((folder / "build.json").read_text(encoding="utf-8"))
        if platform.system() == "Darwin":
            app = folder / "ReShiki.app"
            binary = app / "Contents/MacOS/reshiki"
            run(["codesign", "--verify", "--deep", "--strict", app])
            if signed:
                from sign_macos import verify_app

                verify_app(app)
        else:
            binary = folder / ("reshiki.exe" if metadata["platform"] == "windows" else "reshiki")
        verify_binary(binary, metadata["platform"], metadata["architecture"])
        verify_inchi_worker(binary, metadata["inchi"]["version"])
        verify_single_executable(binary, folder)
        verify_runtime(binary, folder)
        if platform.system() == "Darwin":
            verify_macos_workers(binary)
            run(["codesign", "--verify", "--deep", "--strict", app])
        print("Extracted application and native chemistry verified without Python or uv.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag")
    parser.add_argument("--sign", action="store_true")
    parser.add_argument(
        "--installer",
        action="store_true",
        help="Also build and verify a Windows setup or macOS disk image",
    )
    parser.add_argument("--target", choices=sorted(RELEASE_TARGETS))
    parser.add_argument("--check-tag-only", action="store_true")
    args = parser.parse_args()
    if args.tag:
        check_tag(args.tag)
    if args.check_tag_only:
        return
    if args.sign and platform.system() != "Darwin":
        raise ValueError("Developer ID signing requires macOS")
    target = args.target or host_target()
    system, arch = release_platform(target)
    if {"Darwin": "macos", "Windows": "windows", "Linux": "linux"}.get(platform.system()) != system:
        raise ValueError(
            "Build and verify a release on a runner with the matching operating system"
        )
    name = f"reshiki-{version()}-{system}-{arch}"
    # Keep dependency notices outside Cargo's target tree: rust-cache treats
    # nested crate sources as build output and removes their test directories.
    # This also stays separate from the development app in dist/ReShiki.app.
    folder = ROOT / "build/release-bundles" / name
    if folder.exists():
        shutil.rmtree(folder)
    folder.mkdir(parents=True)
    run(
        ["cargo", "build", "--release", "--locked", "--bin", "reshiki", "--target", target],
        cwd=ROOT,
    )
    build = target_directory() / target / "release"
    binary_name = "reshiki.exe" if system == "windows" else "reshiki"
    verify_binary(build / binary_name, system, arch)
    if system == "macos":
        app = mac_bundle(folder / "ReShiki.app", "release", target=target)
        if args.sign:
            from sign_macos import sign_and_notarize

            sign_and_notarize(app)
    else:
        shutil.copy2(build / binary_name, folder / binary_name)
        notices(folder / "Licenses")
    from build_inchi_helper import INCHI_VERSION, dependency

    metadata = dict(
        version=version(),
        platform=system,
        architecture=arch,
        rust_target=target,
        inchi=dict(version=INCHI_VERSION, dependency=dependency(ROOT), runtime="self-process"),
        signed=args.sign,
        notarized=args.sign,
        commit=os.environ.get("GITHUB_SHA", "local"),
    )
    (folder / "build.json").write_text(json.dumps(metadata, indent=2) + "\n")
    (folder / "README.txt").write_text(
        "ReShiki — molecular drawing workspace\n\n"
        "Drawing and chemistry tools are included and work offline.\n"
        "Keep the entire extracted folder together.\n"
        "Documentation: https://reshiki.com/\n"
        + (
            "Windows 11 on ARM is required. The application is native ARM64.\n"
            if system == "windows" and arch == "arm64"
            else ""
        )
        + (
            "macOS application signed with Developer ID and notarized by Apple.\n"
            if args.sign
            else "This build has no publisher signature.\n"
        ),
        encoding="utf-8",
    )
    output = archive(folder, ROOT / "dist/releases" / name)
    verify_archive(output, args.sign)
    with output.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    Path(str(output) + ".sha256").write_text(
        f"{digest}  {output.name}\n", encoding="ascii", newline="\n"
    )
    print(output)
    if args.installer:
        from installers import build_installer

        print(build_installer(folder, ROOT / "dist/releases", args.sign))


if __name__ == "__main__":
    main()
