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

PIN = "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985"
SOURCE_SHA256 = {
    "Code/GraphMol/Depictor/TemplateSmarts.h": "69530df08d9e532a2ce89275359333fe24a2c50868fe91f1942970bc03ee890b",
    "Code/GraphMol/Depictor/Templates.h": "a4870cc64682e1812dc5cd83028d19410dd4bdfd1876e59c719afba994879106",
    "Code/GraphMol/Depictor/EmbeddedFrag.cpp": "a3c55426a09deb23443e53cb2b59b4056e9d312b8fdb1c749da5033bc5a1eb33",
    "Code/GraphMol/Depictor/EmbeddedFrag.h": "a45692bbffd2dff60aa608565dc98d366a2aae7cfb75eeb372f1cf35a8f3f0f4",
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rdkit-source", type=Path, required=True)
    parser.add_argument("--boost-include", type=Path, default=Path("/usr/include"))
    args = parser.parse_args()
    assert rdBase.rdkitVersion == "2026.03.6"
    root = Path(__file__).resolve().parents[1]
    source = args.rdkit_source.resolve() / "Code"
    for relative, digest in SOURCE_SHA256.items():
        assert hashlib.sha256((args.rdkit_source / relative).read_bytes()).hexdigest() == digest
    catalog = json.loads((root / "src/chemistry/depict/templates/builtin.json").read_text())
    assert catalog["source_commit"] == PIN
    assert (
        hashlib.sha256((source / "GraphMol/Depictor/TemplateSmarts.h").read_bytes()).hexdigest()
        == catalog["source_sha256"]
    )
    if (args.rdkit_source / ".git").exists():
        assert (
            subprocess.check_output(
                ["git", "-C", str(args.rdkit_source), "rev-parse", "HEAD"], text=True
            ).strip()
            == PIN
        )
    generated = root / "artifacts/depict-template-observation"
    (generated / "GraphMol/Depictor").mkdir(parents=True, exist_ok=True)
    helpers = (
        (root / "tests/depict_attachment_reference.cpp").read_text().split("int main() {", 1)[0]
    )
    (generated / "depict-native-fragment-observation.inc").write_text(helpers)
    header = (source / "GraphMol/Depictor/EmbeddedFrag.h").read_text()
    assert header.count(" private:") == 1
    (generated / "GraphMol/Depictor/EmbeddedFrag.h").write_text(
        header.replace(" private:", " public:")
    )
    code = (source / "GraphMol/Depictor/EmbeddedFrag.cpp").read_text()
    start = code.index("static bool checkStereoChemistry(")
    middle = code.index("bool EmbeddedFrag::matchToTemplate(", start)
    end = code.index("  // copy over new coordinates", middle)
    observed = code[middle:end]
    observed = observed.replace(
        "bool EmbeddedFrag::matchToTemplate(const RDKit::INT_VECT &ringSystemAtoms)",
        "static std::pair<int,RDKit::MatchVectType> observedMatch(const RDKit::ROMol *dp_mol,const RDKit::INT_VECT &ringSystemAtoms)",
    )
    observed = observed.replace("return false;", "return {-1,{}};")
    observed = observed.replace(
        "  for (const auto &mol :", "  int observed_slot=-1;\n  for (const auto &mol :"
    )
    marker = "coordinate_templates.getMatchingTemplates(ringSystemAtoms.size())) {"
    assert marker in observed
    observed = observed.replace(marker, marker + "\n    ++observed_slot;")
    (generated / "depict-template-observation.inc").write_text(
        "namespace RDDepict {\n"
        + code[start:middle]
        + observed
        + "return {observed_slot,match};\n}\n}\n"
    )
    config = generated / "RDGeneral"
    config.mkdir(exist_ok=True)
    macros = set()
    for header in source.rglob("*.h"):
        macros.update(
            re.findall(r"\bRDKIT_[A-Z0-9_]+_EXPORT\b", header.read_text(errors="replace"))
        )
    (config / "export.h").write_text(
        "#pragma once\n" + "".join(f"#define {macro}\n" for macro in sorted(macros))
    )
    (config / "RDConfig.h").write_text("#pragma once\n")
    package = Path(rdkit.__file__).parent
    python = Path(sysconfig.get_config_var("LIBDIR")) / sysconfig.get_config_var("LDLIBRARY")
    names = ("Depictor", "GraphMol", "RDGeometryLib", "RDGeneral", "SubstructMatch", "SmilesParse")
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
    libraries += [str(python), "-Wl,-rpath," + str(python.parent), "-Wl,-rpath," + str(libs)]
    subprocess.run(
        [
            os.environ.get("CXX", "c++"),
            "-std=c++20",
            "-I" + str(generated),
            "-I" + str(source),
            "-I" + str(source / "GraphMol/Depictor"),
            "-I" + str(args.boost_include),
            str(root / "tests/depict_templates_reference.cpp"),
            *libraries,
            "-o",
            str(root / "artifacts/depict-templates-oracle"),
        ],
        check=True,
        cwd=root,
    )


if __name__ == "__main__":
    main()
