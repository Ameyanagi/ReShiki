"""Build and verify a portable native Moruno distribution, including RDKit."""

import argparse
import hashlib
import json
import os
import platform
import plistlib
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def run(command, **kwargs):
    return subprocess.run([str(value) for value in command], check=True, **kwargs)


def version():
    return tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]


def check_tag(tag):
    if tag != f"v{version()}":
        raise ValueError(f"Tag {tag!r} must match Cargo.toml version v{version()}")


def freeze_worker():
    work = ROOT / "target/pyinstaller"
    work.mkdir(parents=True, exist_ok=True)
    run(
        [
            "uv",
            "run",
            "--locked",
            "--group",
            "packaging",
            "pyinstaller",
            "--noconfirm",
            "--clean",
            "--onedir",
            "--name",
            "moruno-engine",
            "--paths",
            ROOT / "engine",
            "--collect-all",
            "rdkit",
            "--collect-all",
            "numpy",
            "--add-data",
            str(ROOT / "engine/drawing_style.json") + os.pathsep + ".",
            "--distpath",
            work / "dist",
            "--workpath",
            work / "build",
            "--specpath",
            work,
            ROOT / "engine/worker.py",
        ],
        cwd=ROOT,
    )
    return work / "dist/moruno-engine"


def notices(destination):
    destination.mkdir(parents=True, exist_ok=True)
    with (destination / "rust-dependencies.json").open("w") as stream:
        run(["cargo", "metadata", "--format-version", "1", "--locked"], cwd=ROOT, stdout=stream)
    metadata = json.loads((destination / "rust-dependencies.json").read_text())
    for package in metadata["packages"]:
        source = Path(package["manifest_path"]).parent
        for candidate in source.iterdir():
            if candidate.is_file() and candidate.name.upper().startswith(
                ("LICENSE", "COPYING", "NOTICE")
            ):
                folder = destination / "rust" / f"{package['name']}-{package['version']}"
                folder.mkdir(parents=True, exist_ok=True)
                shutil.copy2(candidate, folder / candidate.name)
    # Query the locked environment; layout differs between Windows and Unix.
    result = run(
        [
            "uv",
            "run",
            "--locked",
            "python",
            "-c",
            "import sysconfig; print(sysconfig.get_path('purelib'))",
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    for entry in Path(result.stdout.strip()).glob("*.dist-info"):
        shutil.copytree(entry, destination / "python" / entry.name, dirs_exist_ok=True)


def mac_bundle(destination, profile, worker=None):
    executable = destination / "Contents/MacOS/moruno"
    executable.parent.mkdir(parents=True, exist_ok=True)
    staged = executable.with_suffix(".new")
    shutil.copy2(ROOT / "target" / profile / "moruno", staged)
    staged.replace(executable)
    print_app = destination / "Contents/Helpers/Moruno Print.app"
    helpers = [
        ("Clipboard", executable.with_name("moruno-clipboard")),
        ("Print", print_app / "Contents/MacOS/moruno-print"),
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
        (destination, "Moruno", "dev.moruno.editor", "moruno"),
        (print_app, "Moruno Print", "dev.moruno.print", "moruno-print"),
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
        output = Path(shutil.make_archive(str(output), "zip", folder.parent, folder.name))
    else:
        output = Path(str(output) + ".tar.gz")
        with tarfile.open(output, "w:gz") as stream:
            stream.add(folder, arcname=folder.name)
    return output


def verify_archive(archive_path, signed=False):
    with tempfile.TemporaryDirectory(prefix="Moruno package check ") as temporary:
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
        if platform.system() == "Darwin":
            app = folder / "Moruno.app"
            binary = app / "Contents/MacOS/moruno"
            run(["codesign", "--verify", "--deep", "--strict", app])
            if signed:
                from sign_macos import verify_app

                verify_app(app)
        else:
            binary = folder / ("moruno.exe" if os.name == "nt" else "moruno")
        environment = dict(os.environ)
        environment.pop("MORUNO_PYTHON", None)
        environment["MORUNO_ROOT"] = str(extracted / "no-checkout")
        try:
            response = run(
                [binary, "--engine-check"],
                cwd=extracted,
                env=environment,
                capture_output=True,
                text=True,
                timeout=120,
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
        print("Extracted application and bundled chemistry engine verified.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag")
    parser.add_argument("--sign", action="store_true")
    parser.add_argument("--check-tag-only", action="store_true")
    args = parser.parse_args()
    if args.tag:
        check_tag(args.tag)
    if args.check_tag_only:
        return
    if args.sign and platform.system() != "Darwin":
        raise ValueError("Developer ID signing requires macOS")
    system = {"Darwin": "macos", "Windows": "windows", "Linux": "linux"}[platform.system()]
    arch = "arm64" if platform.machine().lower() in {"arm64", "aarch64"} else "x64"
    name = f"moruno-{version()}-{system}-{arch}"
    # This staging tree is separate from the app a developer may have open in dist/.
    folder = ROOT / "target/release-bundles" / name
    if folder.exists():
        shutil.rmtree(folder)
    folder.mkdir(parents=True)
    run(["cargo", "build", "--release", "--locked"], cwd=ROOT)
    worker = freeze_worker()
    if system == "macos":
        app = mac_bundle(folder / "Moruno.app", "release", worker)
        if args.sign:
            from sign_macos import sign_and_notarize

            sign_and_notarize(app)
    else:
        binary = "moruno.exe" if os.name == "nt" else "moruno"
        shutil.copy2(ROOT / "target/release" / binary, folder / binary)
        shutil.copytree(worker, folder / "chemistry", symlinks=True)
        notices(folder / "Licenses")
    metadata = dict(
        version=version(),
        platform=system,
        architecture=arch,
        signed=args.sign,
        notarized=args.sign,
        commit=os.environ.get("GITHUB_SHA", "local"),
    )
    (folder / "build.json").write_text(json.dumps(metadata, indent=2) + "\n")
    (folder / "README.txt").write_text(
        "Moruno — molecular drawing workspace\n\n"
        "Keep the entire extracted folder together. Python and RDKit are included.\n"
        "Documentation: https://ameyanagi.github.io/moruno/\n"
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
    Path(str(output) + ".sha256").write_text(f"{digest}  {output.name}\n")
    print(output)


if __name__ == "__main__":
    main()
