"""Build an independent C++ observer against the pinned RDKit wheel.

The source tree may be the pinned Git checkout or its provenance-checked header
snapshot. All observation files are generated below this checkout's artifacts.
Private access changes declarations' visibility only; native code is unchanged.
"""

import argparse
import glob
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

PIN = "fece8caa860bdf6c9c82bdb9253f22a0cf87150c"
SOURCE_SHA256 = {
    "Code/GraphMol/Depictor/EmbeddedFrag.cpp": "a522803423899fa91e986f21db0d45b423b3d8ff85bdb2fb5fcb8ce614f1fa2b",
    "Code/GraphMol/Depictor/EmbeddedFrag.h": "aa9531e31c0353abac3ad7fb637f179046326b03f09b2e96ca64ab58c8c8541a",
    "Code/GraphMol/Depictor/DepictUtils.cpp": "31543385ae45049a153b67c3f26d6e37696e9d9c22669e78a647773e6bb45689",
    "Code/GraphMol/Depictor/DepictUtils.h": "d322579e2c65bf8a51e08f69941e9e9125aa6e51beaf99da70039c7362973634",
    "Code/GraphMol/Matrices.cpp": "fd69c704c084f7e9fd8c2279024f94e9421338bcf556ca45a55aba63937bb8ed",
    "Code/Geometry/point.h": "730244c9f054011af88e802d0fd486a9ddd0e6c385dfbde534aab4e97088f085",
    "Code/Geometry/Transform2D.cpp": "c8cf18d72544c276836d74a17312b7d425ed2cbeed67409ca09fe9b5c80d1a4f",
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rdkit-source", type=Path, required=True)
    parser.add_argument("--boost-include", type=Path, default=Path("/usr/include"))
    args = parser.parse_args()
    assert rdBase.rdkitVersion == "2026.09.1"
    version = re.search(
        r"#define BOOST_VERSION\s+(\d+)", (args.boost_include / "boost/version.hpp").read_text()
    )
    assert version is not None
    number = int(version[1])
    assert f"{number // 100000}_{number // 100 % 1000}" == rdBase.boostVersion
    root = Path(__file__).resolve().parents[1]
    source = args.rdkit_source.resolve() / "Code"
    for relative, digest in SOURCE_SHA256.items():
        assert hashlib.sha256((args.rdkit_source / relative).read_bytes()).hexdigest() == digest
    if (args.rdkit_source / ".git").exists():
        assert (
            subprocess.check_output(
                ["git", "-C", str(args.rdkit_source), "rev-parse", "HEAD"], text=True
            ).strip()
            == PIN
        )
    generated = root / "artifacts/depict-collision-observation"
    (generated / "GraphMol/Depictor").mkdir(parents=True, exist_ok=True)
    helpers = (
        (root / "reference/depict_attachment_reference.cpp").read_text().split("int main() {", 1)[0]
    )
    (generated / "depict-native-fragment-observation.inc").write_text(helpers)
    header = (source / "GraphMol/Depictor/EmbeddedFrag.h").read_text()
    assert header.count(" private:") == 1
    (generated / "GraphMol/Depictor/EmbeddedFrag.h").write_text(
        header.replace(" private:", " public:")
    )
    config = generated / "RDGeneral"
    config.mkdir(exist_ok=True)
    macros = set()
    for header in source.rglob("*.h"):
        macros.update(
            re.findall(r"\bRDKIT_[A-Z0-9_]+_EXPORT\b", header.read_text(errors="replace"))
        )
    (config / "export.h").write_text(
        "#pragma once\n"
        + "".join(
            f"#define {macro}"
            + (" __declspec(dllimport)" if sys.platform == "win32" else "")
            + "\n"
            for macro in sorted(macros)
        )
    )
    (config / "RDConfig.h").write_text("#pragma once\n")
    package = Path(rdkit.__file__).parent
    names = ("Depictor", "GraphMol", "RDGeometryLib", "RDGeneral", "SubstructMatch", "SmilesParse")
    if sys.platform == "win32":
        links = []
        libs = package.parent / "rdkit.libs"
        for name in names:
            matches = list(libs.glob(f"RDKit{name}-*.dll"))
            assert len(matches) == 1, (name, matches)
            dll = matches[0]
            listing = subprocess.check_output(
                ["dumpbin", "/nologo", "/exports", str(dll)], text=True
            )
            exports = re.findall(r"^\s*\d+\s+[0-9A-Fa-f]+\s+[0-9A-Fa-f]+\s+(\S+)", listing, re.M)
            assert exports
            definition = generated / f"{name}.def"
            definition.write_bytes(
                (f"LIBRARY {dll.name}\nEXPORTS\n" + "\n".join(exports) + "\n").encode()
            )
            link = generated / f"{name}.lib"
            subprocess.run(
                ["lib", "/nologo", "/machine:x64", f"/def:{definition}", f"/out:{link}"],
                check=True,
                cwd=generated,
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
            "/I" + str(generated),
            "/I" + str(source),
            "/I" + str(source / "GraphMol/Depictor"),
            "/I" + str(args.boost_include.resolve()),
            str(root / "reference/depict_collision_reference.cpp"),
            str(root / "reference/depict_windows_runtime.cpp"),
            *links,
            "/Fe:" + str(root / "artifacts/depict-collision-oracle.exe"),
            "/Fo" + str(generated) + "\\",
        ]
        result = subprocess.run(command, cwd=generated, capture_output=True)
        (generated / "compiler.log").write_bytes(result.stdout + result.stderr)
        result.check_returncode()
        (generated / "native-build.json").write_text(
            json.dumps(
                dict(
                    compiler_command=command,
                    compiler_log_sha256=hashlib.sha256(result.stdout + result.stderr).hexdigest(),
                    runtime_initializer_sha256=hashlib.sha256(
                        (root / "reference/depict_windows_runtime.cpp").read_bytes()
                    ).hexdigest(),
                    boost_version=rdBase.boostVersion,
                    boost_header_sha256=hashlib.sha256(
                        (args.boost_include / "boost/version.hpp").read_bytes()
                    ).hexdigest(),
                )
            )
        )
        return
    if sys.platform.startswith("linux"):
        libs = package.parent / "rdkit.libs"
        libraries = []
        for name in names:
            matches = glob.glob(str(libs / f"libRDKit{name}-*.so*"))
            assert len(matches) == 1, (name, matches)
            libraries += matches
        libraries += ["-Wl,-rpath-link," + str(libs)]
    elif sys.platform == "darwin":
        libs = package / ".dylibs"
        libraries = [str(libs / f"libRDKit{name}.1.dylib") for name in names]
    else:
        raise SystemExit("Use the matching C++20 SDK on Windows, retaining the observation source.")
    python = Path(sysconfig.get_config_var("LIBDIR")) / sysconfig.get_config_var("LDLIBRARY")
    libraries += [str(python), "-Wl,-rpath," + str(python.parent), "-Wl,-rpath," + str(libs)]
    subprocess.run(
        [
            os.environ.get("CXX", "c++"),
            "-std=c++20",
            "-I" + str(generated),
            "-I" + str(source),
            "-I" + str(source / "GraphMol/Depictor"),
            "-I" + str(args.boost_include),
            str(root / "reference/depict_collision_reference.cpp"),
            *libraries,
            "-o",
            str(root / "artifacts/depict-collision-oracle"),
        ],
        check=True,
        cwd=root,
    )


if __name__ == "__main__":
    main()
