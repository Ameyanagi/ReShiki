"""Build and verify a portable native ReShiki distribution, with a uv-managed local chemistry environment."""

import argparse
import hashlib
import json
import os
import platform
import plistlib
import shutil
import struct
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RELEASE_TARGETS = {
    "aarch64-apple-darwin": ("macos", "arm64"),
    "x86_64-pc-windows-msvc": ("windows", "x64"),
    "aarch64-pc-windows-msvc": ("windows", "arm64"),
    "x86_64-unknown-linux-gnu": ("linux", "x64"),
    "aarch64-unknown-linux-gnu": ("linux", "arm64"),
}


def run(command, **kwargs):
    return subprocess.run([str(value) for value in command], check=True, **kwargs)


def version():
    return tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]


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


def chemistry_architecture(system, architecture):
    # Windows 11 ARM runs the available RDKit x64 wheel in a separate process.
    return "x64" if system == "windows" else architecture


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


def inchi_helper_name(system):
    return "reshiki-inchi-helper.exe" if system == "windows" else "reshiki-inchi-helper"


def verify_inchi_build(binary, target):
    from inchi_source import MANIFEST, manifest
    from inchi_source_patch import patch_manifest

    binary = Path(binary).resolve(strict=True)
    reference = manifest()
    metadata = json.loads(binary.with_name("build.json").read_text(encoding="utf-8"))
    required = {
        "inchi_version": reference["inchi_version"],
        "archive_sha256": reference["archive_sha256"],
        "manifest_sha256": hashlib.sha256(MANIFEST.read_bytes()).hexdigest(),
        "executable_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "target": target,
        "patched_source_hashes": {
            relative: patch["patched_sha256"]
            for relative, patch in patch_manifest()["files"].items()
        },
    }
    if any(metadata.get(key) != value for key, value in required.items()):
        raise ValueError("InChI helper build metadata does not match this source and target")
    bridges = metadata.get("bridge_sources", {})
    for relative in (
        "tools/inchi-helper/main.cpp",
        "tools/inchi-helper/arena.cpp",
        "tools/inchi-helper/arena.h",
        "tools/inchi-helper/allocator_redirect.h",
        "scripts/build_inchi_helper.py",
        "scripts/inchi_source_patch.py",
        "tools/inchi-helper/source-patches.json",
    ):
        if bridges.get(relative) != hashlib.sha256((ROOT / relative).read_bytes()).hexdigest():
            raise ValueError(f"InChI helper bridge source changed: {relative}")
    verify_binary(binary, *release_platform(target))
    return binary, {
        "version": reference["inchi_version"],
        "archive_sha256": reference["archive_sha256"],
        "manifest_sha256": required["manifest_sha256"],
        "build_executable_sha256": required["executable_sha256"],
        "patched_source_hashes": required["patched_source_hashes"],
    }


def prepare_inchi_helper(target, *, source=None, archive=None, fetch=False, prebuilt=None):
    if sum((source is not None, archive is not None, fetch, prebuilt is not None)) != 1:
        raise ValueError(
            "Choose --inchi-source, --inchi-archive, --fetch-inchi-source, or --inchi-helper; "
            "release builds never download native source implicitly"
        )
    if prebuilt is not None:
        return verify_inchi_build(prebuilt, target)
    from inchi_source import prepare_source

    build = target_directory()
    source = prepare_source(build / "inchi-sources", source=source, archive=archive, fetch=fetch)
    output = build / target / "inchi-helper"
    run(
        [
            sys.executable,
            ROOT / "scripts/build_inchi_helper.py",
            "--source",
            source,
            "--output",
            output,
            "--target",
            target,
            "--production",
            "--jobs",
            "4",
        ],
        cwd=ROOT,
    )
    return verify_inchi_build(output / inchi_helper_name(release_platform(target)[0]), target)


