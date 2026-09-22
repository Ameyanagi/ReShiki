"""Build a development helper from the verified official InChI 1.07.3 source.

The resulting helper has no Python or RDKit dependency. Source remains outside
the repository; this script does not change application or release packaging.
"""

import argparse
import concurrent.futures
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--jobs", type=int, default=4, choices=range(1, 5))
    parser.add_argument("--compiler-style", choices=("auto", "unix", "msvc"), default="auto")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    manifest = json.loads((root / "tools/inchi-helper/source-manifest.json").read_text())
    for relative, expected in manifest["files"].items():
        path = args.source / relative
        actual = hashlib.sha256(path.read_bytes()).hexdigest()
        if actual != expected:
            raise ValueError(f"Official InChI source changed: {relative}")
    output = root / "artifacts/inchi-helper"
    output.mkdir(parents=True, exist_ok=True)
    windows = sys.platform == "win32"
    cc = os.environ.get(
        "CC", "cl" if windows else "/usr/bin/clang" if sys.platform == "darwin" else "cc"
    )
    cxx = os.environ.get(
        "CXX", "cl" if windows else "/usr/bin/clang++" if sys.platform == "darwin" else "c++"
    )
    msvc = args.compiler_style == "msvc" or (
        args.compiler_style == "auto" and (windows or Path(cc).stem.lower() in ("cl", "clang-cl"))
    )
    flags = (
        [
            "/nologo",
            "/O2",
            "/MT",
            "/DTARGET_API_LIB",
            "/DRDKIT_INCHI_BUILD",
            "/D_CRT_SECURE_NO_WARNINGS",
            "/I" + str(args.source / "INCHI_BASE/src"),
        ]
        if msvc
        else [
            "-O2",
            "-DTARGET_API_LIB",
            "-DRDKIT_INCHI_BUILD",
            "-D__isascii=isascii",
            "-I" + str(args.source / "INCHI_BASE/src"),
        ]
    )
    redirect = root / "tools/inchi-helper/allocator_redirect.h"
    flags += ["/FI" + str(redirect)] if msvc else ["-include", str(redirect)]
    commands = []
    objects = []
    units = manifest["translation_units"]
    names = [Path(relative).stem.casefold() for relative in units]
    names += ["reshiki-arena", "helper-main", "helper-stub", "arena-test"]
    if len(set(units)) != len(units) or len(set(names)) != len(names):
        raise ValueError("Native translation units must have unique object names")
    for relative in units:
        if relative not in manifest["files"]:
            raise ValueError(f"Unverified native translation unit: {relative}")
        path = args.source / relative
        obj = output / (path.stem + (".obj" if msvc else ".o"))
        objects.append(str(obj))
        commands.append(
            [cc, "/std:c11", *flags, "/c", str(path), "/Fo" + str(obj)]
            if msvc
            else [cc, "-std=gnu11", *flags, "-c", str(path), "-o", str(obj)]
        )

    def compile_one(command):
        result = subprocess.run(command, capture_output=True, text=True, errors="replace")
        if result.returncode:
            raise RuntimeError(result.stdout + result.stderr)

    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        list(pool.map(compile_one, commands))
    # The existing inchi_* hooks miss raw allocations. Inspect the exact kernel
    # objects, excluding the arena implementation that intentionally owns malloc.
    symbol_command = ["dumpbin", "/symbols", *objects] if msvc else ["nm", "-u", *objects]
    symbols = subprocess.run(
        symbol_command, check=True, capture_output=True, text=True, errors="replace"
    ).stdout
    forbidden = {
        "malloc",
        "calloc",
        "realloc",
        "free",
        "strdup",
        "strndup",
        "aligned_alloc",
        "aligned_malloc",
        "aligned_realloc",
        "aligned_free",
        "posix_memalign",
        "mmap",
        "VirtualAlloc",
        "HeapAlloc",
        "HeapReAlloc",
        "HeapFree",
        "VirtualAlloc2",
        "malloc_dbg",
        "calloc_dbg",
        "realloc_dbg",
        "free_dbg",
    }
    imported = set()
    for line in symbols.splitlines():
        if not line.strip() or (msvc and "UNDEF" not in line):
            continue
        symbol = line.rsplit("|", 1)[-1].strip() if msvc else line.split()[-1]
        symbol = symbol.removeprefix("__imp_").removeprefix("_")
        imported.add(symbol)
        if symbol in forbidden:
            raise ValueError(f"Kernel allocation bypasses the arena: {symbol}")
    expected = {"rsh_kernel_malloc", "rsh_kernel_calloc", "rsh_kernel_realloc", "rsh_kernel_free"}
    if windows:
        expected.add("rsh_kernel_strdup")
    if not expected.issubset(imported):
        raise ValueError("Kernel allocator redirection is incomplete")
    (output / "kernel-symbols.txt").write_text(symbols)
    arena_object = output / ("reshiki-arena.obj" if msvc else "reshiki-arena.o")
    arena_command = (
        [
            cxx,
            "/nologo",
            "/std:c++20",
            "/O2",
            "/MT",
            "/EHsc",
            "/c",
            str(root / "tools/inchi-helper/arena.cpp"),
            "/Fo" + str(arena_object),
        ]
        if msvc
        else [
            cxx,
            "-std=c++20",
            "-O2",
            "-c",
            str(root / "tools/inchi-helper/arena.cpp"),
            "-o",
            str(arena_object),
        ]
    )
    subprocess.run(arena_command, check=True)
    objects.append(str(arena_object))
    executable = output / (
        "reshiki-inchi-helper.exe" if sys.platform == "win32" else "reshiki-inchi-helper"
    )
    command = (
        [
            cxx,
            "/nologo",
            "/std:c++20",
            "/O2",
            "/MT",
            "/EHsc",
            "/I" + str(args.source / "INCHI_BASE/src"),
            str(root / "tools/inchi-helper/main.cpp"),
            *objects,
            "/Fe" + str(executable),
            "/Fo" + str(output / "helper-main.obj"),
        ]
        if msvc
        else [
            cxx,
            "-std=c++20",
            "-O2",
            "-I" + str(args.source / "INCHI_BASE/src"),
            str(root / "tools/inchi-helper/main.cpp"),
            *objects,
            "-lm",
            "-o",
            str(executable),
        ]
    )
    subprocess.run(command, check=True)
    stub = output / ("inchi-helper-stub.exe" if sys.platform == "win32" else "inchi-helper-stub")
    stub_command = (
        [
            cxx,
            "/nologo",
            "/std:c++20",
            "/O2",
            "/MT",
            "/EHsc",
            str(root / "tests/inchi_helper_stub.cpp"),
            "/Fe" + str(stub),
            "/Fo" + str(output / "helper-stub.obj"),
        ]
        if msvc
        else [
            cxx,
            "-std=c++20",
            "-O2",
            str(root / "tests/inchi_helper_stub.cpp"),
            "-o",
            str(stub),
        ]
    )
    subprocess.run(stub_command, check=True)
    arena_test = output / ("inchi-arena-test.exe" if windows else "inchi-arena-test")
    arena_test_command = (
        [
            cxx,
            "/nologo",
            "/std:c++20",
            "/O2",
            "/MT",
            "/EHsc",
            str(root / "tests/inchi_arena.cpp"),
            str(arena_object),
            "/Fe" + str(arena_test),
            "/Fo" + str(output / "arena-test.obj"),
        ]
        if msvc
        else [
            cxx,
            "-std=c++20",
            "-O2",
            str(root / "tests/inchi_arena.cpp"),
            str(arena_object),
            "-o",
            str(arena_test),
        ]
    )
    subprocess.run(arena_test_command, check=True)
    compiler = subprocess.run(
        [cc] if msvc else [cc, "--version"], capture_output=True, text=True, errors="replace"
    )
    cxx_compiler = subprocess.run(
        [cxx] if msvc else [cxx, "--version"], capture_output=True, text=True, errors="replace"
    )
    build = {
        "inchi_version": manifest["inchi_version"],
        "archive_sha256": manifest["archive_sha256"],
        "helper_sha256": hashlib.sha256(
            (root / "tools/inchi-helper/main.cpp").read_bytes()
        ).hexdigest(),
        "manifest_sha256": hashlib.sha256(
            (root / "tools/inchi-helper/source-manifest.json").read_bytes()
        ).hexdigest(),
        "executable_sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
        "compiler_style": "msvc" if msvc else "unix",
        "compiler": compiler.stdout + compiler.stderr,
        "cxx_compiler": cxx_compiler.stdout + cxx_compiler.stderr,
        "bridge_sources": {
            str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in (
                root / "tools/inchi-helper/arena.cpp",
                root / "tools/inchi-helper/arena.h",
                root / "tools/inchi-helper/main.cpp",
                root / "tests/inchi_helper_stub.cpp",
                root / "tests/inchi_arena.cpp",
                Path(__file__).resolve(),
                redirect,
            )
        },
        "kernel_allocation_audit": {
            "command": symbol_command,
            "redirected_symbols": sorted(expected),
        },
        "commands": [*commands, arena_command, command, stub_command, arena_test_command],
    }
    (output / "build.json").write_text(json.dumps(build, indent=2) + "\n")
    print(executable)


if __name__ == "__main__":
    main()
