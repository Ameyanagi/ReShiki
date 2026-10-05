"""Generate the pinned builtin query catalog; no runtime SMARTS parser is needed."""

import argparse
import hashlib
import json
import re
import struct
import subprocess
from pathlib import Path

from rdkit import Chem, rdBase

PIN = "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985"
SOURCE = "Code/GraphMol/Depictor/TemplateSmarts.h"


def catalog(source):
    assert rdBase.rdkitVersion == "2026.03.6"
    assert (
        subprocess.check_output(["git", "-C", str(source), "rev-parse", "HEAD"], text=True).strip()
        == PIN
    )
    path = source / SOURCE
    strings = [
        json.loads(line.strip().rstrip(","))
        for line in path.read_text().splitlines()
        if line.lstrip().startswith('"')
    ]
    assert len(strings) == 578
    templates = []
    for text in strings:
        mol = Chem.MolFromSmarts(text)
        assert mol is not None and mol.GetNumConformers() == 1
        Chem.GetSymmSSSR(mol)
        degrees = []
        for atom in mol.GetAtoms():
            query = re.fullmatch(r"\[!#200(?:&D([2346]))?\]", atom.GetSmarts())
            assert query is not None, atom.DescribeQuery()
            degrees.append(int(query[1]) if query[1] else None)
        assert all(b.GetSmarts() == "~" for b in mol.GetBonds())
        conf = mol.GetConformer()
        assert not conf.Is3D()
        points = [conf.GetAtomPosition(i) for i in range(mol.GetNumAtoms())]
        assert all(p.z == 0 for p in points)
        templates.append(
            dict(
                degrees=degrees,
                edges=[[b.GetBeginAtomIdx(), b.GetEndAtomIdx()] for b in mol.GetBonds()],
                positions=[dict(x=p.x, y=p.y) for p in points],
            )
        )
    return dict(
        source_commit=PIN,
        source_sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
        templates=templates,
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rdkit-source", type=Path, required=True)
    parser.add_argument(
        "--check",
        action="store_true",
        help="Verify all generated values, including exact coordinate bits",
    )
    args = parser.parse_args()
    result = catalog(args.rdkit_source)
    path = Path(__file__).resolve().parents[1] / "src/chemistry/depict/templates/builtin.json"
    if args.check:
        actual = json.loads(path.read_text())
        assert (
            actual["source_commit"] == result["source_commit"]
            and actual["source_sha256"] == result["source_sha256"]
        )
        assert len(actual["templates"]) == len(result["templates"])
        for a, b in zip(actual["templates"], result["templates"], strict=True):
            assert a["degrees"] == b["degrees"] and a["edges"] == b["edges"]
            for x, y in zip(a["positions"], b["positions"], strict=True):
                assert all(struct.pack(">d", x[k]) == struct.pack(">d", y[k]) for k in ("x", "y"))
    else:
        path.write_text(json.dumps(result, separators=(",", ":")) + "\n")
    print(
        f"Generated {len(result['templates'])} templates, maximum {max(len(t['degrees']) for t in result['templates'])} atoms, {path.stat().st_size} bytes"
    )


if __name__ == "__main__":
    main()