def verify_inchi_helper(binary, version):
    """Exercise the packaged native protocol with no interpreter or chemistry modules."""
    atom = struct.pack("<ddd6shb4bbB", 0, 0, 0, b"C", 0, 0, -1, 0, 0, 0, 0, 0)
    body = struct.pack("<IHH", 64 * 1024 * 1024, 1, 0) + atom
    request = b"RSHINCHI" + struct.pack("<HBBI", 2, 1, 0, len(body)) + body
    response = run([binary], input=request, capture_output=True, timeout=15).stdout
    if len(response) > 8 * 1024 * 1024 or response[:10] != b"RSHINCHI\x02\x00":
        raise ValueError("Packaged InChI helper returned an incompatible protocol")
    position = 10

    def take(size):
        nonlocal position
        if size > len(response) - position:
            raise ValueError("Packaged InChI helper returned a truncated response")
        data = response[position : position + size]
        position += size
        return data

    def string():
        size = struct.unpack("<I", take(4))[0]
        return take(size).decode("utf-8")

    if string() != version or struct.unpack("<H", take(2))[0] != 0:
        raise ValueError("Packaged InChI helper returned the wrong version or response kind")
    if struct.unpack("<h", take(2))[0] != 0 or string() != "InChI=1S/CH4/h1H4":
        raise ValueError("Packaged InChI helper did not generate methane")
    string()  # message
    string()  # log
    if not string().startswith("AuxInfo=") or position != len(response):
        raise ValueError("Packaged InChI helper returned invalid auxiliary data")


def verify_interpreter(python, system, architecture, environment=None):
    """Inspect the running Python, since uv's launcher may use a different CPU."""
    response = run(
        [
            python,
            "-c",
            "import json, platform, struct, sysconfig; "
            "print(json.dumps({'system': platform.system(), 'machine': platform.machine(), "
            "'platform': sysconfig.get_platform(), 'bits': struct.calcsize('P') * 8}))",
        ],
        env=environment,
        capture_output=True,
        text=True,
        timeout=30,
    )
    details = json.loads(response.stdout)
    actual_system = {"Darwin": "macos", "Windows": "windows", "Linux": "linux"}.get(
        details.get("system")
    )
    if actual_system == "windows":
        # platform.machine() can describe the ARM host under x64 emulation.
        actual_arch = {"win-amd64": "x64", "win-arm64": "arm64"}.get(details.get("platform"))
    else:
        actual_arch = {"arm64": "arm64", "aarch64": "arm64", "x86_64": "x64"}.get(
            details.get("machine")
        )
    if actual_system != system or actual_arch != architecture or details.get("bits") != 64:
        raise ValueError(f"Expected {system} {architecture} Python interpreter, found {details}")


def runtime_project():
    """Package source and locked dependencies; users provide uv, not Python."""
    destination = ROOT / "target/runtime-project"
    if destination.exists():
        shutil.rmtree(destination)
    destination.mkdir(parents=True)
    for name in ["pyproject.toml", "uv.lock"]:
        shutil.copy2(ROOT / name, destination / name)
    shutil.copytree(
        ROOT / "engine",
        destination / "engine",
        ignore=shutil.ignore_patterns("__pycache__", "*.pyc"),
    )
    return destination


def notices(destination):
    destination.mkdir(parents=True, exist_ok=True)
    shutil.copytree(ROOT / "licenses", destination / "sources", dirs_exist_ok=True)
    with (destination / "rust-dependencies.json").open("w") as stream:
        run(["cargo", "metadata", "--format-version", "1", "--locked"], cwd=ROOT, stdout=stream)
    metadata = json.loads((destination / "rust-dependencies.json").read_text(encoding="utf-8"))
    for package in metadata["packages"]:
        source = Path(package["manifest_path"]).parent
        for candidate in source.iterdir():
            if candidate.is_file() and candidate.name.upper().startswith(
                ("LICENSE", "COPYING", "NOTICE")
            ):
                folder = destination / "rust" / f"{package['name']}-{package['version']}"
                folder.mkdir(parents=True, exist_ok=True)
                shutil.copy2(candidate, folder / candidate.name)


