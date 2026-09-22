"""Replay direct native CIP Digraph observations against fresh RDKit states."""

import argparse
import contextlib
import gzip
import hashlib
import json
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

FIXTURE = Path(__file__).parent / "fixtures/cip-digraph-native.json.gz"


def inputs():
    for name, mol, kind in molecules():
        if not mol.GetNumAtoms():
            continue
        group, _, rest = name.partition("/")
        if group in ("molecule", "permuted", "fast", "none"):
            index = int(rest)
            if group == "molecule" and index >= 120 and index % 37:
                continue
            if group == "permuted" and index % 185:
                continue
            if group in ("fast", "none") and index >= 40:
                continue
        roots = [0]
        if group == "molecule" and int(rest) < 120 and mol.GetNumAtoms() > 1:
            roots.append(mol.GetNumAtoms() // 2)
        for root in roots:
            for atrop in (False, True):
                yield f"{name}/{root}/{atrop}", mol, kind, root, atrop
    texts = [
        "CC1(OC2=C(C=3NC[C@@]4(C3C=C2)C([C@@H]5C[C@@]67C(N([C@]5(C4)CN6CC[C@@]7(C)O)C)=O)(C)C)OC=C1)C",
        "C1C2CC3CC1CC(C2)C3",
        "C12C3C4C1C5C2C3C45",
        "[13CH3][C@H]([14CH3])[18OH]",
        "[999CH3][C@H]([3H])[13*]",
        "[999*]",
        "[13NH3+][C@H](C)O",
        "[2H]O[3H]",
        "[He]",
        "N->[Cu]<-N.C",
        "[C]" * 300,
        "[C]1" + "[C]" * 298 + "[C]1",
    ]
    for i, text in enumerate(texts):
        mol = Chem.MolFromSmiles(text)
        assert mol is not None
        for root in sorted({0, min(1, mol.GetNumAtoms() - 1)}):
            for atrop in (False, True):
                yield f"special/{i}/{root}/{atrop}", mol, "symmetric", root, atrop
    # Dense low-level graphs exercise the native expansion limit independently
    # of the editor's strict valence check.
    for size in (9, 10):
        mol = Chem.RWMol()
        for _ in range(size):
            atom = Chem.Atom(6)
            atom.SetNoImplicit(True)
            mol.AddAtom(atom)
        for a in range(size):
            for b in range(a + 1, size):
                mol.AddBond(a, b, Chem.BondType.SINGLE)
        mol.UpdatePropertyCache(strict=False)
        Chem.FastFindRings(mol)
        for atrop in (False, True):
            yield f"dense/{size}/{atrop}", mol, "fast", 0, atrop


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
        for index, (name, mol, kind, root, atrop) in enumerate(inputs()):
            before = snapshot(mol, kind)
            seed = index * 31 + 17
            case = dict(name=name, before=before, root=root, atrop=atrop, seed=seed)
            digest = hashlib.sha256(
                json.dumps(case, sort_keys=True, separators=(",", ":")).encode()
            ).hexdigest()
            if child:
                assert child.stdin and child.stdout
                binary = mol.ToBinary(Chem.PropertyPickleOptions.AllProps).hex()
                child.stdin.write(f"{root} {int(atrop)} {seed} {binary}\n")
                child.stdin.flush()
                expected = json.loads(child.stdout.readline())
                if args.write_fixture:
                    records[name] = [digest, expected]
                    continue
            else:
                recorded, expected = records.pop(name)
                assert recorded == digest, f"Native input changed: {name}"
            print(json.dumps({**case, "expected": expected}))
        if child:
            assert child.stdin
            child.stdin.close()
            assert child.wait() == 0
    if args.write_fixture:
        FIXTURE.write_bytes(
            gzip.compress(json.dumps(records, separators=(",", ":")).encode(), mtime=0)
        )
        print(f"Recorded {len(records)} native CIP digraph cases ({FIXTURE.stat().st_size} bytes)")
    elif not args.oracle:
        assert not records, "Unused native fixtures"


if __name__ == "__main__":
    main()
