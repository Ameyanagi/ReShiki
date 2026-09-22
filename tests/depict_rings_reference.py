"""Independent pinned C++ ring selection and no-template constructor fixtures."""

import argparse
import gzip
import hashlib
import json
import os
import platform
import random
import struct
import subprocess
from pathlib import Path
from typing import TYPE_CHECKING

PIN = "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985"
ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests/fixtures/depict-rings-macos-native.json.gz"
SOURCES = (
    "Code/GraphMol/Depictor/DepictUtils.cpp",
    "Code/GraphMol/Depictor/DepictUtils.h",
    "Code/GraphMol/Depictor/EmbeddedFrag.cpp",
    "Code/GraphMol/Depictor/EmbeddedFrag.h",
    "Code/RDGeneral/types.cpp",
    "Code/Geometry/Transform2D.cpp",
    "Code/Geometry/point.h",
)


def bits(value):
    return struct.pack(">d", value).hex()


def systems(rings):
    remaining = list(range(len(rings)))
    result = []
    while remaining:
        group = [remaining.pop(0)]
        atoms = set(rings[group[0]])
        while True:
            found = next((i for i in remaining if atoms.intersection(rings[i])), None)
            if found is None:
                break
            remaining.remove(found)
            group.append(found)
            atoms.update(rings[found])
        result.append(group)
    return result


def cases():
    from rdkit import Chem, rdBase

    if TYPE_CHECKING or __package__:
        from .perception_reference import snapshot
    else:
        from perception_reference import snapshot

    assert rdBase.rdkitVersion == "2026.03.6"
    texts = [
        "C1CC1",
        "C1CCC1",
        "C1CCCC1",
        "C1CCCCC1",
        "c1ccccc1",
        "C1CCCCCCCCCCC1",
        "C1CCC2CCCCC2C1",
        "c1ccc2ccccc2c1",
        "c1ccc2c(c1)ccc1ccccc12",
        "C1CCC2(CC1)CCCC2",
        "C1CCC2(CC1)CCC1(CCCCC1)C2",
        "C1CC2CCC1C2",
        "C1CCC(CC12)CCC2",
        "C1C2CC3CC1CC(C2)C3",
        "C12C3C4C1C5C2C3C45",
        "C1CC2CCC3CCCC4CCC1C2C34",
        "CC1CCC2CCCCC2C1",
        "OC1CCCC1O",
        "CC1(C)CCC2(CC1)CCCC2",
        "c1ccc2[nH]ccc2c1",
        "c1ccc2nc3ccccc3cc2c1",
        "C1CCC1.C1CCCCC1",
        "C/C1=C/CCCCCC1",
        "C/C1=C\\CCCCCC1",
        "C1/C=C/CCCCCC1",
        "C1/C=C\\CCCCCC1",
        "C1/C=C/C=C/CCCCCC1",
        "C1/C=C/CC/C=C/CCCCCC1",
        "C[C@H]1CCC[C@@H]1O",
        "F[C@H]1C[C@H]2CCC1C2",
        "C1CCC2C3CCC4CCCCC4C3CCC12",
        "C1COCCN1",
        "C1CCC2=C(C1)C=CC=C2",
        "[H]C1([H])CC2(C1)CCCCC2",
        "C1C2C3C4C1C5C2C3C45",
        "CC1=CC=C2C(=C1)C=CC=C2",
    ]
    rng = random.Random(83491)
    for index, text in enumerate(texts):
        params = Chem.SmilesParserParams()
        params.removeHs = False
        original = Chem.MolFromSmiles(text, params)
        assert original is not None
        for permutation in range(2):
            order = list(range(original.GetNumAtoms()))
            if permutation:
                rng.shuffle(order)
            mol = Chem.RenumberAtoms(original, order)
            Chem.GetSymmSSSR(mol)
            state = snapshot(mol, "symmetric")
            rings = [list(r) for r in mol.GetRingInfo().AtomRings()]
            state["rings"] = {"kind": "symmetric", "atoms": rings}
            for group_index, group in enumerate(systems(rings)):
                for ordering in range(3):
                    selected = group[:]
                    if ordering == 1:
                        selected.reverse()
                    elif ordering == 2:
                        rng.shuffle(selected)
                    for length in (0.5, 1.5, 2.7):
                        done = [rng.randrange(len(selected))]
                        yield {
                            "name": f"molecule/{index}/{permutation}/{group_index}/{ordering}/{length}",
                            "state": state,
                            "selected": selected,
                            "done": done,
                            "bond_length": bits(length),
                            "construct": True,
                            "pickle": mol.ToBinary(Chem.PropertyPickleOptions.AllProps).hex(),
                        }
                    # Rotate and reverse ring traversals without changing chemistry.
                    changed = json.loads(json.dumps(state))
                    for rid in selected:
                        ring = changed["rings"]["atoms"][rid]
                        ring[:] = ring[1:] + ring[:1]
                        if ordering % 2:
                            ring.reverse()
                    yield {
                        "name": f"traversal/{index}/{permutation}/{group_index}/{ordering}",
                        "state": changed,
                        "selected": selected,
                        "done": [0],
                        "bond_length": bits(1.5),
                        "construct": True,
                        "pickle": mol.ToBinary(Chem.PropertyPickleOptions.AllProps).hex(),
                    }
                # Partial/all completed, repeated selections, empty selection,
                # and disconnected selection exercise the helpers independently.
                for label, selected, done in (
                    ("all-done", group, list(range(len(group)))),
                    ("no-done", group, []),
                    ("repeat", group + group, [0, 0]),
                    ("empty", [], []),
                    ("all-systems", list(range(len(rings))), [0]),
                ):
                    yield {
                        "name": f"selection/{index}/{permutation}/{group_index}/{label}",
                        "state": state,
                        "selected": selected,
                        "done": done,
                        "bond_length": bits(1.5),
                        "construct": label == "repeat",
                        "pickle": mol.ToBinary(Chem.PropertyPickleOptions.AllProps).hex(),
                    }
            # Direct low-level constructor behavior for E/Z versus trans/cis
            # aliases and alternative adjacent control atoms. No stereo cleanup
            # is run after these intentional metadata changes.
            for original_bond in mol.GetBonds():
                if (
                    original_bond.GetBondType() != Chem.BondType.DOUBLE
                    or not original_bond.IsInRing()
                ):
                    continue
                begin, end = original_bond.GetBeginAtomIdx(), original_bond.GetEndAtomIdx()
                left = [
                    a.GetIdx()
                    for a in mol.GetAtomWithIdx(begin).GetNeighbors()
                    if a.GetIdx() != end
                ]
                right = [
                    a.GetIdx()
                    for a in mol.GetAtomWithIdx(end).GetNeighbors()
                    if a.GetIdx() != begin
                ]
                for stereo in (2, 3, 4, 5):
                    for lcontrol in left:
                        for rcontrol in right:
                            changed = Chem.Mol(mol)
                            bond = changed.GetBondWithIdx(original_bond.GetIdx())
                            bond.SetStereoAtoms(lcontrol, rcontrol)
                            bond.SetStereo(Chem.BondStereo.values[stereo])
                            selected = next(
                                group
                                for group in systems(rings)
                                if any(begin in rings[r] and end in rings[r] for r in group)
                            )
                            yield {
                                "name": f"stereo/{index}/{permutation}/{bond.GetIdx()}/{stereo}/{lcontrol}/{rcontrol}",
                                "state": snapshot(changed, "symmetric"),
                                "selected": selected,
                                "done": [0],
                                "bond_length": bits(1.5),
                                "construct": True,
                                "pickle": changed.ToBinary(
                                    Chem.PropertyPickleOptions.AllProps
                                ).hex(),
                            }


