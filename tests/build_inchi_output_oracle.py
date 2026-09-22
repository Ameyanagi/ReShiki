"""Compile the exact pinned native adapter with its GetStructFromINCHI call observed.

This optional development tool needs the RDKit checkout, official InChI 1.07.3
headers and the locked RDKit wheel. It generates no production dependency.
"""

import argparse
import hashlib
import json
import os
import re
import subprocess
from pathlib import Path

import rdkit
from rdkit import rdBase

PIN = "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985"
ADAPTER_SHA256 = "68c9b20d1d5920ed602ea931c1429395280c3d040971593073917618015183d1"
HEADER_SHA256 = "2d41d745be35a47853bf67fea03a46b1f518e85c42af9027a104662dbcb20116"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--rdkit-source", type=Path, required=True)
    parser.add_argument("--inchi-source", type=Path, required=True)
    parser.add_argument("--boost-include", type=Path, default=Path("/opt/homebrew/include"))
    args = parser.parse_args()
    assert rdBase.rdkitVersion == "2026.03.6"
    assert (
        subprocess.check_output(
            ["git", "-C", str(args.rdkit_source), "rev-parse", "HEAD"], text=True
        ).strip()
        == PIN
    )
    root = Path(__file__).resolve().parents[1]
    output = root / "artifacts/inchi-output-oracle"
    include = root / "artifacts/inchi-output-include"
    (include / "RDGeneral").mkdir(parents=True, exist_ok=True)
    code = args.rdkit_source / "Code"
    macros = set()
    for header in code.rglob("*.h"):
        macros.update(
            re.findall(r"\bRDKIT_[A-Z0-9_]+_EXPORT\b", header.read_text(errors="replace"))
        )
    (include / "RDGeneral/export.h").write_text(
        "#pragma once\n" + "".join(f"#define {macro}\n" for macro in sorted(macros))
    )
    (include / "RDGeneral/RDConfig.h").write_text("#pragma once\n")
    adapter_path = args.rdkit_source / "External/INCHI-API/inchi.cpp"
    assert hashlib.sha256(adapter_path.read_bytes()).hexdigest() == ADAPTER_SHA256
    adapter = adapter_path.read_text()
    notice = adapter[: adapter.index("//\n// Known issues:")]
    body = adapter[adapter.index("namespace {\n") : adapter.index("void fixOptionSymbol(")]
    # Observation calls do not alter any native chemistry statement. The
    # generated body and original source hashes are recorded in the manifest.
    for before, after in (
        ("      // 0Dstereo", '      snapshot("topology", *m);\n      // 0Dstereo'),
        (
            "    cleanUp(*m);",
            '    snapshot("assembled", *m);\n    cleanUp(*m);\n    snapshot("cleaned", *m);',
        ),
        (
            "    // call assignStereochemistry",
            '    snapshot("prepared", *m);\n    // call assignStereochemistry',
        ),
    ):
        assert body.count(before) == 1
        body = body.replace(before, after)
    # Record successful cleanup predicates without changing their return value
    # or any molecule operation. This proves the rare rewrite corpus reaches
    # each rule, independently of the Rust implementation.
    for match in reversed(list(re.finditer(r"bool (_Valence\w+)\([^)]*\) \{", body))):
        start = match.end()
        depth = 1
        end = start
        while depth:
            depth += (body[end] == "{") - (body[end] == "}")
            end += 1
        name = match.group(1)
        body = (
            body[:start]
            + body[start:end].replace("return true;", f'return rule_hit("{name}");')
            + body[end:]
        )
    (include / "inchi_adapter_body.h").write_text(notice + "\n" + body)
    inchi_include = args.inchi_source / "INCHI_BASE/src"
    assert hashlib.sha256((inchi_include / "inchi_api.h").read_bytes()).hexdigest() == HEADER_SHA256
    import sys
    import sysconfig

    if sys.platform == "darwin":
        libs = Path(rdkit.__file__).parent / ".dylibs"
        compiler = "/usr/bin/clang++"
        libraries = [
            str(libs / f"libRDKit{name}.1.dylib")
            for name in ("GraphMol", "SmilesParse", "SubstructMatch", "RDGeneral", "Inchi")
        ]
        python = Path(sysconfig.get_config_var("LIBDIR")) / sysconfig.get_config_var("LDLIBRARY")
        libraries += [str(python), "-Wl,-rpath," + str(python.parent)]
    else:
        libs = Path(rdkit.__file__).parent.parent / "rdkit.libs"
        compiler = os.environ.get("CXX", "c++")
        libraries = [
            str(next(libs.glob(f"libRDKit{name}-*.so.*")))
            for name in ("GraphMol", "SmilesParse", "SubstructMatch", "RDGeneral", "Inchi")
        ]
    command = [
        compiler,
        "-std=c++20",
        "-O2",
        "-I" + str(include),
        "-I" + str(code),
        "-I" + str(inchi_include),
        "-I" + str(args.boost_include),
        str(root / "tests/inchi_output_reference.cpp"),
        *libraries,
        "-Wl,-rpath," + str(libs),
        "-o",
        str(output),
    ]
    subprocess.run(command, check=True)
    manifest = {
        "rdkit_commit": PIN,
        "rdkit_version": rdBase.rdkitVersion,
        "command": command,
        "sources": {
            str(path): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in (
                adapter_path,
                inchi_include / "inchi_api.h",
                root / "tests/inchi_output_reference.cpp",
                include / "inchi_adapter_body.h",
            )
        },
    }
    (root / "artifacts/inchi-output-build.json").write_text(json.dumps(manifest, indent=2) + "\n")


if __name__ == "__main__":
    main()
