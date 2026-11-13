"""Build optional native CIP fixture generators on macOS.

Requires the pinned RDKit checkout, its locked Python wheel, Apple clang,
Boost headers and RingDecomposerLib headers. The resulting executable is
development-only.
"""

import argparse
import os
import re
import subprocess
import sys
import sysconfig
from pathlib import Path

import rdkit
from rdkit import rdBase

PIN = "fece8caa860bdf6c9c82bdb9253f22a0cf87150c"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--rdkit-source", required=True, type=Path)
    parser.add_argument("--boost-include", type=Path, default=Path("/opt/homebrew/include"))
    parser.add_argument(
        "--ringdecomposer-include",
        type=Path,
        help="directory containing RingDecomposerLib.h (defaults to the RDKit source dependency)",
    )
    parser.add_argument(
        "--component",
        choices=("molecule", "digraph", "rules", "pairing", "configuration"),
        default="molecule",
    )
    args = parser.parse_args()
    ringdecomposer_include = args.ringdecomposer_include or (
        args.rdkit_source / "External/RingFamilies/RingDecomposerLib/src/RingDecomposerLib"
    )
    ringdecomposer_header = ringdecomposer_include / "RingDecomposerLib.h"
    if not ringdecomposer_header.is_file():
        parser.error(
            f"RingDecomposerLib.h is missing at {ringdecomposer_header}; "
            "extract RDKit's RingDecomposerLib dependency under "
            "External/RingFamilies/RingDecomposerLib, or pass "
            "--ringdecomposer-include with the directory containing that header"
        )
    assert rdBase.rdkitVersion == "2026.09.1"
    assert (
        subprocess.check_output(
            ["git", "-C", str(args.rdkit_source), "rev-parse", "HEAD"], text=True
        ).strip()
        == PIN
    )
    root = Path(__file__).resolve().parents[1]
    output = root / f"artifacts/cip-{args.component}-oracle"
    include = root / "artifacts/cip-oracle-include/RDGeneral"
    include.mkdir(parents=True, exist_ok=True)
    source = args.rdkit_source / "Code"
    macros = set()
    for header in source.rglob("*.h"):
        macros.update(
            re.findall(r"\bRDKIT_[A-Z0-9_]+_EXPORT\b", header.read_text(errors="replace"))
        )
    (include / "export.h").write_text(
        "#pragma once\n" + "".join(f"#define {m}\n" for m in sorted(macros))
    )
    (include / "RDConfig.h").write_text("#pragma once\n")
    libs = Path(rdkit.__file__).parent / ".dylibs"
    python = Path(sysconfig.get_config_var("LIBDIR")) / sysconfig.get_config_var("LDLIBRARY")
    standalone = args.component in ("configuration", "rules", "pairing")
    cip_sources = []
    if standalone:
        cip_sources = sorted(
            path
            for path in (source / "GraphMol/CIPLabeler").rglob("*.cpp")
            if path.name not in ("CIPLabeler.cpp", "catch_tests.cpp")
            and not {"Wrap", "nbWrap"}.intersection(path.parts)
        )
    libraries = ("GraphMol", "RDGeneral") if standalone else ("CIPLabeler", "GraphMol", "RDGeneral")
    subprocess.run(
        [
            "/usr/bin/clang++",
            "-std=c++20",
            "-I" + str(include.parent),
            "-I" + str(source),
            "-I" + str(args.boost_include),
            "-I" + str(ringdecomposer_include),
            str(
                root
                / f"reference/cip_{'rules' if args.component == 'pairing' else args.component}_reference.cpp"
            ),
            *map(str, cip_sources),
            *[str(libs / f"libRDKit{n}.1.dylib") for n in libraries],
            str(python),
            "-Wl,-rpath," + str(python.parent),
            "-o",
            str(output),
        ],
        check=True,
    )
    # The wheel's absolute install names need its own library directory.
    subprocess.run(
        [
            sys.executable,
            str(root / f"reference/cip_{args.component}_reference.py"),
            "--oracle",
            str(output),
            "--write-fixture",
        ],
        check=True,
        env={**os.environ, "DYLD_LIBRARY_PATH": str(libs)},
    )


if __name__ == "__main__":
    main()
