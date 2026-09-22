"""Independent native initial-fragment orchestration and merge-stage capture."""

import argparse
import gzip
import hashlib
import json
import os
import platform
import random
import struct
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PIN = "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985"
FIXTURE = (
    ROOT
    / "tests/fixtures"
    / (
        "depict-expansion-macos-native.json.gz"
        if sys.platform == "darwin"
        else "depict-expansion-windows-native.json.gz"
        if sys.platform == "win32"
        else "depict-expansion-linux-native.json.gz"
    )
)
SOURCES = (
    "Code/GraphMol/Depictor/RDDepictor.cpp",
    "Code/GraphMol/Depictor/DepictUtils.h",
    "Code/GraphMol/Depictor/EmbeddedFrag.cpp",
    "Code/GraphMol/Depictor/EmbeddedFrag.h",
)


def bits(value):
    return struct.pack(">d", value).hex()


def cases():
    from perception_reference import snapshot
    from rdkit import Chem, RDConfig

    def emit(name, mol, coordinates=None, length=1.5, templates=True, trace=True):
        mol.UpdatePropertyCache(strict=False)
        Chem.GetSymmSSSR(mol)
        return dict(
            name=name,
            state=snapshot(mol, "symmetric"),
            chiral_ranks=[
                a.GetUnsignedProp("_chiralAtomRank") if a.HasProp("_chiralAtomRank") else None
                for a in mol.GetAtoms()
            ],
            coordinates=coordinates,
            bond_length=bits(length),
            templates=templates,
            trace=trace,
            pickle=mol.ToBinary(Chem.PropertyPickleOptions.AllProps).hex(),
        )

    texts = [
        "",
        "C",
        "[H]",
        "CC",
        "CCC",
        "CCCCC",
        "CC(C)(C)CC(C)C",
        "CC#CC#N",
        "O=C=O",
        "C.C",
        "[Na+].[Cl-]",
        "CCC.CCCCC.CCC",
        "C1CC1",
        "C1CCCCC1",
        "CC1CCCCC1O",
        "c1ccccc1-c1ccccc1",
        "c1ccccc1CCc1ccccc1",
        "c1ccccc1C(C)(C)C1CC1",
        "C1CC2CCC1C2",
        "C1CCC2(CC1)CCCC2",
        "C1CCC2(CC1)CCC1(CCCCC1)C2",
        "C1C2CC3CC1CC(C2)C3",
        "C12C3C4C1C5C2C3C45",
        "C1CCCCCCCC1",
        "C1/C=C/CCCCCC1",
        "C1/C=C\\CCCCCC1",
        "C/C=C/C",
        "C/C=C\\C",
        "C/C=C/C=C/C",
        "C/C=C/C=C\\C",
        "C/C=C/C1CCCCC1",
        "C1CCCCC1/C=C/C1CCCCC1",
        "C1CCC(=C/C)CC1",
        "C1CCC(/C=C/C2CCCCC2)CC1",
        "F/C(Cl)=C(Br)/C",
        "CC[C@H](O)C",
        "C[C@H]1CCC[C@@H]1O",
        "F[C@H]1C[C@H]2CCC1C2",
        "N[Pt@SP1](Cl)(Br)I",
        "Cl[As@TB5](F)(Br)(I)N",
        "Cl[Co@OH12](F)(Br)(I)(N)O",
        "CCN[Pt@SP2](Cl)(NCCC)NCC",
        "C1CCN[Pt@SP1](NCCC)(Cl)NCC1",
        "N->[Cu+2]<-N",
        "[Cu+2]1<-NCCN->1",
        "[H]C([H])([H])C1CCCCC1",
    ]
    rng = random.Random(678102)
    params = Chem.SmilesParserParams()
    params.removeHs = False
    for index, text in enumerate(texts):
        original = Chem.MolFromSmiles(text, params)
        assert original is not None, text
        for permutation in range(2):
            order = list(range(original.GetNumAtoms()))
            if permutation:
                rng.shuffle(order)
            mol = Chem.RenumberAtoms(original, order) if order else Chem.Mol(original)
            n = mol.GetNumAtoms()
            maps = [None, []]
            if n:
                maps += [[[0, bits(4.1), bits(-2.3)]]]
            if n > 1:
                maps += [
                    [[0, bits(-1.2), bits(0.5)], [1, bits(0.3), bits(0.5)]],
                    [[0, bits(2.8), bits(-1.3)], [n - 1, bits(-1.0), bits(2.0)]],
                    [[0, bits(0.0), bits(0.0)], [n - 1, bits(0.0), bits(0.0)]],
                ]
            if n > 2:
                maps += [
                    [
                        [i, bits(x), bits(y)]
                        for i, x, y in [(0, 0.0, 0.0), (1, 1.5, 0.0), (2, 1.0, -1.3)]
                    ]
                ]
            for mi, coordinates in enumerate(maps):
                for templates in (False, True):
                    yield emit(
                        f"molecule/{index}/{permutation}/{mi}/{int(templates)}",
                        mol,
                        coordinates,
                        templates=templates,
                    )
            unassigned = Chem.Mol(mol)
            if unassigned.HasProp("_StereochemDone"):
                unassigned.ClearProp("_StereochemDone")
            for atom in unassigned.GetAtoms():
                for key in ("_CIPCode", "_CIPRank"):
                    if atom.HasProp(key):
                        atom.ClearProp(key)
            yield emit(f"unassigned-stereo/{index}/{permutation}", unassigned)
            for length in (0.25, 2.7, 0.0):
                yield emit(f"length/{index}/{permutation}/{length}", mol, length=length)
            if n:
                for prop in ("_CIPRank", "_chiralAtomRank"):
                    altered = Chem.Mol(mol)
                    for atom in altered.GetAtoms():
                        if atom.HasProp("_CIPRank"):
                            atom.ClearProp("_CIPRank")
                        atom.SetUnsignedProp(prop, 0xFFFFFFFF if atom.GetIdx() % 3 == 0 else 0)
                    yield emit(f"rank/{index}/{permutation}/{prop}", altered)
    sample = Path(RDConfig.RDDataDir) / "NCI/first_5K.smi"
    for index, line in enumerate(sample.read_text().splitlines()[:100]):
        mol = Chem.MolFromSmiles(line.split()[0], params)
        assert mol is not None, f"NCI sample {index}"
        yield emit(f"sample/{index}", mol)
    for n in (64, 256, 2048):
        mol = Chem.MolFromSmiles("C" * n)
        assert mol is not None
        yield emit(f"chain/{n}", mol, trace=False)
    for n in (64, 256):
        mol = Chem.MolFromSmiles(".".join("C" for _ in range(n)))
        assert mol is not None
        yield emit(f"disconnected/{n}", mol, trace=False)


