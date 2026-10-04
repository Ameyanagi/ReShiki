"""Build/prove the offline native geometry core without invoking Cargo."""

import argparse
import json
import os
import platform
import shutil
import subprocess
from pathlib import Path

from geometry_source import ROOT, digest, verify


def build(output, target=None, jobs=2, tests=False):
    provenance = verify()
    output = Path(output).resolve()
    system = platform.system()
    if target:
        expected = (
            "Darwin"
            if target.endswith("apple-darwin")
            else "Windows"
            if target.endswith("windows-msvc")
            else "Linux"
            if target.endswith("linux-gnu")
            else None
        )
        if expected != system or not target.startswith(("aarch64-", "x86_64-")):
            raise ValueError("Use a supported target on its matching native operating system")
    configure = [
        os.environ.get("CMAKE", "cmake"),
        "-S",
        str(ROOT / "native/geometry/cpp"),
        "-B",
        str(output),
        "-DCMAKE_BUILD_TYPE=Release",
        "-DRESHIKI_GEOMETRY_BUILD_TESTS=" + ("ON" if tests else "OFF"),
    ]
    if target and system == "Windows":
        generator = os.environ.get("CMAKE_GENERATOR", "")
        if not generator or generator.startswith("Visual Studio"):
            configure.extend(["-A", "ARM64" if target.startswith("aarch64") else "x64"])
        configure.append("-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded")
    if system == "Darwin":
        configure.append(
            "-DCMAKE_OSX_DEPLOYMENT_TARGET=" + os.environ.get("MACOSX_DEPLOYMENT_TARGET", "14.0")
        )
        if target:
            configure.append(
                "-DCMAKE_OSX_ARCHITECTURES="
                + ("arm64" if target.startswith("aarch64") else "x86_64")
            )
    subprocess.run(configure, check=True)
    subprocess.run(
        [configure[0], "--build", str(output), "--config", "Release", "--parallel", str(jobs)],
        check=True,
    )
    if tests:
        subprocess.run(
            [
                os.environ.get("CTEST", "ctest"),
                "--test-dir",
                str(output),
                "-C",
                "Release",
                "--output-on-failure",
                "--timeout",
                "90",
            ],
            check=True,
        )
    candidates = [
        p for p in output.rglob("*") if p.name in {"libreshiki_geometry.a", "reshiki_geometry.lib"}
    ]
    if len(candidates) != 1:
        raise ValueError("Native geometry build did not produce exactly one static core")
    archive = candidates[0]
    cache = (output / "CMakeCache.txt").read_text()
    compiler = next(
        line.split("=", 1)[1]
        for line in cache.splitlines()
        if line.startswith("CMAKE_CXX_COMPILER:")
    )
    result = {
        **provenance,
        "target": target,
        "host": platform.platform(),
        "compiler": compiler,
        "cmake": shutil.which(configure[0]) or configure[0],
        "static_archive": str(archive),
        "static_archive_sha256": digest(archive),
        "external_link_libraries": ["system C++ standard library", "system threads"],
        "runtime_python": False,
        "build_downloads": False,
        "native_tests": tests,
    }
    (output / "build.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--target")
    parser.add_argument("--jobs", type=int, default=2, choices=range(1, 5))
    parser.add_argument("--tests", action="store_true")
    args = parser.parse_args()
    print(json.dumps(build(args.output, args.target, args.jobs, args.tests), indent=2))


if __name__ == "__main__":
    main()
