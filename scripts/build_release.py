"""Build and verify a portable native ReShiki distribution."""

import argparse
import hashlib
import json
import math
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

from build_inchi_helper import RELEASE_TARGETS
from check_runtime_dependencies import (
    verify_macos_workers,
    verify_payload,
    verify_runtime,
    verify_single_executable,
)
from license_notices import write_notices

ROOT = Path(__file__).resolve().parents[1]


def run(command, **kwargs):
    return subprocess.run([str(value) for value in command], check=True, **kwargs)


def checksum(output):
    with output.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    Path(str(output) + ".sha256").write_text(
        f"{digest}  {output.name}\n", encoding="ascii", newline="\n"
    )


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


def verify_geometry_dependencies(binary):
    """Reject external chemistry/Python libraries and Windows CRT redistributables."""
    binary = Path(binary).resolve(strict=True)
    system = platform.system()
    if system == "Darwin":
        command = ["otool", "-L", binary]
    elif system == "Windows":
        command = ["dumpbin", "/DEPENDENTS", binary]
    elif system == "Linux":
        command = ["readelf", "-d", binary]
    else:
        raise ValueError(f"Unsupported geometry dependency check: {system}")
    try:
        output = run(
            command,
            capture_output=True,
            text=True,
            timeout=30,
            env=dict(os.environ, LC_ALL="C"),
        ).stdout
    except FileNotFoundError as error:
        raise ValueError(f"Geometry package verification requires {command[0]}") from error
    if system == "Darwin":
        dependencies = [line.strip().split(" (", 1)[0] for line in output.splitlines()[1:]]
    elif system == "Windows":
        dependencies = re.findall(
            r"^\s+([A-Za-z0-9_.-]+\.dll)\s*$", output, re.MULTILINE | re.IGNORECASE
        )
    else:
        dependencies = re.findall(r"\(NEEDED\).*\[([^\]]+)\]", output)
    for dependency in dependencies:
        name = dependency.replace("\\", "/").rsplit("/", 1)[-1].lower()
        if (
            "rdkit" in name
            or "cosmolkit" in name
            or "boost" in name
            or "python" in name
            or (
                system == "Windows"
                and re.match(r"(?:msvcp|msvcr|vcruntime|concrt|vcomp|mfc)\d", name)
            )
        ):
            raise ValueError(
                f"Geometry executable requires an external runtime library: {dependency}"
            )


def geometry_smoke_request(field):
    """Exercise ETKDG embedding and explicit force-field optimization of ethanol."""
    operation = dict(
        atoms=[
            dict(
                atomic_number=number,
                isotope=0,
                charge=0,
                explicit_h=0,
                no_implicit=False,
                aromatic=False,
                radical=0,
                chiral_tag=0,
            )
            for number in (6, 6, 8)
        ],
        bonds=[
            dict(a=a, b=b, order=1, aromatic=False, stereo=0, stereo_atoms=None)
            for a, b in ((0, 1), (1, 2))
        ],
        field=field,
        operation="Generate",
        coordinates=[],
        fixed_atoms=[],
        conformers=2,
        seed=42,
        max_iterations=1000,
    )
    body = json.dumps(dict(heap_bytes=256 * 1024 * 1024, operation=operation)).encode()
    return b"RSHGEOM1" + struct.pack("<HHI", 1, 0, len(body)) + body


