"""Independent original C++ builtin matching and ring-construction captures."""

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

PIN = "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985"
ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests/fixtures/depict-templates-linux-native.json.gz"
SOURCES = (
    "Code/GraphMol/Depictor/TemplateSmarts.h",
    "Code/GraphMol/Depictor/Templates.h",
    "Code/GraphMol/Depictor/EmbeddedFrag.cpp",
    "Code/GraphMol/Depictor/EmbeddedFrag.h",
)


def bits(value):
    return struct.pack(">d", value).hex()


def cases(source):
    from depict_rings_reference import systems
    from perception_reference import snapshot
    from rdkit import Chem

    def emit(name, mol, selected=None, atoms=None, length=1.5):
        mol.UpdatePropertyCache(strict=False)
        Chem.GetSymmSSSR(mol)
        state = snapshot(mol, "symmetric")
        selected = selected if selected is not None else list(range(len(state["rings"]["atoms"])))
        atoms = (
            atoms
            if atoms is not None
            else list(dict.fromkeys(a for i in selected for a in state["rings"]["atoms"][i]))
        )
        return dict(
            name=name,
            state=state,
            selected=selected,
            atoms=atoms,
            bond_length=bits(length),
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

    strings = [
        json.loads(line.strip().rstrip(","))
        for line in (source / SOURCES[0]).read_text().splitlines()
        if line.lstrip().startswith('"')
    ]
    assert len(strings) == 578
    rng = random.Random(591765)
    for ordinal, text in enumerate(strings):
        query = Chem.MolFromSmarts(text)
        assert query is not None
        original = Chem.RWMol()
        for _ in query.GetAtoms():
            atom = Chem.Atom(6)
            atom.SetNoImplicit(True)
            original.AddAtom(atom)
        for bond in query.GetBonds():
            original.AddBond(bond.GetBeginAtomIdx(), bond.GetEndAtomIdx(), Chem.BondType.SINGLE)
        for permutation in range(2):
            order = list(range(original.GetNumAtoms()))
            if permutation:
                rng.shuffle(order)
            mol = Chem.RenumberAtoms(original, order)
            yield emit(f"builtin/{ordinal}/{permutation}", mol)
        # Outside neighbors are masked out of induced graph filters but retain
        # their contribution to exact-degree predicates and attachment setup.
        mol = Chem.RWMol(original)
        for atom in query.GetAtoms():
            if "D2" in atom.GetSmarts():
                added = mol.AddAtom(Chem.Atom(8))
                mol.AddBond(atom.GetIdx(), added, Chem.BondType.SINGLE)
                break
        yield emit(f"outside-degree/{ordinal}", mol, atoms=list(range(original.GetNumAtoms())))
        if ordinal % 7 == 0:
            edge = next(
                (
                    b
                    for b in query.GetBonds()
                    if b.GetBeginAtom().GetSmarts() == "[!#200]"
                    and b.GetEndAtom().GetSmarts() == "[!#200]"
                ),
                None,
            )
            if edge is not None:
                # More than 50 total atoms rules out a full builtin. The outer
                # 52-member ring shares an edge and can be pruned before the
                # native core-template attempt; its remaining arc is merged.
                grown = Chem.RWMol(original)
                previous = edge.GetBeginAtomIdx()
                for _ in range(50):
                    atom = Chem.Atom(6)
                    atom.SetNoImplicit(True)
                    current = grown.AddAtom(atom)
                    grown.AddBond(previous, current, Chem.BondType.SINGLE)
                    previous = current
                grown.AddBond(previous, edge.GetEndAtomIdx(), Chem.BondType.SINGLE)
                yield emit(f"forced-core/{ordinal}", grown)

    for text in (
        "C1CC1",
        "C1CCCCC1",
        "C1CCCCCCCC1",
        "C1CCCCCCCCCCC1",
        "C1CCC2(CC1)CCCC2",
        "C1CC2CCC1C2",
        "C1C2CC3CC1CC(C2)C3",
        "CC1CCC2CCCCC2C1",
        "C1CCC2C3CCC4CCCCC4C3CCC12",
        "C1CC2CCC3C4CCCC4CCC3C2C1",
        "C1CCCCC1.C1CCCCCCCC1",
        "C1/C=C/CCCCCC1",
        "C1/C=C\\CCCCCC1",
        "C/C1=C/CCCCCC1",
        "C/C1=C\\CCCCCC1",
        "F/C(Cl)=C(Br)/CC1CCCCC1",
    ):
        mol = Chem.MolFromSmiles(text)
        assert mol is not None
        Chem.GetSymmSSSR(mol)
        for selected in systems(mol.GetRingInfo().AtomRings()):
            for length in (0.25, 1.5, 3.25, 0.0):
                yield emit(f"system/{text}/{selected}/{length}", mol, selected, length=length)
            for bond in mol.GetBonds():
                if not bond.IsInRing() or bond.GetBondType() != Chem.BondType.DOUBLE:
                    continue
                a, b = bond.GetBeginAtomIdx(), bond.GetEndAtomIdx()
                left = [n.GetIdx() for n in mol.GetAtomWithIdx(a).GetNeighbors() if n.GetIdx() != b]
                right = [
                    n.GetIdx() for n in mol.GetAtomWithIdx(b).GetNeighbors() if n.GetIdx() != a
                ]
                for lcontrol in left:
                    for rcontrol in right:
                        for stereo in (0, 1, 2, 3, 4, 5):
                            changed = Chem.Mol(mol)
                            edge = changed.GetBondWithIdx(bond.GetIdx())
                            edge.SetStereoAtoms(lcontrol, rcontrol)
                            edge.SetStereo(Chem.BondStereo.values[stereo])
                            yield emit(
                                f"stereo/{text}/{bond.GetIdx()}/{lcontrol}/{rcontrol}/{stereo}",
                                changed,
                                selected,
                            )


def request(case):
    rings = [case["state"]["rings"]["atoms"][i] for i in case["selected"]]
    parts = [
        case["pickle"],
        case["bond_length"],
        str(len(case["atoms"])),
        *map(str, case["atoms"]),
        str(len(rings)),
    ]
    for ring in rings:
        parts += [str(len(ring)), *map(str, ring)]
    parts.append(str(int(case["name"].startswith(("stereo/", "system/")))))
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
    from rdkit import RDLogger, rdBase

    assert rdBase.rdkitVersion == "2026.03.6"
    RDLogger.DisableLog("rdApp.*")
    assert args.rdkit_source is not None
    hashes = {p: hashlib.sha256((args.rdkit_source / p).read_bytes()).hexdigest() for p in SOURCES}
    if args.replay:
        previous = [
            json.loads(line) for line in gzip.decompress(args.fixture.read_bytes()).splitlines()
        ]
        assert previous[0]["provenance"]["commit"] == PIN
        assert previous[0]["provenance"]["source_sha256"] == hashes
        inputs = previous[1:]
    else:
        inputs = list(cases(args.rdkit_source))
    package = Path(rdkit.__file__).parent
    libraries = (
        package / ".dylibs" if platform.system() == "Darwin" else package.parent / "rdkit.libs"
    )
    environment = dict(os.environ)
    if platform.system() == "Darwin":
        environment["DYLD_LIBRARY_PATH"] = str(libraries)
    elif platform.system() == "Windows":
        environment["PATH"] = str(libraries) + os.pathsep + environment.get("PATH", "")
    else:
        environment["LD_LIBRARY_PATH"] = str(libraries)
    result = subprocess.run(
        [str(args.oracle.resolve())],
        input="".join(map(request, inputs)),
        text=True,
        capture_output=True,
        check=True,
        timeout=240,
        env=environment,
    )
    outputs = result.stdout.splitlines()
    assert len(outputs) == len(inputs), (len(outputs), len(inputs), result.stderr)
    header = dict(
        provenance=dict(
            commit=PIN,
            version=rdBase.rdkitVersion,
            source_sha256=hashes,
            platform=platform.platform(),
            oracle_sha256=hashlib.sha256(args.oracle.read_bytes()).hexdigest(),
            library_sha256={
                p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                for p in libraries.glob("*RDKit*")
                if p.is_file()
                and any(s in p.name for s in ("Depictor", "SubstructMatch", "SmilesParse"))
            },
            cases=len(inputs),
            templates=True,
        )
    )
    build = args.oracle.with_suffix(".build.json")
    if build.exists():
        header["provenance"]["native_build"] = json.loads(build.read_text())
    lines = [json.dumps(header, separators=(",", ":"))]
    for case, line in zip(inputs, outputs, strict=True):
        case["expected"] = json.loads(line)
        lines.append(json.dumps(case, separators=(",", ":")))
    content = ("\n".join(lines) + "\n").encode()
    if args.output:
        args.output.write_bytes(gzip.compress(content, mtime=0))
        print(
            len(inputs), "native template cases; JSONL SHA256", hashlib.sha256(content).hexdigest()
        )
    else:
        print(content.decode(), end="")


if __name__ == "__main__":
    main()
