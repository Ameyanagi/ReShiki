"""Build fresh x64 depiction observers for the installed Windows reference wheel.

Windows ARM uses the same x64 wheel under emulation. Observers follow that
reference ABI; the application and its Rust tests retain their native target.
Only the hash-checked Boost download may be cached, never observer binaries.
"""

import argparse
import ctypes
import json
import os
import platform
import shutil
import subprocess
import sys
import sysconfig
import tempfile
from pathlib import Path

from setup_linux_depict_reference import (
    COMPONENTS,
    PIN,
    boost_archive,
    checked_source,
    digest,
    extract_headers,
)

ROOT = Path(__file__).resolve().parents[1]


def source_checkout(path):
    if not path.exists():
        path.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="rdkit-fetch-", dir=path.parent) as temporary:
            subprocess.run(["git", "init", "--quiet", temporary], check=True)
            # Windows Git defaults must not change the audited source bytes.
            subprocess.run(["git", "-C", temporary, "config", "core.autocrlf", "false"], check=True)
            subprocess.run(
                [
                    "git",
                    "-C",
                    temporary,
                    "fetch",
                    "--depth=1",
                    "https://github.com/rdkit/rdkit.git",
                    PIN,
                ],
                check=True,
                timeout=300,
            )
            subprocess.run(
                ["git", "-C", temporary, "checkout", "--quiet", "--detach", "FETCH_HEAD"],
                check=True,
            )
            checked_source(Path(temporary))
            Path(temporary).rename(path)
    return checked_source(path)


def build(source, boost, directory, environment):
    common = ["--rdkit-source", str(source), "--boost-include", str(boost)]
    oracles = {}
    for component in COMPONENTS:
        output = directory / f"{component}.json.gz"
        if component in ("geometry", "rings", "attachment", "seeds", "templates"):
            builder = "build_depict_windows_oracle.py"
            extra = ["--component", component, "--output", str(output)]
            binary = ROOT / f"artifacts/depict-windows-{component}/oracle.exe"
        elif component == "expansion":
            builder = "build_depict_expansion_windows_oracle.py"
            extra = ["--output", str(output)]
            binary = ROOT / "artifacts/depict-windows-expansion/oracle.exe"
        elif component == "finalize":
            builder = "build_depict_finalize_oracle.py"
            extra = ["--replay", "--output", str(output)]
            binary = ROOT / "artifacts/depict-finalize-oracle/oracle.exe"
        else:
            builder = "build_depict_collision_oracle.py"
            extra = []
            binary = ROOT / "artifacts/depict-collision-oracle.exe"
        print(f"Building live x64 {component} observer", flush=True)
        subprocess.run(
            [sys.executable, str(ROOT / "tests" / builder), *common, *extra],
            check=True,
            cwd=ROOT,
            env=environment,
            timeout=600,
        )
        if not binary.is_file():
            raise ValueError(f"Observer was not built: {binary}")
        if component == "collision":
            subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "tests/depict_collision_reference.py"),
                    "--oracle",
                    str(binary),
                    "--rdkit-source",
                    str(source),
                    "--fixture",
                    str(ROOT / "tests/fixtures/depict-collision-linux-native.json.gz"),
                    "--replay",
                    "--output",
                    str(output),
                ],
                check=True,
                cwd=ROOT,
                env=environment,
                timeout=600,
            )
        oracles[component] = binary.resolve()
    return oracles


def module_version(path):
    if sys.platform != "win32":
        raise ValueError("CRT version resources require Windows")
    version = ctypes.WinDLL("version", use_last_error=True)
    version.GetFileVersionInfoSizeW.argtypes = [ctypes.c_wchar_p, ctypes.POINTER(ctypes.c_uint32)]
    version.GetFileVersionInfoSizeW.restype = ctypes.c_uint32
    version.GetFileVersionInfoW.argtypes = [
        ctypes.c_wchar_p,
        ctypes.c_uint32,
        ctypes.c_uint32,
        ctypes.c_void_p,
    ]
    version.GetFileVersionInfoW.restype = ctypes.c_int
    version.VerQueryValueW.argtypes = [
        ctypes.c_void_p,
        ctypes.c_wchar_p,
        ctypes.POINTER(ctypes.c_void_p),
        ctypes.POINTER(ctypes.c_uint32),
    ]
    version.VerQueryValueW.restype = ctypes.c_int
    size = version.GetFileVersionInfoSizeW(str(path), None)
    if not 0 < size <= 1024 * 1024:
        raise ValueError("Missing or oversized CRT version resource")
    data = ctypes.create_string_buffer(size)
    if not version.GetFileVersionInfoW(str(path), 0, size, data):
        raise OSError(ctypes.get_last_error(), "Cannot read CRT version resource")
    pointer, length = ctypes.c_void_p(), ctypes.c_uint32()
    if (
        not version.VerQueryValueW(data, "\\", ctypes.byref(pointer), ctypes.byref(length))
        or length.value < 52
    ):
        raise ValueError("Missing CRT fixed version info")
    info = ctypes.cast(pointer, ctypes.POINTER(ctypes.c_uint32 * 13)).contents
    if info[0] != 0xFEEF04BD:
        raise ValueError("Invalid CRT version signature")
    return f"{info[2] >> 16}.{info[2] & 65535}.{info[3] >> 16}.{info[3] & 65535}"


