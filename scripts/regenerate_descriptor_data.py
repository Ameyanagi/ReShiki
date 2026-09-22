"""Compile the pinned descriptor SMARTS to a checked, runtime-independent AST."""

import argparse
import ast
import hashlib
import json
import re
from pathlib import Path

from rdkit import Chem, rdBase

VERSION = "2026.03.6"
ROOT = Path(__file__).resolve().parents[1]


def strings(text):
    return "".join(ast.literal_eval(s) for s in re.findall(r'"(?:[^"\\]|\\.)*"', text))


class Compiler:
    def __init__(self):
        self.patterns = []

    def expr(self, node):
        result: dict[str, object]
        desc = node["descr"]
        operations = {
            "AtomAnd": "all",
            "AtomOr": "any",
            "BondAnd": "all",
            "BondOr": "any",
            "AtomNull": "always",
            "BondNull": "always",
            "AtomType": "atom_type",
            "AtomAtomicNum": "number",
            "AtomHCount": "hydrogens",
            "AtomTotalDegree": "degree",
            "AtomTotalValence": "valence",
            "AtomFormalCharge": "charge",
            "AtomIsAliphatic": "aliphatic",
            "AtomIsAromatic": "aromatic",
            "BondOrder": "bond_order",
            "SingleOrAromaticBond": "single_or_aromatic",
            "BondInRing": "ring_bond",
        }
        if desc == "RecursiveStructure":
            result = dict(op="recursive", value=self.molecule(node["subquery"]))
        elif desc in operations:
            result = dict(op=operations[desc])
            if "children" in node:
                result["children"] = [self.expr(c) for c in node["children"]]
            elif "val" in node:
                result["value"] = node["val"]
        else:
            raise ValueError(f"Unsupported query: {node}")
        if node.get("negated"):
            result = dict(op="not", children=[result])
        return result

    def molecule(self, mol):
        extension = next(e for e in mol["extensions"] if e["name"] == "rdkitQueries")
        if extension["formatVersion"] != 10 or extension["toolkitVersion"] != VERSION:
            raise ValueError("Unexpected query encoding")
        atoms = [self.expr(q) for q in extension["atomQueries"]]
        bonds = [
            dict(a=b["atoms"][0], b=b["atoms"][1], query=self.expr(q))
            for b, q in zip(mol["bonds"], extension.get("bondQueries", []), strict=True)
        ]
        if len(atoms) != len(mol["atoms"]):
            raise ValueError("Missing atom query")
        index = len(self.patterns)
        self.patterns.append(dict(atoms=atoms, bonds=bonds))
        return index

    def compile(self, smarts):
        query = Chem.MolFromSmarts(smarts)
        if query is None:
            raise ValueError(f"Invalid source SMARTS: {smarts}")
        return self.molecule(json.loads(Chem.MolsToJSON([query]))["molecules"][0])


def main():
    # The JSON writer warns about its ordinary bond-order field for aromatic
    # queries. We consume the complete bondQueries extension instead.
    rdBase.DisableLog("rdApp.warning")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rdkit-source", type=Path, default=Path.home() / "dev/rdkit")
    args = parser.parse_args()
    if rdBase.rdkitVersion != VERSION:
        raise ValueError(f"Expected RDKit {VERSION}")
    base = args.rdkit_source / "Code/GraphMol/Descriptors"
    hashes = {
        "Crippen.cpp": "8f39e609a2d1c9207bfe0883987d7c4eafb57ceded47efa3f0847fb6ececc8e4",
        "Lipinski.cpp": "427002f10cecccc044d8c0b4ab313ac467fccdae80e2d97488c6bda224fe51fd",
    }
    sources = {}
    for name, digest in hashes.items():
        raw = (base / name).read_bytes()
        if hashlib.sha256(raw).hexdigest() != digest:
            raise ValueError(f"Unexpected {name} source")
        sources[name] = raw.decode()
    compiler = Compiler()
    table = strings(sources["Crippen.cpp"].split("const std::string defaultParamData =", 1)[1])
    rules = []
    for line in table.splitlines():
        if not line or line.startswith("#"):
            continue
        label, smarts, logp, mr, *_ = line.split("\t")
        rules.append(
            dict(
                label=label,
                smarts=smarts,
                pattern=compiler.compile(smarts),
                logp=float(logp or 0),
                mr=float(mr or 0),
            )
        )
    counters = {}
    for name in ["NumHBD", "NumHBA"]:
        match = re.search(
            r"^SMARTSCOUNTFUNC\(" + name + r",(.*?),\s*\"[\d.]+\"\);",
            sources["Lipinski.cpp"],
            re.MULTILINE | re.DOTALL,
        )
        if match is None:
            raise ValueError(f"Missing {name} definition")
        counters[name] = compiler.compile(strings(match[1]))
    output = dict(
        rdkit_version=VERSION,
        patterns=compiler.patterns,
        crippen=rules,
        donors=counters["NumHBD"],
        acceptors=counters["NumHBA"],
    )
    (ROOT / "src/chemistry/descriptor_data.json").write_text(
        json.dumps(output, separators=(",", ":")) + "\n"
    )
    print(
        f"Compiled {len(rules)} Crippen rules and two counters into {len(compiler.patterns)} queries"
    )


if __name__ == "__main__":
    main()