def mac_bundle(destination, profile, worker=None, target=None, inchi_helper=None):
    executable = destination / "Contents/MacOS/reshiki"
    executable.parent.mkdir(parents=True, exist_ok=True)
    staged = executable.with_suffix(".new")
    build = target_directory()
    if target:
        build /= target
    shutil.copy2(build / profile / "reshiki", staged)
    staged.replace(executable)
    if inchi_helper is not None:
        shutil.copy2(inchi_helper, executable.with_name(inchi_helper_name("macos")))
    print_app = destination / "Contents/Helpers/ReShiki Print.app"
    helpers = [
        ("Clipboard", executable.with_name("reshiki-clipboard")),
        ("Print", print_app / "Contents/MacOS/reshiki-print"),
    ]
    for name, binary in helpers:
        binary.parent.mkdir(parents=True, exist_ok=True)
        staged = binary.with_suffix(".new")
        run(
            [
                "swiftc",
                "-O",
                ROOT / f"native/macos/{name}Support.swift",
                ROOT / f"native/macos/{name}.swift",
                "-o",
                staged,
            ]
        )
        staged.replace(binary)
    for bundle, name, identifier, binary in [
        (destination, "ReShiki", "dev.reshiki.editor", "reshiki"),
        (print_app, "ReShiki Print", "dev.reshiki.print", "reshiki-print"),
    ]:
        info = dict(
            CFBundleName=name,
            CFBundleDisplayName=name,
            CFBundleIdentifier=identifier,
            CFBundleExecutable=binary,
            CFBundlePackageType="APPL",
            CFBundleShortVersionString=version().split("-")[0],
            CFBundleVersion=version().split("-")[0],
            NSHighResolutionCapable=True,
            LSMinimumSystemVersion=os.environ.get("MACOSX_DEPLOYMENT_TARGET", "14.0"),
        )
        if bundle == print_app:
            info["LSUIElement"] = True
        with (bundle / "Contents/Info.plist").open("wb") as stream:
            plistlib.dump(info, stream)
    if worker:
        chemistry = destination / "Contents/Resources/chemistry"
        if chemistry.exists():
            shutil.rmtree(chemistry)
        shutil.copytree(worker, chemistry, symlinks=True)
        notices(destination / "Contents/Resources/Licenses")
        # Development/manual unsigned builds. Release signing replaces these signatures.
        run(["codesign", "--force", "--deep", "--sign", "-", destination])
    return destination


