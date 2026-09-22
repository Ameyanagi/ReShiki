"""Build the development-only native geometry oracle against the pinned wheel.

macOS: .venv/bin/python tests/build_depict_geometry_oracle.py --rdkit-source /path/to/rdkit
Linux: use the pinned wheel's Python, add --boost-include /usr/include if needed.
Windows: build tests/depict_geometry_reference.cpp using C++20 and a matching
RDKit 2026.03.6 SDK (Depictor, GraphMol, RDGeometryLib, RDGeneral import libraries),
then run depict_geometry_reference.py --oracle path/to/oracle.exe --write-fixture.
The SDK must use the pinned source commit and matching runtime/architecture.
The checked-in fixture needs only standard-library Python on all three hosts.
"""

import argparse
import glob
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
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rdkit-source", required=True, type=Path)
    parser.add_argument("--boost-include", type=Path, default=Path("/opt/homebrew/include"))
    parser.add_argument("--replay", action="store_true", help="Preserve the checked-in input bits")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--component", choices=("geometry", "rings"), default="geometry")
    args = parser.parse_args()
    assert rdBase.rdkitVersion == "2026.03.6"
    assert (
        subprocess.check_output(
            ["git", "-C", str(args.rdkit_source), "rev-parse", "HEAD"], text=True
        ).strip()
        == PIN
    )
    root = Path(__file__).resolve().parents[1]
    output = root / f"artifacts/depict-{args.component}-oracle"
    include = root / "artifacts/depict-oracle-include/RDGeneral"
    include.mkdir(parents=True, exist_ok=True)
    source = args.rdkit_source / "Code"
    macros = set()
    for header in source.rglob("*.h"):
        macros.update(
            re.findall(r"\bRDKIT_[A-Z0-9_]+_EXPORT\b", header.read_text(errors="replace"))
        )
    (include / "export.h").write_text(
        "#pragma once\n" + "".join(f"#define {macro}\n" for macro in sorted(macros))
    )
    (include / "RDConfig.h").write_text("#pragma once\n")
    package = Path(rdkit.__file__).parent
    names = ("Depictor", "GraphMol", "RDGeometryLib", "RDGeneral")
    python = Path(sysconfig.get_config_var("LIBDIR")) / sysconfig.get_config_var("LDLIBRARY")
    if sys.platform == "darwin":
        libs = package / ".dylibs"
        library_files = [str(libs / f"libRDKit{name}.1.dylib") for name in names]
        library_files += [str(python), "-Wl,-rpath," + str(python.parent)]
        compiler = os.environ.get("CXX", "/usr/bin/clang++")
        env = {**os.environ, "DYLD_LIBRARY_PATH": str(libs)}
    elif sys.platform.startswith("linux"):
        libs = package.parent / "rdkit.libs"
        library_files = []
        for name in names:
            matches = glob.glob(str(libs / f"libRDKit{name}-*.so*"))
            assert len(matches) == 1, (name, matches)
            library_files.extend(matches)
        library_files += [
            str(python),
            "-Wl,-rpath," + str(python.parent),
            "-Wl,-rpath," + str(libs),
            "-Wl,-rpath-link," + str(libs),
        ]
        compiler = os.environ.get("CXX", "c++")
        env = {**os.environ, "LD_LIBRARY_PATH": str(libs)}
    else:
        raise SystemExit(
            "Use a matching RDKit SDK and the C++20 compiler on Windows; see docstring"
        )
    subprocess.run(
        [
            compiler,
            "-std=c++20",
            "-I" + str(include.parent),
            "-I" + str(source),
            "-I" + str(args.boost_include),
            str(root / f"tests/depict_{args.component}_reference.cpp"),
            *library_files,
            "-o",
            str(output),
        ],
        check=True,
        cwd=root,
    )
    generate = [
        sys.executable,
        str(root / f"tests/depict_{args.component}_reference.py"),
        "--oracle",
        str(output),
        "--rdkit-source",
        str(args.rdkit_source),
    ]
    if args.component == "geometry":
        generate.append("--write-fixture")
    elif not args.output:
        generate.extend(
            ["--output", str(root / "tests/fixtures/depict-rings-macos-native.json.gz")]
        )
    if args.replay:
        generate.append("--replay")
    if args.output:
        generate.extend(["--output", str(args.output.resolve())])
    subprocess.run(
        generate,
        check=True,
        cwd=root,
        env=env,
    )


if __name__ == "__main__":
    main()