def verify_geometry_response(response, field, version):
    if (
        len(response) < 16
        or len(response) > 4 * 1024 * 1024
        or response[:12] != b"RSHGEOM1\x01\x00\x00\x00"
    ):
        raise ValueError("Packaged geometry worker returned an incompatible protocol")
    if struct.unpack("<I", response[12:16])[0] != len(response) - 16:
        raise ValueError("Packaged geometry worker returned an invalid frame length")
    result = json.loads(response[16:])

    def finite(value):
        try:
            return (
                isinstance(value, (int, float))
                and not isinstance(value, bool)
                and math.isfinite(value)
            )
        except OverflowError:
            return False

    try:
        geometry = result["result"]["Ok"]
        coordinates = geometry["coordinates"]
        parents = geometry["hydrogen_parents"]
        valid = (
            result["version"] == version
            and geometry["field"] == field
            and geometry["original_count"] == 3
            and len(coordinates) == 9
            and len(parents) == 6
            and all(type(parent) is int and 0 <= parent < 3 for parent in parents)
            and [parents.count(i) for i in range(3)] == [3, 2, 1]
            and all(
                len(point) == 3 and all(finite(value) and abs(value) < 1000 for value in point)
                for point in coordinates
            )
            and finite(geometry["initial_energy"])
            and finite(geometry["energy"])
            and geometry["energy"] <= geometry["initial_energy"] + 1e-5
            and geometry["converged"] is True
            and geometry["gradient"] is None
            and all(
                0.9 < math.dist(coordinates[a], coordinates[b]) < 2.0 for a, b in ((0, 1), (1, 2))
            )
            and all(
                0.7 < math.dist(coordinates[3 + i], coordinates[parent]) < 1.4
                for i, parent in enumerate(parents)
            )
        )
    except (KeyError, TypeError, IndexError) as error:
        raise ValueError("Packaged geometry worker returned invalid ethanol geometry") from error
    if not valid:
        raise ValueError(f"Packaged geometry worker did not optimize ethanol with {field}")


def geometry_payload_inventory(directory):
    """Record every relocated entry and file byte before launching a worker."""
    inventory = {}
    for entry in Path(directory).rglob("*"):
        relative = str(entry.relative_to(directory))
        if entry.is_symlink():
            inventory[relative] = ("symlink", os.readlink(entry))
        elif entry.is_dir():
            inventory[relative] = ("directory",)
        elif entry.is_file():
            with entry.open("rb") as stream:
                inventory[relative] = ("file", hashlib.file_digest(stream, "sha256").hexdigest())
        else:
            raise ValueError(f"Unexpected entry in geometry package: {relative}")
    return inventory


