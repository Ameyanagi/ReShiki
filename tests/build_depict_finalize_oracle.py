"""Build an independent pinned-native finalization observer in isolated artifacts.

Use matching Boost 1.85 headers. Windows requires the x64 MSVC developer prompt
and pinned x64 Python; all compilers run sequentially. No Rust code is linked.
"""

import argparse
import ctypes
import hashlib
import json
import os
import re
import subprocess
import sys
import sysconfig
from pathlib import Path

import rdkit
from rdkit import rdBase

PIN = "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rdkit-source", type=Path, required=True)
    parser.add_argument("--boost-include", type=Path, required=True)
    parser.add_argument("--fixture", type=Path)
    parser.add_argument("--replay", action="store_true")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--fma3", choices=("0", "1"))
    args = parser.parse_args()
    if args.fma3 is not None and sys.platform != "win32":
        parser.error("--fma3 requires the Windows native observer")
    assert rdBase.rdkitVersion == "2026.03.6"
    boost = args.boost_include.resolve()
    match = re.search(r"#define BOOST_VERSION\s+(\d+)", (boost / "boost/version.hpp").read_text())
    assert match is not None
    version = int(match[1])
    assert f"{version // 100000}_{version // 100 % 1000}" == rdBase.boostVersion
    root = Path(__file__).resolve().parents[1]
    directory = root / "artifacts/depict-finalize-oracle"
    directory.mkdir(parents=True, exist_ok=True)
    source = args.rdkit_source.resolve() / "Code"
    # The source hashes in the independent replay fixture are checked by the
    # reference script; a new corpus also requires the exact git commit.
    native = (source / "GraphMol/Depictor/RDDepictor.cpp").read_bytes()
    assert (
        digest(source / "GraphMol/Depictor/RDDepictor.cpp")
        == "f1ae9e705405d5254575c595c09c5d2ee240d4d2abf05209dd42dad5ae99b31f"
    )
    extracted = []

    def extract(start, end):
        a = native.index(start)
        b = native.index(end, a)
        body = native[a:b]
        extracted.append(dict(byte_range=[a, b], sha256=hashlib.sha256(body).hexdigest()))
        return body

    adapter = b"#include <GraphMol/RWMol.h>\n#include <GraphMol/Conformer.h>\n#include <GraphMol/Depictor/EmbeddedFrag.h>\n"
    windows = sys.platform == "win32"
    if windows:
        assert sysconfig.get_platform() == "win-amd64"
        adapter += b"namespace RDDepict { namespace DepictorLocal {\n"
        adapter += extract(b"void _shiftCoords(", b"// we do not use std::copysign")
        adapter += b"}\n"
        adapter += extract(b"unsigned int copyCoordinate(", b"void setRingSystemTemplates(")
        adapter += b"}\n"
    adapter += b"void singleCoordinate(RDKit::ROMol &mol, unsigned int cid, const RDGeom::INT_POINT2D_MAP *coordinates) {\nstruct {const RDGeom::INT_POINT2D_MAP *coordMap;} params{coordinates};\n"
    adapter += extract(b"  // special case for a single-atom coordMap template", b"  return cid;")
    adapter += b"}\n"
    adapted = directory / "source_adapter.cpp"
    adapted.write_bytes(adapter)
    include = directory / "generated/RDGeneral"
    include.mkdir(parents=True, exist_ok=True)
    macros = set()
    for path in source.rglob("*.h"):
        macros.update(re.findall(r"\bRDKIT_[A-Z0-9_]+_EXPORT\b", path.read_text(errors="replace")))
    export = "__declspec(dllimport)" if windows else ""
    (include / "export.h").write_bytes(
        ("#pragma once\n" + "".join(f"#define {m} {export}\n" for m in sorted(macros))).encode()
    )
    (include / "RDConfig.h").write_bytes(b"#pragma once\n")
    original = (source / "GraphMol/Depictor/EmbeddedFrag.h").read_bytes()
    assert original.count(b" private:") == 1
    observed = directory / "observation/GraphMol/Depictor/EmbeddedFrag.h"
    observed.parent.mkdir(parents=True, exist_ok=True)
    observed.write_bytes(original.replace(b" private:", b" public:"))
    includes = [
        directory / "observation",
        include.parent,
        source,
        source / "GraphMol/Depictor",
        boost,
    ]
    package = Path(rdkit.__file__).parent
    libraries = package / ".dylibs" if sys.platform == "darwin" else package.parent / "rdkit.libs"
    links = []
    names = ("Depictor", "GraphMol", "RDGeometryLib", "RDGeneral")
    binary = directory / ("oracle.exe" if windows else "oracle")
    cpp = root / "tests/depict_finalize_reference.cpp"
    if windows:
        for name in names:
            files = list(libraries.glob(f"RDKit{name}-*.dll"))
            assert len(files) == 1
            dll = files[0]
            listing = subprocess.check_output(
                ["dumpbin", "/nologo", "/exports", str(dll)], text=True
            )
            exports = re.findall(r"^\s*\d+\s+[0-9A-Fa-f]+\s+[0-9A-Fa-f]+\s+(\S+)", listing, re.M)
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
            *("/I" + str(i) for i in includes),
            str(cpp),
            str(adapted),
            str(root / "tests/depict_windows_runtime.cpp"),
            *links,
            "/Fe:" + str(binary),
            "/Fo" + str(directory) + "\\",
        ]
        env = {
            **os.environ,
            "PATH": str(libraries) + os.pathsep + os.environ["PATH"],
            "VSLANG": "1033",
        }
        env.pop("RESHIKI_REFERENCE_FMA3", None)
        if args.fma3 is not None:
            env["RESHIKI_REFERENCE_FMA3"] = args.fma3
    else:
        python = Path(sysconfig.get_config_var("LIBDIR")) / sysconfig.get_config_var("LDLIBRARY")
        for name in names:
            files = list(
                libraries.glob(
                    f"libRDKit{name}.1.dylib"
                    if sys.platform == "darwin"
                    else f"libRDKit{name}-*.so*"
                )
            )
            assert len(files) == 1
            links += list(map(str, files))
        links += [str(python), "-Wl,-rpath," + str(python.parent)]
        if sys.platform != "darwin":
            links += ["-Wl,-rpath," + str(libraries), "-Wl,-rpath-link," + str(libraries)]
        command = [
            os.environ.get("CXX", "c++"),
            "-std=c++20",
            *("-I" + str(i) for i in includes),
            str(cpp),
            str(adapted),
            *links,
            "-o",
            str(binary),
        ]
        env = {
            **os.environ,
            "DYLD_LIBRARY_PATH" if sys.platform == "darwin" else "LD_LIBRARY_PATH": str(libraries),
        }
    build = subprocess.run(command, capture_output=True, cwd=directory, env=env)
    (directory / "compiler.log").write_bytes(build.stdout + build.stderr)
    build.check_returncode()
    provenance = dict(
        compiler_command=command,
        compiler_log_sha256=digest(directory / "compiler.log"),
        source_adapter_sha256=digest(adapted),
        verbatim_ranges=extracted,
        observation_header_sha256=digest(observed),
        boost_version=rdBase.boostVersion,
        boost_header_sha256=digest(boost / "boost/version.hpp"),
        reference_sha256=digest(cpp),
    )
    if windows:
        provenance.update(
            fma3=args.fma3,
            runtime_initializer_sha256=digest(root / "tests/depict_windows_runtime.cpp"),
            ucrt_sha256=digest(Path(os.environ["SYSTEMROOT"]) / "System32/ucrtbase.dll"),
        )
    metadata = directory / "build.json"
    metadata.write_text(json.dumps(provenance, indent=2) + "\n")
    generate = [
        sys.executable,
        str(root / "tests/depict_finalize_reference.py"),
        "--oracle",
        str(binary),
        "--rdkit-source",
        str(args.rdkit_source.resolve()),
        "--output",
        str(args.output.resolve()),
        "--build-provenance",
        str(metadata),
    ]
    if args.replay:
        generate.append("--replay")
    if args.fixture:
        generate += ["--fixture", str(args.fixture.resolve())]
    if windows:
        previous = ctypes.windll.kernel32.SetErrorMode(0x0001 | 0x0002 | 0x8000)
        try:
            subprocess.run(generate, check=True, timeout=180, cwd=root, env=env)
        finally:
            ctypes.windll.kernel32.SetErrorMode(previous)
    else:
        subprocess.run(generate, check=True, timeout=180, cwd=root, env=env)


if __name__ == "__main__":
    main()