def request(case):
    rings = [case["state"]["rings"]["atoms"][i] for i in case["selected"]]
    parts = [case["pickle"], str(int(case["construct"])), case["bond_length"], str(len(rings))]
    for ring in rings:
        parts += [str(len(ring)), *map(str, ring)]
    return " ".join([*parts, str(len(case["done"])), *map(str, case["done"])]) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--oracle", type=Path)
    parser.add_argument("--rdkit-source", type=Path)
    parser.add_argument("--fixture", type=Path, default=FIXTURE)
    parser.add_argument("--replay", action="store_true")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if not args.oracle:
        print(gzip.decompress(args.fixture.read_bytes()).decode(), end="")
        return
    import rdkit
    from rdkit import rdBase

    assert rdBase.rdkitVersion == "2026.03.6"
    package = Path(rdkit.__file__).parent
    library_dir = (
        package / ".dylibs" if platform.system() == "Darwin" else package.parent / "rdkit.libs"
    )
    libraries = sorted(
        path
        for path in library_dir.glob("*")
        if path.is_file()
        and any(
            name in path.name for name in ("RDKitDepictor", "RDKitRDGeometryLib", "RDKitRDGeneral")
        )
    )
    assert args.rdkit_source is not None
    hashes = {
        name: hashlib.sha256((args.rdkit_source / name).read_bytes()).hexdigest()
        for name in SOURCES
    }
    if args.replay:
        original = [
            json.loads(line) for line in gzip.decompress(args.fixture.read_bytes()).splitlines()
        ]
        assert original[0]["provenance"]["commit"] == PIN
        assert original[0]["provenance"]["source_sha256"] == hashes
        inputs = original[1:]
    else:
        assert (
            subprocess.check_output(
                ["git", "-C", str(args.rdkit_source), "rev-parse", "HEAD"], text=True
            ).strip()
            == PIN
        )
        inputs = list(cases())
    result = subprocess.run(
        [str(args.oracle.resolve())],
        input="".join(map(request, inputs)),
        text=True,
        capture_output=True,
        timeout=120,
        check=True,
        cwd=ROOT,
        env=os.environ,
    )
    outputs = result.stdout.splitlines()
    assert len(outputs) == len(inputs), result.stderr
    header = {
        "provenance": {
            "version": "2026.03.6",
            "commit": PIN,
            "source_sha256": hashes,
            "library_sha256": {
                path.name: hashlib.sha256(path.read_bytes()).hexdigest() for path in libraries
            },
            "platform": platform.platform(),
            "oracle_sha256": hashlib.sha256(args.oracle.read_bytes()).hexdigest(),
            "cases": len(inputs),
            "templates": False,
        }
    }
    lines = [json.dumps(header, separators=(",", ":"))]
    for case, output in zip(inputs, outputs, strict=True):
        case["expected"] = json.loads(output)
        lines.append(json.dumps(case, separators=(",", ":")))
    data = ("\n".join(lines) + "\n").encode()
    if args.output:
        args.output.write_bytes(gzip.compress(data, mtime=0))
        print(f"{len(inputs)} native ring cases; JSONL SHA256 {hashlib.sha256(data).hexdigest()}")
    else:
        print(data.decode(), end="")


if __name__ == "__main__":
    main()
