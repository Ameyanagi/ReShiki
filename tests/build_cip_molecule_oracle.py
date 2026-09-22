"""Build optional native CIP fixture generators on macOS.

Requires the pinned RDKit checkout, its locked Python wheel, Apple clang and
Boost headers. The resulting executable is development-only.
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

PIN = "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--rdkit-source", required=True, type=Path)
    parser.add_argument("--boost-include", type=Path, default=Path("/opt/homebrew/include"))
    parser.add_argument(
        "--component", choices=("molecule", "digraph", "rules", "pairing"), default="molecule"
    )
    args = parser.parse_args()
    assert rdBase.rdkitVersion == "2026.03.6"
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
    subprocess.run(
        [
            "/usr/bin/clang++",
            "-std=c++20",
            "-I" + str(include.parent),
            "-I" + str(source),
            "-I" + str(args.boost_include),
            str(
                root
                / f"tests/cip_{'rules' if args.component == 'pairing' else args.component}_reference.cpp"
            ),
            *[str(libs / f"libRDKit{n}.1.dylib") for n in ("CIPLabeler", "GraphMol", "RDGeneral")],
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
            str(root / f"tests/cip_{args.component}_reference.py"),
            "--oracle",
            str(output),
            "--write-fixture",
        ],
        check=True,
        env={**os.environ, "DYLD_LIBRARY_PATH": str(libs)},
    )


if __name__ == "__main__":
    main()