def verify_geometry_worker(binary, version, *, signed=False):
    """Launch the sole relocated executable without Python or chemistry libraries."""
    binary = Path(binary).resolve(strict=True)
    verify_geometry_dependencies(binary)
    source_app = None
    if signed and platform.system() == "Darwin":
        if (
            binary.parent.name != "MacOS"
            or binary.parent.parent.name != "Contents"
            or binary.parents[2].suffix != ".app"
        ):
            raise ValueError("Signed macOS geometry verification requires a standard .app bundle")
        source_app = binary.parents[2]
        run(["codesign", "--verify", "--deep", "--strict", source_app], timeout=30)
        verify_payload(source_app)
        verify_single_executable(binary, source_app)
        source_inventory = geometry_payload_inventory(source_app)
    with tempfile.TemporaryDirectory(prefix="ReShiki geometry distribution ") as temporary:
        root = Path(temporary)
        isolated = root / "Only executable"
        isolated.mkdir()
        copied_app = None
        if source_app is not None:
            # A Developer ID bundle signature binds the executable to Info.plist.
            # Relocate its unchanged bundle instead of detaching the signed Mach-O.
            copied_app = isolated / source_app.name
            run(["ditto", source_app, copied_app], timeout=120)
            executable = copied_app / binary.relative_to(source_app)
            if geometry_payload_inventory(copied_app) != source_inventory:
                raise ValueError("Relocated signed geometry bundle bytes or entries changed")
            run(["codesign", "--verify", "--deep", "--strict", copied_app], timeout=30)
            verify_payload(isolated)
            verify_single_executable(executable, isolated)
        else:
            executable = isolated / binary.name
            shutil.copy2(binary, executable)
        with binary.open("rb") as source, executable.open("rb") as copied:
            source_sha256 = hashlib.file_digest(source, "sha256").hexdigest()
            copied_sha256 = hashlib.file_digest(copied, "sha256").hexdigest()
        print(
            json.dumps(
                dict(geometry_source_sha256=source_sha256, geometry_copy_sha256=copied_sha256)
            )
        )
        if copied_sha256 != source_sha256:
            raise ValueError("Relocated geometry executable bytes changed")
        empty_path = root / "Empty PATH"
        empty_path.mkdir()
        initial_inventory = geometry_payload_inventory(root)
        environment = dict(os.environ)
        for key in (
            "VIRTUAL_ENV",
            "PYTHONPATH",
            "PYTHONHOME",
            "UV_PROJECT_ENVIRONMENT",
            "LD_LIBRARY_PATH",
            "LD_PRELOAD",
            "LD_AUDIT",
            "LD_DEBUG_OUTPUT",
            "DYLD_LIBRARY_PATH",
            "DYLD_FALLBACK_LIBRARY_PATH",
            "DYLD_FRAMEWORK_PATH",
            "DYLD_FALLBACK_FRAMEWORK_PATH",
            "DYLD_INSERT_LIBRARIES",
            "DYLD_VERSIONED_LIBRARY_PATH",
            "DYLD_VERSIONED_FRAMEWORK_PATH",
            "RDBASE",
            "RDKIT_HOME",
            "RDKIT_ROOT",
            "BOOST_ROOT",
            "RESHIKI_GEOMETRY_HELPER",
            "MORUNO_GEOMETRY_HELPER",
            "RESHIKI_INCHI_HELPER",
            "MORUNO_INCHI_HELPER",
        ):
            environment.pop(key, None)
        environment.update(
            PATH=str(empty_path),
            HOME=str(root / "Home"),
            USERPROFILE=str(root / "Home"),
            XDG_CACHE_HOME=str(root / "Cache"),
            XDG_DATA_HOME=str(root / "Data"),
            APPDATA=str(root / "Roaming"),
            LOCALAPPDATA=str(root / "Local"),
            UV_CACHE_DIR=str(root / "UV cache"),
            UV_OFFLINE="1",
            PYTHONNOUSERSITE="1",
        )
        for prefix in ("RESHIKI", "MORUNO"):
            for key in ("PYTHON", "REFERENCE_PYTHON", "UV", "ROOT", "RUNTIME_DIR", "DATA_DIR"):
                environment[f"{prefix}_{key}"] = str(root / f"Missing {prefix} {key}")
        for field in ("MMFF94", "UFF"):
            response = run(
                [executable, "--geometry-worker"],
                input=geometry_smoke_request(field),
                cwd=root,
                env=environment,
                capture_output=True,
                timeout=120,
            ).stdout
            verify_geometry_response(response, field, version)
        # Signed bundle metadata must remain intact; workers cannot stage payloads.
        if geometry_payload_inventory(root) != initial_inventory:
            raise ValueError("Packaged geometry worker created an unexpected runtime payload")
        if copied_app is not None:
            verify_payload(root)
            verify_single_executable(executable, root)
            run(["codesign", "--verify", "--deep", "--strict", copied_app], timeout=30)
    print(
        "Rust MMFF94 and UFF passed from the sole relocated executable without Python or RDKit libraries."
    )


def notices(destination):
    from geometry_source import verify as verify_geometry_source

    metadata = json.loads(
        run(
            ["cargo", "metadata", "--format-version", "1", "--locked"],
            cwd=ROOT,
            capture_output=True,
            text=True,
        ).stdout
    )
    verify_geometry_source(ROOT, cargo_metadata=metadata)
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
        verify_geometry_worker(binary, metadata["geometry"]["version"], signed=signed)
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
    from geometry_source import verify as geometry_metadata

    metadata = dict(
        version=version(),
        platform=system,
        architecture=arch,
        rust_target=target,
        inchi=dict(version=INCHI_VERSION, dependency=dependency(ROOT), runtime="self-process"),
        geometry=dict(geometry_metadata(ROOT), linkage="static"),
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
    checksum(output)
    print(output)
    if args.installer:
        from installers import build_installer

        print(build_installer(folder, ROOT / "dist/releases", args.sign))


if __name__ == "__main__":
    main()
