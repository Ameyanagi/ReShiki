"""Direct pinned-native configuration labels, carriers, mutations and graphs."""

import argparse
import contextlib
import gzip
import hashlib
import itertools
import json
import random
import subprocess
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDLogger, rdBase

if TYPE_CHECKING or __package__:
    from .cip_molecule_reference import molecules
    from .perception_reference import snapshot
else:
    from cip_molecule_reference import molecules
    from perception_reference import snapshot

FIXTURE = Path(__file__).parent / "fixtures/cip-configuration-native.json.gz"
KEYS = (
    "kind",
    "target",
    "cfg",
    "origin",
    "node_atom",
    "seed",
    "full",
    "aux",
    "budget",
    "repeat",
    "reverse",
    "write",
)


def inputs():
    texts = [
        "N[C@@H](C)C(=O)O",
        "F[C@](Cl)(Br)I",
        "[1H][C@H](F)Cl",
        "C[S@H]=O",
        "C[S@](=O)CC",
        "C[P@](F)Cl",
        "C[C@H](C)O",
        "C[C@](C)(C)C",
        "C[C@](CC)(CC)C",
        "[C@]12(CCCCC1)CCCCC2",
        "C[C@H]1CCCCO1",
        "F/C=C/F",
        "Cl/C(F)=C(/Br)I",
        "[1H]/C=C/F",
        "C/C=C/C=C/C",
        "C/C=C\\C",
        "F/C=C(/[C@H](F)Cl)[C@@H](F)Cl",
        "CC(Cl)=C(C)CC",
        "CC(F)C(C)C(Br)C",
        "c1ccccc1-c1ncccc1",
        "Cc1cccc(Br)c1-c1c(Cl)cccc1F",
        "CC",
        "C1C2CC3CC1CC(C2)C3",
        "[C@H](F)Cl",
        "[C@](F)(Cl)Br",
        "[P@H](F)Cl",
        "[C@H3]F",
        "[C@]",
        "[C@](F)(F)(F)F",
        "FC(F)C(Cl)(Br)C(F)F",
        "F[C@]([C@H](F)Cl)([C@@H](F)Cl)Br",
        "C[C@]([C@H](F)Cl)([C@@H](F)Cl)C",
        "F[C@H](Cl)C(C)=C([C@H](F)Cl)[C@@H](F)Cl",
        "F[C@H](Cl)C([C@@H](F)Cl)=C([C@H](F)Cl)[C@@H](F)Cl",
        "F[C@H](Cl)C(C)C([C@H](F)Cl)[C@@H](F)Cl",
        "F[C@H](Cl)C([C@@H](F)Cl)C([C@H](F)Cl)[C@@H](F)Cl",
    ]
    rng = random.Random(51045)
    for i, text in enumerate(texts):
        params = Chem.SmilesParserParams()
        params.removeHs = False
        params.sanitize = False
        mol = Chem.MolFromSmiles(text, params)
        assert mol is not None
        try:
            Chem.SanitizeMol(mol)
        except (ValueError, RuntimeError):
            mol.UpdatePropertyCache(strict=False)
        Chem.GetSymmSSSR(mol)
        # Preserve low-level tags that a complete stereo cleanup may remove.
        yield f"special/{i}", mol, "symmetric"
        order = list(range(mol.GetNumAtoms()))
        rng.shuffle(order)
        yield f"permuted/{i}", Chem.RenumberAtoms(mol, order), "symmetric"
    for name, mol, cache in molecules():
        if name.startswith("molecule/") and int(name.split("/")[1]) < 160:
            yield name, mol, cache


def configurations():
    for name, source, cache in inputs():
        targets = [(0, a.GetIdx()) for a in source.GetAtoms() if a.GetDegree() in (2, 3, 4)]
        targets += [
            (1, b.GetIdx()) for b in source.GetBonds() if b.GetBondType() == Chem.BondType.DOUBLE
        ]
        targets += [
            (2, b.GetIdx())
            for b in source.GetBonds()
            if b.GetBondType() == Chem.BondType.SINGLE
            and b.GetBeginAtom().GetDegree() > 1
            and b.GetEndAtom().GetDegree() > 1
        ]
        if name.startswith("molecule/"):
            targets = targets[:3]
        for kind, target in targets:
            mol = Chem.Mol(source)
            cfg = 1
            if kind == 0:
                atom = mol.GetAtomWithIdx(target)
                if atom.GetChiralTag() not in (
                    Chem.ChiralType.CHI_TETRAHEDRAL_CW,
                    Chem.ChiralType.CHI_TETRAHEDRAL_CCW,
                ):
                    atom.SetChiralTag(Chem.ChiralType.CHI_TETRAHEDRAL_CW)
                focus = target
            else:
                bond = mol.GetBondWithIdx(target)
                focus = bond.GetBeginAtomIdx()
                ends = [bond.GetBeginAtom(), bond.GetEndAtom()]
                nbrs = [
                    [a.GetIdx() for a in end.GetNeighbors() if a.GetIdx() != ends[1 - i].GetIdx()]
                    for i, end in enumerate(ends)
                ]
                if kind == 1:
                    if not all(nbrs):
                        continue
                    bond.SetStereoAtoms(nbrs[0][0], nbrs[1][0])
                    cfg = 4 + target % 2
                    bond.SetStereo(Chem.BondStereo.values[cfg])
                else:
                    cfg = 6 + target % 2
                    bond.SetStereo(Chem.BondStereo.values[cfg])
            yield name, mol, cache, kind, target, cfg, focus


