"""Build direct Windows x64 depiction references in an isolated artifact directory.

Run with the pinned RDKit wheel's Python from an x64 MSVC developer prompt.
Supply the pinned RDKit source tree and matching Boost 1.85 include directory.
No SDK installation, Rust, coordinate alignment, or system setting is used.
"""

import argparse
import ctypes
import gzip
import hashlib
import json
import os
import platform
import re
import subprocess
import sys
from pathlib import Path

import rdkit
from rdkit import rdBase

PIN = "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985"
COMPONENTS = ("geometry", "rings", "attachment", "seeds", "templates")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def seed_adapter(source, directory):
    """Windows does not export these four DepictorLocal functions."""
    path = source / "GraphMol/Depictor/RDDepictor.cpp"
    original = path.read_bytes()
    start = original.index(b"constexpr auto ISQRT2 = ")
    end = original.index(b"void embedNontetrahedralStereo(", start)
    body = original[start:end]
    prefix = (
        b"#include <GraphMol/RWMol.h>\n"
        b"#include <GraphMol/Chirality.h>\n"
        b"#include <GraphMol/Depictor/DepictUtils.h>\n"
        b"#include <GraphMol/Depictor/EmbeddedFrag.h>\n"
        b"#include <algorithm>\n"
        b"namespace RDDepict { namespace DepictorLocal {\n"
    )
    output = directory / "seed_source.cpp"
    output.write_bytes(prefix + body + b"\n}}\n")
    return output, {
        "source": "Code/GraphMol/Depictor/RDDepictor.cpp",
        "source_sha256": digest(path),
        "byte_range": [start, end],
        "verbatim_body_sha256": hashlib.sha256(body).hexdigest(),
        "generated_sha256": digest(output),
        "functions": ["getRankedAtomNeighbors", "embedSquarePlanar", "embedTBP", "embedOctahedral"],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rdkit-source", required=True, type=Path)
    parser.add_argument("--boost-include", required=True, type=Path)
    parser.add_argument("--component", choices=COMPONENTS, required=True)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--fma3", "--geometry-fma3", dest="fma3", choices=("0", "1"))
    args = parser.parse_args()
    if sys.platform != "win32" or platform.machine().upper() != "AMD64":
        raise SystemExit("Use the pinned x64 Windows RDKit Python and x64 MSVC compiler")
    assert rdBase.rdkitVersion == "2026.03.6"
    boost_header = args.boost_include / "boost/version.hpp"
    match = re.search(r"#define BOOST_VERSION\s+(\d+)", boost_header.read_text())
    assert match is not None
    version = int(match[1])
    assert f"{version // 100000}_{version // 100 % 1000}" == rdBase.boostVersion, (
        "Boost headers must match the wheel's graph iterator ABI",
        version,
        rdBase.boostVersion,
    )
    root = Path(__file__).resolve().parents[1]
    directory = root / "artifacts" / f"depict-windows-{args.component}"
    directory.mkdir(parents=True, exist_ok=True)
    source = args.rdkit_source.resolve() / "Code"
    fixture = root / f"tests/fixtures/depict-{args.component}-linux-native.json.gz"
    original = gzip.decompress(fixture.read_bytes())
    header = json.loads(original.splitlines()[0])["provenance"]
    assert header["commit"] == PIN
    for name, expected in header["source_sha256"].items():
        assert digest(args.rdkit_source / name) == expected, name
    generated = directory / "generated/RDGeneral"
    generated.mkdir(parents=True, exist_ok=True)
    macros = set()
    for path in source.rglob("*.h"):
        macros.update(re.findall(r"\bRDKIT_[A-Z0-9_]+_EXPORT\b", path.read_text(errors="replace")))
    (generated / "export.h").write_bytes(
        (
            "#pragma once\n"
            + "".join(f"#define {m} __declspec(dllimport)\n" for m in sorted(macros))
        ).encode()
    )
    (generated / "RDConfig.h").write_bytes(b"#pragma once\n")
    includes = []
    if args.component == "templates":
        from build_depict_templates_oracle import prepare_observation

        prepare_observation(root, source, directory / "observation")
        includes.append("/I" + str(directory / "observation"))
    elif args.component in ("attachment", "seeds"):
        data = (source / "GraphMol/Depictor/EmbeddedFrag.h").read_bytes()
        assert data.count(b" private:") == 1
        observer = directory / "observation/GraphMol/Depictor/EmbeddedFrag.h"
        observer.parent.mkdir(parents=True, exist_ok=True)
        observer.write_bytes(data.replace(b" private:", b" public:"))
        includes.append("/I" + str(directory / "observation"))
    library_dir = Path(rdkit.__file__).parent.parent / "rdkit.libs"
    libraries = {}
    links = []
    names = ["Depictor", "GraphMol", "RDGeometryLib", "RDGeneral"]
    if args.component == "templates":
        names.extend(("SubstructMatch", "SmilesParse"))
    for name in names:
        matches = list(library_dir.glob(f"RDKit{name}-*.dll"))
        assert len(matches) == 1, (name, matches)
        dll = matches[0]
        libraries[dll.name] = digest(dll)
        listing = subprocess.check_output(["dumpbin", "/nologo", "/exports", str(dll)], text=True)
        exports = re.findall(r"^\s*\d+\s+[0-9A-Fa-f]+\s+[0-9A-Fa-f]+\s+(\S+)", listing, re.M)
        assert exports
        definition = directory / f"{name}.def"
        definition.write_bytes(
            (f"LIBRARY {dll.name}\nEXPORTS\n" + "\n".join(exports) + "\n").encode()
        )
        link = directory / f"{name}.lib"
        subprocess.run(
            ["lib", "/nologo", "/machine:x64", f"/def:{definition}", f"/out:{link}"],
            check=True,
            cwd=directory,
        )
        links.append(str(link))
    runtime = root / "tests/depict_windows_runtime.cpp"
    additional = [str(runtime)]
    adapter = None
    if args.component == "seeds":
        path, adapter = seed_adapter(source, directory)
        additional.append(str(path))
    binary = directory / "oracle.exe"
    reference = root / f"tests/depict_{args.component}_reference.cpp"
    command = [
        "cl",
        "/nologo",
        "/Bv",
        "/std:c++20",
        "/O2",
        "/MD",
        "/EHsc",
        "/DWIN32",
        "/DBOOST_ALL_NO_LIB",
        "/utf-8",
        *includes,
        "/I" + str(generated.parent),
        "/I" + str(source),
        "/I" + str(source / "GraphMol/Depictor"),
        "/I" + str(args.boost_include.resolve()),
        str(reference),
        *additional,
        *links,
        "/Fe:" + str(binary),
        "/Fo" + str(directory) + "\\",
    ]
    build = subprocess.run(command, cwd=directory, capture_output=True)
    (directory / "compiler.log").write_bytes(build.stdout + build.stderr)
    build.check_returncode()
    output = (
        args.output or root / f"tests/fixtures/depict-{args.component}-windows-native.json.gz"
    ).resolve()
    generate = [
        sys.executable,
        str(root / f"tests/depict_{args.component}_reference.py"),
        "--oracle",
        str(binary),
        "--rdkit-source",
        str(args.rdkit_source.resolve()),
        "--fixture",
        str(fixture),
        "--replay",
        "--output",
        str(output),
    ]
    if args.component == "geometry":
        generate.append("--write-fixture")
    previous = ctypes.windll.kernel32.SetErrorMode(0x0001 | 0x0002 | 0x8000)
    environment = {**os.environ, "PATH": str(library_dir) + os.pathsep + os.environ["PATH"]}
    environment.pop("RESHIKI_REFERENCE_FMA3", None)
    if args.fma3 is not None:
        environment["RESHIKI_REFERENCE_FMA3"] = args.fma3
    try:
        subprocess.run(
            generate,
            check=True,
            cwd=root,
            timeout=180,
            env=environment,
        )
    finally:
        ctypes.windll.kernel32.SetErrorMode(previous)
    first, _, body = gzip.decompress(output.read_bytes()).partition(b"\n")
    metadata = json.loads(first)
    metadata["provenance"]["library_sha256"] = libraries
    metadata["provenance"]["native_build"] = {
        "compiler_command": command,
        "compiler_log_sha256": digest(directory / "compiler.log"),
        "boost_version": rdBase.boostVersion,
        "boost_version_header_sha256": digest(boost_header),
        "reference_sha256": digest(reference),
        "export_header_sha256": digest(generated / "export.h"),
        "source_adapter": adapter,
        "observation_sha256": {
            p.relative_to(directory / "observation").as_posix(): digest(p)
            for p in sorted((directory / "observation").rglob("*"))
            if p.is_file()
        },
        "runtime": "/MD, x64 MSVC and the pinned x64 wheel",
        "geometry_fma3": args.fma3 if args.component == "geometry" else None,
        "fma3": args.fma3,
        "runtime_initializer_sha256": digest(runtime),
        "ucrt_sha256": digest(Path(os.environ["SYSTEMROOT"]) / "System32/ucrtbase.dll"),
    }
    data = json.dumps(metadata, separators=(",", ":")).encode() + b"\n" + body
    output.write_bytes(gzip.compress(data, mtime=0))
    print(f"Recorded {output.name}: {digest(output)}")


if __name__ == "__main__":
    main()
