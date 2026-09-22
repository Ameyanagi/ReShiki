"""Capture Windows expansion through a pinned source adapter and native full layout.

Only unexported RDDepictor.cpp orchestration is extracted. Every request checks
both complete extracted wrappers against the original exported compute2DCoords.
Run using pinned x64 Python in an x64 MSVC developer prompt; no Rust is invoked.
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
import sysconfig
from pathlib import Path

import rdkit
from build_depict_expansion_oracle import SOURCE_SHA256
from rdkit import rdBase


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def extract_adapter(source, output):
    path = source / "GraphMol/Depictor/RDDepictor.cpp"
    original = path.read_bytes()
    assert digest(path) == SOURCE_SHA256["Code/GraphMol/Depictor/RDDepictor.cpp"]
    sections = []

    def section(name, first, last, replacements=()):
        begin = original.index(first)
        end = original.index(last, begin)
        body = original[begin:end]
        changed = body
        for old, new in replacements:
            assert changed.count(old) == 1, (name, old)
            changed = changed.replace(old, new, 1)
        sections.append(
            dict(
                name=name,
                byte_range=[begin, end],
                verbatim_sha256=hashlib.sha256(body).hexdigest(),
                compiled_sha256=hashlib.sha256(changed).hexdigest(),
                replacements=[(a.decode(), b.decode()) for a, b in replacements],
            )
        )
        return changed

    helpers = section(
        "DepictorLocal helpers", b"constexpr auto ISQRT2 = ", b"// we do not use std::copysign"
    )
    initial = section(
        "initial",
        b"void computeInitialCoords(",
        b"unsigned int copyCoordinate(",
        ((b"void computeInitialCoords(", b"void expansionSourceInitial("),),
    )
    copy = section(
        "conformer copy",
        b"unsigned int copyCoordinate(",
        b"void setRingSystemTemplates(",
        ((b"unsigned int copyCoordinate(", b"unsigned int expansionSourceCopy("),),
    )
    full = section(
        "complete wrapper",
        b"unsigned int compute2DCoords(RDKit::ROMol &mol,\n                             const Compute2DCoordParameters &params)",
        b"//! \\brief Compute the 2D coordinates such",
        (
            (b"unsigned int compute2DCoords(", b"unsigned int expansionSourceFull("),
            (
                b"  computeInitialCoords(cp, params.coordMap, efrags, params.useRingTemplates);",
                b"  expansionSourceInitial(cp, params.coordMap, efrags, params.useRingTemplates);",
            ),
            (
                b"copyCoordinate(mol, efrags, params.clearConfs)",
                b"expansionSourceCopy(mol, efrags, params.clearConfs)",
            ),
        ),
    )
    prefix = b"#include <GraphMol/Chirality.h>\n#include <GraphMol/Rings.h>\n#include <boost/dynamic_bitset.hpp>\n#include <algorithm>\nnamespace RDDepict { namespace DepictorLocal {\n"
    output.write_bytes(prefix + helpers + b"\n}\n" + initial + copy + full + b"\n}\n")
    return dict(
        source="Code/GraphMol/Depictor/RDDepictor.cpp",
        source_sha256=digest(path),
        sections=sections,
        generated_sha256=digest(output),
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rdkit-source", type=Path, required=True)
    parser.add_argument("--boost-include", type=Path, required=True)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--fma3", choices=("0", "1"))
    args = parser.parse_args()
    assert sys.platform == "win32" and sysconfig.get_platform() == "win-amd64"
    assert rdBase.rdkitVersion == "2026.03.6"
    boost = args.boost_include / "boost/version.hpp"
    version = re.search(r'#define BOOST_LIB_VERSION "([^"]+)"', boost.read_text())
    assert version and version[1] == rdBase.boostVersion
    root = Path(__file__).resolve().parents[1]
    source = args.rdkit_source.resolve() / "Code"
    for relative, expected in SOURCE_SHA256.items():
        assert digest(args.rdkit_source / relative) == expected, relative
    directory = root / "artifacts/depict-windows-expansion"
    generated = directory / "generated"
    config = generated / "RDGeneral"
    config.mkdir(parents=True, exist_ok=True)
    macros = set()
    for path in source.rglob("*.h"):
        macros.update(re.findall(r"\bRDKIT_[A-Z0-9_]+_EXPORT\b", path.read_text(errors="replace")))
    (config / "export.h").write_bytes(
        (
            "#pragma once\n"
            + "".join(f"#define {name} __declspec(dllimport)\n" for name in sorted(macros))
        ).encode()
    )
    (config / "RDConfig.h").write_bytes(b"#pragma once\n")
    header = (source / "GraphMol/Depictor/EmbeddedFrag.h").read_bytes()
    assert header.count(b" private:") == 1
    observation = generated / "GraphMol/Depictor/EmbeddedFrag.h"
    observation.parent.mkdir(parents=True, exist_ok=True)
    observation.write_bytes(header.replace(b" private:", b" public:"))
    # No private method is called by this observer; exposed fields do not alter
    # MSVC method access mangling. All fragment methods called are public exports.
    helpers = (
        (root / "tests/depict_attachment_reference.cpp").read_bytes().split(b"int main() {", 1)[0]
    )
    (generated / "depict-native-fragment-observation.inc").write_bytes(helpers)
    adapter = extract_adapter(source, generated / "depict-expansion-windows-source.inc")
    code = (source / "GraphMol/Depictor/RDDepictor.cpp").read_bytes()
    begin = code.index(b"void computeInitialCoords(")
    end = code.index(b"unsigned int copyCoordinate(", begin)
    observed = (
        code[begin:end]
        .replace(b"void computeInitialCoords(", b"void observedInitial(", 1)
        .replace(b"bool useRingTemplates) {", b"bool useRingTemplates, Capture &capture) {", 1)
    )
    for old, new in (
        (
            b"  RDKit::VECT_INT_VECT arings;",
            b"  capture.ranks=atomRanks;\n  RDKit::VECT_INT_VECT arings;",
        ),
        (
            b"  auto nratms = DepictorLocal::getNonEmbeddedAtoms(mol, efrags);",
            b"  auto nratms = DepictorLocal::getNonEmbeddedAtoms(mol, efrags);\n  capture.seeded(efrags,nratms);",
        ),
        (
            b"    mri->expandEfrag(nratms, efrags);",
            b"    capture.before(efrags,nratms,mri);\n    mri->expandEfrag(nratms, efrags);\n    capture.after(efrags,nratms);",
        ),
    ):
        assert observed.count(old) == 1
        observed = observed.replace(old, new, 1)
    (generated / "depict-expansion-observation.inc").write_bytes(
        b"namespace RDDepict {\n" + observed + b"}\n"
    )
    package = Path(rdkit.__file__).parent
    library_dir = package.parent / "rdkit.libs"
    links, libraries, export_hashes = [], {}, {}
    for name in ("Depictor", "GraphMol", "RDGeometryLib", "RDGeneral"):
        matches = list(library_dir.glob(f"RDKit{name}-*.dll"))
        assert len(matches) == 1
        dll = matches[0]
        listing = subprocess.check_output(["dumpbin", "/nologo", "/exports", str(dll)], text=True)
        exports = re.findall(r"^\s*\d+\s+[0-9A-Fa-f]+\s+[0-9A-Fa-f]+\s+(\S+)", listing, re.M)
        assert exports
        if name == "Depictor":
            assert not any("computeInitialCoords" in e or "DepictorLocal" in e for e in exports)
            assert any("?compute2DCoords@" in e for e in exports)
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
        libraries[dll.name] = digest(dll)
        export_hashes[name] = digest(definition)
    reference = root / "tests/depict_expansion_reference.cpp"
    binary = directory / "oracle.exe"
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
        "/DRESHIKI_EXPANSION_WINDOWS_SOURCE",
        "/utf-8",
        "/I" + str(generated),
        "/I" + str(source),
        "/I" + str(source / "GraphMol/Depictor"),
        "/I" + str(args.boost_include.resolve()),
        str(reference),
        str(root / "tests/depict_windows_runtime.cpp"),
        *links,
        "/Fe:" + str(binary),
        "/Fo" + str(directory) + "\\",
    ]
    built = subprocess.run(command, cwd=directory, capture_output=True)
    (directory / "compiler.log").write_bytes(built.stdout + built.stderr)
    built.check_returncode()
    imports = subprocess.check_output(["dumpbin", "/nologo", "/imports", str(binary)])
    assert b"?compute2DCoords@RDDepict@@" in imports and b"?BOND_LEN@RDDepict@@" in imports
    (directory / "imports.txt").write_bytes(imports)
    native_build = dict(
        fma3=args.fma3,
        runtime_initializer_sha256=digest(root / "tests/depict_windows_runtime.cpp"),
        ucrt_sha256=digest(Path(os.environ["SYSTEMROOT"]) / "System32/ucrtbase.dll"),
        builder_sha256=digest(Path(__file__)),
        compiler_command=command,
        machine=platform.machine(),
        processor=platform.processor(),
        input_fixture_jsonl_sha256=hashlib.sha256(
            gzip.decompress(
                (root / "tests/fixtures/depict-expansion-linux-native.json.gz").read_bytes()
            )
        ).hexdigest(),
        import_table_sha256=hashlib.sha256(imports).hexdigest(),
        raw_initial_source="Exact source adapter validated against original public full layout",
        input_headers_sha256=hashlib.sha256(
            "".join(
                str(p.relative_to(source)) + ":" + digest(p) + "\n"
                for p in sorted(source.rglob("*.h"))
            ).encode()
        ).hexdigest(),
        compiler_log_sha256=digest(directory / "compiler.log"),
        boost_version=rdBase.boostVersion,
        boost_version_header_sha256=digest(boost),
        source_adapter=adapter,
        observation_sha256=digest(generated / "depict-expansion-observation.inc"),
        export_hashes=export_hashes,
        export_header_sha256=digest(config / "export.h"),
        runtime="/MD, x64 MSVC and pinned x64 wheel",
        independent_check="Every request: original exported compute2DCoords vs extracted full wrapper; both canonical settings; exact outcome and conformer bits",
    )
    (directory / "native-build.json").write_text(
        json.dumps(native_build, sort_keys=True), encoding="utf-8"
    )
    output = (
        args.output or root / "tests/fixtures/depict-expansion-windows-native.json.gz"
    ).resolve()
    previous = ctypes.windll.kernel32.SetErrorMode(0x0001 | 0x0002 | 0x8000)
    environment = {**os.environ, "PATH": str(library_dir) + os.pathsep + os.environ["PATH"]}
    environment.pop("RESHIKI_REFERENCE_FMA3", None)
    if args.fma3 is not None:
        environment["RESHIKI_REFERENCE_FMA3"] = args.fma3
    try:
        subprocess.run(
            [
                sys.executable,
                str(root / "tests/depict_expansion_reference.py"),
                "--oracle",
                str(binary),
                "--rdkit-source",
                str(args.rdkit_source.resolve()),
                "--fixture",
                str(root / "tests/fixtures/depict-expansion-linux-native.json.gz"),
                "--replay",
                "--output",
                str(output),
            ],
            cwd=root,
            check=True,
            timeout=600,
            env=environment,
        )
    finally:
        ctypes.windll.kernel32.SetErrorMode(previous)
    first, _, body = gzip.decompress(output.read_bytes()).partition(b"\n")
    metadata = json.loads(first)
    metadata["provenance"]["library_sha256"] = libraries
    metadata["provenance"]["native_build"] = native_build
    data = json.dumps(metadata, separators=(",", ":")).encode() + b"\n" + body
    output.write_bytes(gzip.compress(data, mtime=0))
    print(f"Recorded {output.name}: {digest(output)}")


if __name__ == "__main__":
    main()