def cases():
    for index, (name, mol, cache, kind, target, cfg, focus) in enumerate(configurations()):
        variants = range(9) if name.startswith(("special/", "permuted/")) else range(3)
        for variant in variants:
            changed = Chem.Mol(mol)
            if variant == 4:
                obj = (
                    changed.GetAtomWithIdx(target) if kind == 0 else changed.GetBondWithIdx(target)
                )
                obj.SetProp("_CIPCode", "legacy")
            reverse = variant in (6, 8) and kind != 0
            origin = (focus + 1) % changed.GetNumAtoms() if variant in (2, 3) else -1
            node_atom = focus if origin >= 0 else -1
            if variant == 3 and kind != 0:
                node_atom = changed.GetBondWithIdx(target).GetEndAtomIdx()
            yield (
                changed,
                dict(
                    name=f"{name}/{kind}/{target}/{variant}",
                    before=snapshot(changed, cache),
                    kind=kind,
                    target=target,
                    cfg=cfg,
                    origin=origin,
                    node_atom=node_atom,
                    seed=index * 31 + variant * 7,
                    full=variant != 0,
                    aux=3 if variant >= 7 else 1 if variant == 3 else 2 if variant in (4, 6) else 0,
                    budget=1 if variant == 5 else 3000,
                    repeat=variant in (1, 4),
                    reverse=reverse,
                    write=-1,
                ),
            )
    # Explicit constructor and primary-label rejection, missing inferred controls,
    # and native implicit-H/phantom centers with too few incident graph edges.
    for kind, text, target in (
        (0, "[C]", 0),
        (0, "CF", 0),
        (0, "C(F)Cl", 0),
        (1, "FC=CF", 1),
        (2, "CC", 0),
    ):
        for variant in range(20):
            mol = Chem.MolFromSmiles(text)
            assert mol is not None
            cfg = 4 if kind == 1 else 6
            if kind == 0:
                mol.GetAtomWithIdx(target).SetChiralTag(Chem.ChiralType.CHI_TETRAHEDRAL_CW)
            elif kind == 1:
                bond = mol.GetBondWithIdx(target)
                bond.SetStereo(Chem.BondStereo.STEREOE)
                if variant % 3:
                    for atom in mol.GetAtoms():
                        atom.SetUnsignedProp("_CIPRank", atom.GetIdx())
            yield (
                mol,
                dict(
                    name=f"invalid/{kind}/{text}/{variant}",
                    before=snapshot(mol, "symmetric"),
                    kind=kind,
                    target=target,
                    cfg=cfg,
                    origin=-1,
                    node_atom=-1,
                    seed=0,
                    full=True,
                    aux=0,
                    budget=1000,
                    repeat=False,
                    reverse=False,
                    write=variant % 18,
                ),
            )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--oracle", type=Path)
    parser.add_argument("--write-fixture", action="store_true")
    args = parser.parse_args()
    RDLogger.DisableLog("rdApp.*")
    assert rdBase.rdkitVersion == "2026.03.6"
    if args.write_fixture and not args.oracle:
        parser.error("--write-fixture requires --oracle")
    records = {} if args.write_fixture else json.loads(gzip.decompress(FIXTURE.read_bytes()))
    with contextlib.ExitStack() as stack:
        child = None
        if args.oracle:
            child = stack.enter_context(
                subprocess.Popen(
                    [str(args.oracle)],
                    stdin=subprocess.PIPE,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.DEVNULL,
                    text=True,
                )
            )
            stack.callback(child.kill)
        if not args.write_fixture:
            print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
        for mol, case in cases():
            name = case["name"]
            digest = hashlib.sha256(
                json.dumps(case, sort_keys=True, separators=(",", ":")).encode()
            ).hexdigest()
            if child:
                assert child.stdin and child.stdout
                params = " ".join(str(int(case[key])) for key in KEYS)
                child.stdin.write(
                    f"{params} {mol.ToBinary(Chem.PropertyPickleOptions.AllProps).hex()}\n"
                )
                child.stdin.flush()
                line = child.stdout.readline()
                if not line:
                    assert child.wait() < 0, name
                    expected = {"error": "native_crash"}
                    child = stack.enter_context(
                        subprocess.Popen(
                            [str(args.oracle)],
                            stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE,
                            stderr=subprocess.DEVNULL,
                            text=True,
                        )
                    )
                    stack.callback(child.kill)
                else:
                    expected = json.loads(line)
                if args.write_fixture:
                    records[name] = [digest, expected]
                    continue
            else:
                recorded, expected = records.pop(name)
                assert recorded == digest, f"Native input changed: {name}"
            print(json.dumps({**case, "expected": expected}))
        # Exhaustively include repeated elements as well as all permutations.
        parity = []
        for a, b in itertools.product(range(256), repeat=2):
            if child:
                assert child.stdin and child.stdout
                child.stdin.write(f"3 {a} {b}\n")
                child.stdin.flush()
                parity.append(int(child.stdout.readline()))
        if args.write_fixture:
            records["parity"] = parity
        else:
            print(json.dumps(dict(parity=parity if child else records.pop("parity"))))
        if child:
            assert child.stdin
            child.stdin.close()
            assert child.wait() == 0
    if args.write_fixture:
        FIXTURE.write_bytes(
            gzip.compress(json.dumps(records, separators=(",", ":")).encode(), mtime=0)
        )
        print(
            f"Recorded {len(records) - 1} native configurations and {len(records['parity'])} parities ({FIXTURE.stat().st_size} bytes)"
        )
    elif not args.oracle:
        assert not records, "Unused native fixtures"


if __name__ == "__main__":
    main()