def crt_evidence(directory, environment):
    """Observe Python's loaded CRT and a separately compiled /MD process."""
    python = subprocess.check_output(
        [sys.executable, str(ROOT / "tests/depict_windows_profile.py"), "--observe"],
        env=environment,
        text=True,
    )
    source = ROOT / "tests/depict_windows_crt_reference.cpp"
    binary = directory / "crt-reference.exe"
    command = [
        "cl",
        "/nologo",
        "/std:c++20",
        "/O2",
        "/MD",
        "/EHsc",
        str(source),
        "/Fe:" + str(binary),
        "/Fo" + str(directory) + "\\",
    ]
    compiled = subprocess.run(command, capture_output=True, env=environment, cwd=directory)
    (directory / "crt-compiler.log").write_bytes(compiled.stdout + compiled.stderr)
    compiled.check_returncode()
    reported = dict(
        line.split(": ", 1)
        for line in subprocess.check_output([str(binary)], env=environment, text=True).splitlines()
    )
    native = dict(
        reported,
        primitive_bits=reported["primitive_bits"].split(),
        ring_bits=reported["ring_bits"].split(),
        module_sha256=digest(Path(reported["module"])),
        module_version=module_version(Path(reported["module"])),
        observer_sha256=digest(source),
        binary_sha256=digest(binary),
        compiler_command=command,
        compiler_log_sha256=digest(directory / "crt-compiler.log"),
    )
    python_evidence = json.loads(python)
    python_evidence["module_version"] = module_version(Path(python_evidence["module"]))
    result = {"python": python_evidence, "native": native}
    (directory / "crt-evidence.json").write_text(json.dumps(result, indent=2) + "\n")
    print("Independent CRT witnesses: " + json.dumps(result), flush=True)
    return result


def publish(values, directory, github_env, github_path, libraries):
    if any("\n" in value or "\r" in value for value in values.values()):
        raise ValueError("Reference environment values must fit on one line")
    (directory / "environment.json").write_text(json.dumps(values, indent=2) + "\n")
    library_prefix = (str(libraries) + os.pathsep).replace("'", "''")
    (directory / "environment.ps1").write_text(
        f"$env:PATH = '{library_prefix}' + $env:PATH\n"
        + "".join(
            f"$env:{key} = '{value.replace(chr(39), chr(39) * 2)}'\n"
            for key, value in values.items()
            if key != "PATH"
        )
    )
    if github_env:
        with github_env.open("a", encoding="utf-8") as output:
            output.writelines(f"{key}={value}\n" for key, value in values.items() if key != "PATH")
    if github_path:
        with github_path.open("a", encoding="utf-8") as output:
            output.write(str(libraries) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rdkit-source", type=Path, help="Use a clean pinned Git checkout")
    parser.add_argument("--cache-dir", type=Path, default=ROOT / "artifacts/depict-download-cache")
    parser.add_argument("--github-env", type=Path, default=os.environ.get("GITHUB_ENV"))
    parser.add_argument("--github-path", type=Path, default=os.environ.get("GITHUB_PATH"))
    args = parser.parse_args()
    if sys.platform != "win32" or sysconfig.get_platform() != "win-amd64":
        raise SystemExit("Use the pinned x64 Windows reference interpreter, including on ARM")
    if any(
        os.environ.get(key)
        for key in ("RESHIKI_REFERENCE_FMA3", "RESHIKI_TEST_WINDOWS_MATH_PROFILE")
    ):
        raise ValueError("Live CI observers must use the runtime's original default profile")
    import rdkit
    from rdkit import rdBase

    if (rdBase.rdkitVersion, rdBase.boostVersion) != ("2026.03.6", "1_85"):
        raise ValueError("Install the pinned RDKit 2026.03.6 wheel with Boost 1_85 first")
    if shutil.which("cl") is None or os.environ.get("VSCMD_ARG_TGT_ARCH") != "x64":
        raise ValueError("Initialize the x64 MSVC developer environment for the reference wheel")
    directory = ROOT / "artifacts/depict-live-windows"
    directory.mkdir(parents=True, exist_ok=True)
    source = (
        checked_source(args.rdkit_source)
        if args.rdkit_source
        else source_checkout(directory / "rdkit")
    )
    archive = boost_archive(args.cache_dir.resolve())
    libraries = Path(rdkit.__file__).resolve().parent.parent / "rdkit.libs"
    if not libraries.is_dir():
        raise ValueError("Missing pinned wheel libraries")
    environment = {
        **os.environ,
        "PATH": str(libraries) + os.pathsep + os.environ["PATH"],
        "VSLANG": "1033",
    }
    evidence = crt_evidence(directory, environment)
    with tempfile.TemporaryDirectory(prefix="boost185-", dir=directory) as temporary:
        boost = Path(temporary)
        extract_headers(archive, boost)
        oracles = build(source, boost, directory, environment)
    values = {
        "RESHIKI_RDKIT_SOURCE": str(source),
        "DEPICT_EXPANSION_SOURCE": str(source),
        "PATH": environment["PATH"],
    }
    for component, binary in oracles.items():
        key = (
            "RESHIKI_DEPICT_ORACLE"
            if component == "geometry"
            else "DEPICT_EXPANSION_ORACLE"
            if component == "expansion"
            else f"RESHIKI_DEPICT_{component.upper()}_ORACLE"
        )
        values[key] = str(binary)
    metadata = dict(
        platform=platform.platform(),
        machine=platform.machine(),
        processor=platform.processor(),
        python=sys.version,
        python_platform=sysconfig.get_platform(),
        rdkit=rdBase.rdkitVersion,
        source_commit=PIN,
        boost_archive_sha256=digest(archive),
        crt=evidence,
        oracles={name: digest(path) for name, path in oracles.items()},
        libraries={path.name: digest(path) for path in sorted(libraries.glob("*.dll"))},
        environment=values,
    )
    (directory / "provenance.json").write_text(json.dumps(metadata, indent=2) + "\n")
    publish(values, directory, args.github_env, args.github_path, libraries)
    print(f"All eight live x64 observers are ready: {directory / 'environment.ps1'}")


if __name__ == "__main__":
    main()
