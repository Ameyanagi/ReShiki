"""Independent pinned C++ coordination, coordinate-map and stereobond seeds fixtures."""

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
FIXTURE = ROOT / "tests/fixtures/depict-seeds-macos-native.json.gz"
SOURCES = (
    "Code/GraphMol/Depictor/DepictUtils.cpp",
    "Code/GraphMol/Depictor/RDDepictor.cpp",
    "Code/GraphMol/NontetrahedralStereo.cpp",
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
    import math

    from rdkit import Chem, rdBase

    if TYPE_CHECKING or __package__:
        from .perception_reference import snapshot
    else:
        from perception_reference import snapshot

    assert rdBase.rdkitVersion == "2026.03.6"
    rng = random.Random(924583)

    def emit(
        mol,
        name,
        mode,
        seed=0,
        ranks=None,
        coords=(),
        lengths=(1.5, 1.5, 1.5),
        current=1.5,
        cache=True,
    ):
        mol.UpdatePropertyCache(strict=False)
        if cache:
            Chem.GetSymmSSSR(mol)
        else:
            mol.ClearComputedProps(includeRings=True)
            mol.UpdatePropertyCache(strict=False)
        return dict(
            name=name,
            state=snapshot(mol, "symmetric" if cache else "none"),
            mode=mode,
            seed=seed,
            current_length=bits(current),
            ideal_lengths=list(map(bits, lengths)),
            ranks=ranks
            if ranks is not None
            else [
                100 * (1000 if a.GetAtomicNum() == 1 else a.GetAtomicNum()) + a.GetDegree()
                for a in mol.GetAtoms()
            ],
            coordinates=[[i, bits(x), bits(y)] for i, x, y in coords],
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
            pickle=mol.ToBinary(Chem.PropertyPickleOptions.AllProps).hex(),
        )

    def star(degree, tag=0, permutation=None, tails=False):
        mol = Chem.RWMol()
        center = Chem.Atom(78)
        center.SetNoImplicit(True)
        center.SetChiralTag(Chem.ChiralType.values[tag])
        if permutation is not None:
            center.SetUnsignedProp("_chiralPermutation", permutation)
        mol.AddAtom(center)
        for index in range(degree):
            atom = Chem.Atom((9, 17, 35, 53, 7, 8, 6, 1)[index])
            atom.SetNoImplicit(True)
            mol.AddBond(0, mol.AddAtom(atom), Chem.BondType.SINGLE)
        if tails:
            for index in range(degree):
                atom = Chem.Atom(6)
                atom.SetNoImplicit(True)
                mol.AddBond(index + 1, mol.AddAtom(atom), Chem.BondType.SINGLE)
        return mol

    for mode, tag, maximum, size in (("sp", 6, 3, 4), ("tbp", 7, 20, 5), ("oh", 8, 30, 6)):
        for permutation in range(1, maximum + 1):
            for degree in range(1, size + 1):
                for rank_mode in range(2):
                    mol = star(degree, tag, permutation, True)
                    ranks = [0 if rank_mode == 0 else -i for i in range(mol.GetNumAtoms())]
                    yield emit(
                        mol,
                        f"permutation/{mode}/{permutation}/{degree}/{rank_mode}",
                        mode,
                        ranks=ranks,
                    )
        for permutation in (None, 0, maximum + 1, 0xFFFFFFFF):
            for degree in range(1, (3 if mode == "tbp" else size) + 1):
                mol = star(degree, tag, permutation, True)
                yield emit(mol, f"missing-permutation/{mode}/{permutation}/{degree}", mode)
        for lengths in ((0.25, 1.25, 3.25), (3.25, 0.25, 1.25)):
            for current in (0.5, 2.75):
                for rank_mode in range(3):
                    mol = star(size, tag, maximum, True)
                    order = list(range(mol.GetNumAtoms()))
                    rng.shuffle(order)
                    mol = Chem.RenumberAtoms(mol, order)
                    center = order.index(0)
                    ranks = [
                        (0, 0x7FFFFFFF, -0x80000000)[i % 3] if rank_mode else 0
                        for i in range(mol.GetNumAtoms())
                    ]
                    for atom in mol.GetAtoms():
                        if rank_mode == 1:
                            atom.SetUnsignedProp("_CIPRank", mol.GetNumAtoms() - atom.GetIdx())
                        if rank_mode == 2:
                            atom.SetUnsignedProp("_chiralAtomRank", atom.GetIdx())
                    yield emit(
                        mol,
                        f"length-order/{mode}/{lengths}/{current}/{rank_mode}",
                        mode,
                        center,
                        ranks,
                        lengths=lengths,
                        current=current,
                    )

    # These malformed degrees have defined native results (unlike zero-degree
    # SP or TBP with more than three unclassified equatorial neighbors).
    for mode, tag, degree, permutation in (
        ("tbp", 7, 0, 1),
        ("oh", 8, 0, 1),
        ("sp", 6, 5, 1),
        ("sp", 6, 6, 0),
        ("oh", 8, 7, 1),
        ("oh", 8, 8, 0),
    ):
        mol = star(degree, tag, permutation, True)
        yield emit(mol, f"defined-edge/{mode}/{degree}/{permutation}", mode)

    for degree in range(9):
        for completed in range(degree + 1):
            mol = star(degree)
            for variant in range(3):
                coords = [(0, 0.0, 0.0)]
                for index in range(completed):
                    angle = (index % 4) * math.pi / 2 if variant == 0 else index * math.pi / 6
                    coords.append((index + 1, math.cos(angle), math.sin(angle)))
                if variant == 2 and completed:
                    coords[-1] = (completed, 0.0, 0.0)
                yield emit(
                    mol, f"coordinates/{degree}/{completed}/{variant}", "coordinates", coords=coords
                )
            if completed >= 3 and completed < degree:
                yield emit(
                    mol,
                    f"missing-cache/{degree}/{completed}",
                    "coordinates",
                    coords=coords[:-1] + [(completed, 2.0, 3.0)],
                    cache=False,
                )
    yield emit(star(0), "coordinates/empty", "coordinates")
    yield emit(star(0), "wrong-tag/sp", "sp")
    yield emit(star(0), "wrong-tag/tbp", "tbp")
    yield emit(star(0), "wrong-tag/oh", "oh")

    # Source-native ring membership influences the attachment-angle winner.
    for text in ("CC1(C)CCC1", "CC1(C)CC2(CC1)CCCC2", "CC12CCC(CC1)CC2", "CC1CC2CCC1C2O"):
        mol = Chem.MolFromSmiles(text)
        assert mol is not None
        for center in mol.GetAtoms():
            if center.GetDegree() < 3:
                continue
            neighbors = [a.GetIdx() for a in center.GetNeighbors()]
            for removed in neighbors:
                ids = [center.GetIdx(), *(i for i in neighbors if i != removed)]
                coords = [
                    (atom, math.cos(index * 1.1), math.sin(index * 1.1))
                    for index, atom in enumerate(ids)
                ]
                yield emit(
                    mol,
                    f"ring-coordinates/{text}/{center.GetIdx()}/{removed}",
                    "coordinates",
                    coords=coords,
                )

    for text in ("F/C=C/F", "F/C(Cl)=C(Br)/I", "CC/C=C/CCC", "C/C1=C/CCCCCC1"):
        original = Chem.MolFromSmiles(text)
        assert original is not None
        for bond in original.GetBonds():
            if bond.GetBondType() != Chem.BondType.DOUBLE:
                continue
            begin = [
                a.GetIdx()
                for a in bond.GetBeginAtom().GetNeighbors()
                if a.GetIdx() != bond.GetEndAtomIdx()
            ]
            end = [
                a.GetIdx()
                for a in bond.GetEndAtom().GetNeighbors()
                if a.GetIdx() != bond.GetBeginAtomIdx()
            ]
            for stereo in range(8):
                for length in (0.25, 1.5, 3.25, -1.5):
                    mol = Chem.RWMol(original)
                    target = mol.GetBondWithIdx(bond.GetIdx())
                    target.SetStereoAtoms(begin[0], end[0])
                    target.SetStereo(Chem.BondStereo.values[stereo])
                    yield emit(
                        mol, f"bond/{text}/{stereo}/{length}", "bond", bond.GetIdx(), current=length
                    )


def request(case):
    parts = [
        *case["ideal_lengths"],
        case["current_length"],
        case["pickle"],
        case["mode"],
        str(case["seed"]),
        str(len(case["ranks"])),
        *map(str, case["ranks"]),
        str(len(case["coordinates"])),
    ]
    for index, x, y in case["coordinates"]:
        parts += [str(index), x, y]
    return " ".join(parts) + "\n"


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
    outputs = [None] * len(inputs)
    groups = {}
    for index, case in enumerate(inputs):
        groups.setdefault(tuple(case["ideal_lengths"]), []).append(index)
    for group in groups.values():
        result = subprocess.run(
            [str(args.oracle.resolve())],
            input="".join(request(inputs[i]) for i in group),
            text=True,
            capture_output=True,
            timeout=120,
            check=True,
            cwd=ROOT,
            env=os.environ,
        )
        lines = result.stdout.splitlines()
        assert len(lines) == len(group), result.stderr
        for index, line in zip(group, lines, strict=True):
            outputs[index] = line
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
        assert output is not None
        case["expected"] = json.loads(output)
        lines.append(json.dumps(case, separators=(",", ":")))
    data = ("\n".join(lines) + "\n").encode()
    if args.output:
        args.output.write_bytes(gzip.compress(data, mtime=0))
        print(f"{len(inputs)} native seed cases; JSONL SHA256 {hashlib.sha256(data).hexdigest()}")
    else:
        print(data.decode(), end="")


if __name__ == "__main__":
    main()