def archive(folder, output):
    output.parent.mkdir(parents=True, exist_ok=True)
    if platform.system() == "Darwin":
        output = Path(str(output) + ".zip")
        run(["ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", folder, output])
    elif platform.system() == "Windows":
        output = Path(str(output) + ".zip")
        # Some wheel files have reproducible 1970 timestamps; ZIP starts in 1980.
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
            binary = folder / ("reshiki.exe" if os.name == "nt" else "reshiki")
        verify_binary(binary, metadata["platform"], metadata["architecture"])
        helper = binary.with_name(inchi_helper_name(metadata["platform"]))
        verify_binary(helper, metadata["platform"], metadata["architecture"])
        verify_inchi_helper(helper, metadata["inchi_helper"]["version"])
        environment = dict(os.environ)
        environment.pop("RESHIKI_PYTHON", None)
        environment.pop("MORUNO_PYTHON", None)
        environment.pop("RESHIKI_INCHI_HELPER", None)
        environment["RESHIKI_RUNTIME_DIR"] = str(extracted / "user runtime")
        environment["RESHIKI_ROOT"] = str(extracted / "no-checkout")
        missing_uv = subprocess.run(
            [str(binary), "--engine-check"],
            cwd=extracted,
            env={**environment, "RESHIKI_UV": str(extracted / "missing-uv")},
            capture_output=True,
            text=True,
            timeout=30,
        )
        if missing_uv.returncode == 0 or "Install uv" not in missing_uv.stderr:
            raise ValueError("Missing-uv setup instructions were not reported")
        try:
            response = run(
                [binary, "--engine-check"],
                cwd=extracted,
                env=environment,
                capture_output=True,
                text=True,
                timeout=660,
            )
        except subprocess.CalledProcessError as error:
            raise RuntimeError(
                f"Packaged engine check failed:\n{error.stdout}\n{error.stderr}"
            ) from error
        result = json.loads(response.stdout)
        if (
            result.get("analysis", {}).get("formula") != "C2H6O"
            or result.get("analysis", {}).get("smiles") != "CCO"
        ):
            raise ValueError("Packaged chemistry engine did not return ethanol")
        python_name = "Scripts/python.exe" if os.name == "nt" else "bin/python"
        interpreters = list((extracted / "user runtime").glob(f"*/{python_name}"))
        if len(interpreters) != 1:
            raise ValueError("Expected one locally installed chemistry interpreter")
        verify_interpreter(
            interpreters[0],
            metadata["platform"],
            chemistry_architecture(metadata["platform"], metadata["architecture"]),
            environment,
        )
        # Reuse exactly this environment with network disabled on the next launch.
        environment["UV_OFFLINE"] = "1"
        offline = run(
            [binary, "--engine-check"],
            cwd=extracted,
            env=environment,
            capture_output=True,
            text=True,
            timeout=120,
        )
        if json.loads(offline.stdout).get("analysis", {}).get("smiles") != "CCO":
            raise ValueError("Offline chemistry environment reuse failed")
        if platform.system() == "Darwin":
            run(["codesign", "--verify", "--deep", "--strict", app])
        print(
            "Extracted application, missing-uv guidance, first-use setup, and offline reuse verified."
        )


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
    native = parser.add_mutually_exclusive_group()
    native.add_argument(
        "--inchi-source", type=Path, help="Audited local official InChI source tree"
    )
    native.add_argument("--inchi-archive", type=Path, help="Pinned official InChI source ZIP")
    native.add_argument(
        "--fetch-inchi-source",
        action="store_true",
        help="Fetch the pinned official source explicitly",
    )
    native.add_argument(
        "--inchi-helper", type=Path, help="Already built helper with matching build.json"
    )
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
    helper, helper_metadata = prepare_inchi_helper(
        target,
        source=args.inchi_source,
        archive=args.inchi_archive,
        fetch=args.fetch_inchi_source,
        prebuilt=args.inchi_helper,
    )
    name = f"reshiki-{version()}-{system}-{arch}"
    # This staging tree is separate from the app a developer may have open in dist/.
    folder = ROOT / "target/release-bundles" / name
    if folder.exists():
        shutil.rmtree(folder)
    folder.mkdir(parents=True)
    run(["cargo", "build", "--release", "--locked", "--target", target], cwd=ROOT)
    build = target_directory() / target / "release"
    binary_name = "reshiki.exe" if system == "windows" else "reshiki"
    verify_binary(build / binary_name, system, arch)
    worker = runtime_project()
    if system == "macos":
        app = mac_bundle(
            folder / "ReShiki.app", "release", worker, target=target, inchi_helper=helper
        )
        if args.sign:
            from sign_macos import sign_and_notarize

            sign_and_notarize(app)
    else:
        shutil.copy2(build / binary_name, folder / binary_name)
        shutil.copy2(helper, folder / inchi_helper_name(system))
        shutil.copytree(worker, folder / "chemistry", symlinks=True)
        notices(folder / "Licenses")
    metadata = dict(
        version=version(),
        platform=system,
        architecture=arch,
        rust_target=target,
        inchi_helper=helper_metadata,
        chemistry_architecture=chemistry_architecture(system, arch),
        signed=args.sign,
        notarized=args.sign,
        commit=os.environ.get("GITHUB_SHA", "local"),
    )
    (folder / "build.json").write_text(json.dumps(metadata, indent=2) + "\n")
    (folder / "README.txt").write_text(
        "ReShiki — molecular drawing workspace\n\n"
        "Install uv first: https://docs.astral.sh/uv/getting-started/installation/\n"
        "ReShiki installs Python and RDKit into its local user environment on first use.\n"
        "First setup requires internet access; later use works offline.\n"
        "Keep the entire extracted folder together.\n"
        "Documentation: https://reshiki.com/\n"
        + (
            "Windows 11 on ARM is required. The app is native ARM64; its local chemistry\n"
            "worker uses Windows' built-in x64 emulation.\n"
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
