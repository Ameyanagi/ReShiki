"""Independent pinned C++ neighbor setup and non-ring attachment fixtures."""

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
FIXTURE = ROOT / "tests/fixtures/depict-attachment-macos-native.json.gz"
SOURCES = (
    "Code/GraphMol/Depictor/DepictUtils.cpp",
    "Code/GraphMol/Depictor/DepictUtils.h",
    "Code/GraphMol/Depictor/EmbeddedFrag.cpp",
    "Code/GraphMol/Depictor/EmbeddedFrag.h",
    "Code/RDGeneral/types.cpp",
    "Code/RDGeneral/types.h",
    "Code/Numerics/SquareMatrix.h",
    "Code/Geometry/Transform2D.cpp",
    "Code/Geometry/point.h",
)


def bits(value):
    return struct.pack(">d", value).hex()


def cases():
    from rdkit import Chem, rdBase

    if TYPE_CHECKING or __package__:
        from .perception_reference import snapshot
    else:
        from perception_reference import snapshot

    assert rdBase.rdkitVersion == "2026.03.6"
    rng = random.Random(845039)

    def emit(mol, name, mode, seed, length=1.5, patch=0):
        mol.UpdatePropertyCache(strict=False)
        Chem.GetSymmSSSR(mol)
        return dict(
            name=name,
            state=snapshot(mol, "symmetric"),
            mode=mode,
            seed=seed,
            bond_length=bits(length),
            patch=patch,
            atom_data=[
                dict(
                    hybridization=str(a.GetHybridization()),
                    cip_rank=a.GetUnsignedProp("_CIPRank") if a.HasProp("_CIPRank") else None,
                    chiral_rank=a.GetUnsignedProp("_chiralAtomRank")
                    if a.HasProp("_chiralAtomRank")
                    else None,
                )
                for a in mol.GetAtoms()
            ],
            rings=[list(r) for r in mol.GetRingInfo().AtomRings()] if mode == "ring" else [],
            pickle=mol.ToBinary(Chem.PropertyPickleOptions.AllProps).hex(),
        )

    texts = [
        "C",
        "CC",
        "CCC",
        "CCCCCCCC",
        "CC(C)C",
        "CC(C)(C)C",
        "NC(=O)CC(O)C",
        "CC#CC",
        "C=C=C",
        "C[C@H](F)[C@@H](Cl)Br",
        "[H]C([H])([H])C([H])([H])[H]",
        "CC(C)(F)C(C)(Cl)Br",
        "CC1CCC1C",
        "CC1(C)CC(C)(O)CC1N",
        "CC1=CC(C)=CC=C1O",
        "CC1CCC2(CC1)CCC(C)C2",
        "CC1CCC2CC(C)CCC2C1",
        "CC1CC2CCC1C2O",
        "C[C@H]1CCC[C@@H](O)C1",
        "CC.CCC",
        "[Na+].[Cl-]",
        "CN->[Cu]<-NC",
        "[Pt@SP1](Cl)(F)(Br)I",
        "[P@TB1](F)(Cl)(Br)(I)N",
        "[Co@OH1](F)(Cl)(Br)(I)(N)O",
    ]
    for index, text in enumerate(texts):
        params = Chem.SmilesParserParams()
        params.removeHs = False
        original = Chem.MolFromSmiles(text, params)
        assert original is not None
        for permutation in range(2):
            order = list(range(original.GetNumAtoms()))
            if permutation:
                rng.shuffle(order)
            for ranks in range(4):
                mol = Chem.RenumberAtoms(original, order)
                for atom in mol.GetAtoms():
                    for prop in ("_CIPRank", "_chiralAtomRank"):
                        if atom.HasProp(prop):
                            atom.ClearProp(prop)
                    rank = (0, 1, 0x7FFFFFFF, 0x80000000, 0xFFFFFFFF)[atom.GetIdx() % 5]
                    if ranks in (1, 3):
                        atom.SetUnsignedProp("_CIPRank", rank)
                    if ranks in (2, 3):
                        atom.SetUnsignedProp("_chiralAtomRank", rank)
                mode = "ring" if mol.GetRingInfo().NumRings() else "single"
                seed = permutation % mol.GetNumAtoms()
                for length in (0.25, 1.5, 3.25):
                    yield emit(
                        mol, f"molecule/{index}/{permutation}/{ranks}/{length}", mode, seed, length
                    )

    for degree in range(1, 9):
        for hybridization in Chem.HybridizationType.names.values():
            mol = Chem.RWMol()
            atom = Chem.Atom(6)
            atom.SetNoImplicit(True)
            atom.SetHybridization(hybridization)
            mol.AddAtom(atom)
            for index in range(degree):
                atom = Chem.Atom((1, 9, 17, 35, 53, 7, 8, 6)[index])
                atom.SetNoImplicit(True)
                mol.AddBond(0, mol.AddAtom(atom), Chem.BondType.SINGLE)
            for patch in range(5):
                yield emit(mol, f"star/{degree}/{hybridization}/{patch}", "single", 0, patch=patch)

    for text in ("C/C=C/C", "C/C=C\\C", "F/C(Cl)=C(Br)/I", "F/C(Cl)=C(Br)\\I", "CC/C=C/CCC"):
        mol = Chem.MolFromSmiles(text)
        assert mol is not None
        for bond in mol.GetBonds():
            if len(bond.GetStereoAtoms()) == 2:
                yield emit(mol, f"cis-trans/{text}", "bond", bond.GetIdx())

    mol = Chem.MolFromSmiles("CCC")
    assert mol is not None
    yield emit(mol, "native-error/zero-normal", "single", 0, patch=5)
    yield emit(mol, "native-error/zero-bond-length", "single", 0, length=0.0)
    yield emit(mol, "native-error/tiny-bond-length", "single", 0, length=1.0e-18)
    yield emit(mol, "negative-bond-length", "single", 0, length=-1.5)

    # 45,001 * 100,000 wraps u32 in the hydrogen fallback rank. Uranium's
    # unwrapped rank lies between the wrapped and unwrapped hydrogen values.
    # This catches both unsigned overflow and native pair<int,int> narrowing.
    mol = Chem.RWMol()
    for index in range(45_001):
        atom = Chem.Atom((6, 1, 92)[index % 3])
        atom.SetNoImplicit(True)
        mol.AddAtom(atom)
    yield emit(mol, "rank-wrap/45001", "single", 0)


def request(case):
    parts = [
        case["pickle"],
        case["bond_length"],
        case["mode"],
        str(case["seed"]),
        str(len(case["rings"])),
    ]
    for ring in case["rings"]:
        parts += [str(len(ring)), *map(str, ring)]
    return " ".join([*parts, str(case["patch"])]) + "\n"


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
            "observation_patch": "EmbeddedFrag.h: replace the single private: line with public:",
            "observation_header_sha256": hashlib.sha256(
                (args.rdkit_source / "Code/GraphMol/Depictor/EmbeddedFrag.h")
                .read_bytes()
                .replace(b" private:", b" public:")
            ).hexdigest(),
        }
    }
    lines = [json.dumps(header, separators=(",", ":"))]
    for case, output in zip(inputs, outputs, strict=True):
        case["expected"] = json.loads(output)
        lines.append(json.dumps(case, separators=(",", ":")))
    data = ("\n".join(lines) + "\n").encode()
    if args.output:
        args.output.write_bytes(gzip.compress(data, mtime=0))
        print(
            f"{len(inputs)} native attachment cases; JSONL SHA256 {hashlib.sha256(data).hexdigest()}"
        )
    else:
        print(data.decode(), end="")


if __name__ == "__main__":
    main()