def request(case):
    coordinates = case["coordinates"]
    parts = [
        case["pickle"],
        case["bond_length"],
        str(int(case["templates"])),
        str(int(case["trace"])),
        str(-1 if coordinates is None else len(coordinates)),
    ]
    for atom, x, y in coordinates or []:
        parts += [str(atom), x, y]
    return " ".join(parts) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--oracle", type=Path, default=os.environ.get("DEPICT_EXPANSION_ORACLE"))
    parser.add_argument(
        "--rdkit-source", type=Path, default=os.environ.get("DEPICT_EXPANSION_SOURCE")
    )
    parser.add_argument("--fixture", type=Path, default=FIXTURE)
    parser.add_argument("--replay", action="store_true")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if not args.oracle:
        print(gzip.decompress(args.fixture.read_bytes()).decode(), end="")
        return
    import rdkit
    from perception_reference import snapshot
    from rdkit import Chem, RDConfig, RDLogger, rdBase

    assert rdBase.rdkitVersion == "2026.03.6"
    RDLogger.DisableLog("rdApp.*")
    assert args.rdkit_source is not None
    hashes = {
        name: hashlib.sha256((args.rdkit_source / name).read_bytes()).hexdigest()
        for name in SOURCES
    }
    if args.replay or os.environ.get("DEPICT_EXPANSION_ORACLE"):
        previous = [
            json.loads(line) for line in gzip.decompress(args.fixture.read_bytes()).splitlines()
        ]
        assert previous[0]["provenance"]["commit"] == PIN
        assert previous[0]["provenance"]["source_sha256"] == hashes
        inputs = previous[1:]
    else:
        inputs = list(cases())
    result = subprocess.run(
        [str(args.oracle.resolve())],
        input="".join(map(request, inputs)),
        text=True,
        capture_output=True,
        check=True,
        timeout=540,
        env={**os.environ, "DYLD_LIBRARY_PATH": str(Path(rdkit.__file__).parent / ".dylibs")}
        if sys.platform == "darwin"
        else None,
    )
    outputs = result.stdout.splitlines()
    assert len(outputs) == len(inputs), (len(outputs), len(inputs), result.stderr)
    header = dict(
        provenance=dict(
            commit=PIN,
            version=rdBase.rdkitVersion,
            boost=rdBase.boostVersion,
            source_sha256=hashes,
            platform=platform.platform(),
            oracle_sha256=hashlib.sha256(args.oracle.read_bytes()).hexdigest(),
            cases=len(inputs),
            dataset_sha256=hashlib.sha256(
                (Path(RDConfig.RDDataDir) / "NCI/first_5K.smi").read_bytes()
            ).hexdigest(),
            observer_sha256=hashlib.sha256(
                (ROOT / "tests/depict_expansion_reference.cpp").read_bytes()
            ).hexdigest(),
            builder_sha256=hashlib.sha256(
                (ROOT / "tests/build_depict_expansion_oracle.py").read_bytes()
            ).hexdigest(),
            library_sha256={
                p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                for p in sorted(
                    (
                        Path(rdkit.__file__).parent / ".dylibs"
                        if sys.platform == "darwin"
                        else Path(rdkit.__file__).parent.parent / "rdkit.libs"
                    ).glob("*")
                )
                if p.is_file()
                and any(
                    name in p.name
                    for name in (
                        "RDKitDepictor",
                        "RDKitGraphMol",
                        "RDKitRDGeometryLib",
                        "RDKitRDGeneral",
                    )
                )
            },
        )
    )
    build_manifest = args.oracle.parent / "native-build.json"
    if build_manifest.is_file():
        header["provenance"]["native_build"] = json.loads(
            build_manifest.read_text(encoding="utf-8")
        )
    lines = [json.dumps(header, separators=(",", ":"))]
    errors = 0
    for case, line in zip(inputs, outputs, strict=True):
        expected = json.loads(line)
        if isinstance(expected, dict) and "adapter_initial" in expected:
            case["public_native"] = expected["public_native"]
            assert len(case["public_native"]) == 2
            expected = expected["adapter_initial"]
        if expected is not None:
            mol = Chem.Mol(bytes.fromhex(expected.pop("prepared")))
            expected["state"] = snapshot(mol, "symmetric")
        else:
            errors += 1
        case["expected"] = expected
        lines.append(json.dumps(case, separators=(",", ":")))
    content = ("\n".join(lines) + "\n").encode()
    if args.output:
        args.output.write_bytes(gzip.compress(content, mtime=0))
        print(
            len(inputs),
            "native cases;",
            errors,
            "exceptions; JSONL SHA256",
            hashlib.sha256(content).hexdigest(),
        )
        if result.stderr:
            (ROOT / "artifacts/depict-expansion-native.stderr.txt").write_text(result.stderr)
    else:
        print(content.decode(), end="")


if __name__ == "__main__":
    main()
