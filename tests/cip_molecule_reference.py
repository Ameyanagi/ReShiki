"""Replay native CIPMol results against fresh pinned-RDKit input states.

Regenerate with --oracle PATH --write-fixture. The C++ helper calls RDKit's
exported CIPMol routines; it contains no copied chemistry implementation.
"""

import argparse
import contextlib
import gzip
import hashlib
import json
import random
import subprocess
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDConfig, RDLogger, rdBase

if TYPE_CHECKING or __package__:
    from .perception_reference import snapshot
else:
    from perception_reference import snapshot

FIXTURE = Path(__file__).parent / "fixtures/cip-molecule-native.json.gz"


def molecules():
    texts = [
        t["smiles"]
        for t in json.loads((Path(__file__).parents[1] / "assets/templates.json").read_text())
    ]
    texts += [
        "c1nccnc1",
        "[n-]1nnnc1",
        "c1cc[nH]c1",
        "[o+]1ccccc1",
        "C[C@H](c1ncccc1)O",
        "[13cH:5]1ncccc1",
        "c1cccc1",
        "c1ccc1",
        "*1ccccc1",
        "[n-]1cccc1",
        "c1nccnc1.c1cc[nH]c1",
        "",
        "N->[Cu]",
        "C~C",
    ]
    texts += [
        line.split()[0]
        for line in (Path(RDConfig.RDDataDir) / "NCI/first_5K.smi").read_text().splitlines()
    ]
    rng = random.Random(28411)
    for i, text in enumerate(texts):
        mol = Chem.MolFromSmiles(text, sanitize=False)
        if mol is None:
            continue
        try:
            Chem.SanitizeMol(mol)
        except (ValueError, RuntimeError):
            mol.UpdatePropertyCache(strict=False)
        # States need supported editor bond types. UNSPECIFIED is intentionally
        # outside the graph contract and is covered by graph-validation tests.
        if any(b.GetBondType() == Chem.BondType.UNSPECIFIED for b in mol.GetBonds()):
            continue
        Chem.GetSymmSSSR(mol)
        yield f"molecule/{i}", mol, "symmetric"
        if i % 5 == 0 and mol.GetNumAtoms():
            order = list(range(mol.GetNumAtoms()))
            rng.shuffle(order)
            yield f"permuted/{i}", Chem.RenumberAtoms(mol, order), "symmetric"
        if i < 120 or i % 13 == 0:
            for kind in ("fast", "none"):
                changed = Chem.Mol(mol)
                changed.ClearComputedProps(includeRings=True)
                changed.UpdatePropertyCache(strict=False)
                if kind == "fast":
                    Chem.FastFindRings(changed)
                yield f"{kind}/{i}", changed, kind
    for n in (6, 7, 8, 14, 15, 16, 32, 33):
        for charge in (-1, 0, 1):
            for size in (3, 5, 6, 7):
                mol = Chem.RWMol()
                for i in range(size):
                    a = Chem.Atom(n if i == 0 else 6)
                    a.SetIsAromatic(True)
                    a.SetFormalCharge(charge if i == 0 else 0)
                    mol.AddAtom(a)
                for i in range(size):
                    mol.AddBond(i, (i + 1) % size, Chem.BondType.AROMATIC)
                mol.UpdatePropertyCache(strict=False)
                Chem.GetSymmSSSR(mol)
                yield f"aromatic/{n}/{charge}/{size}", mol, "symmetric"
    for kind in (
        Chem.BondType.HYDROGEN,
        Chem.BondType.DATIVE,
        Chem.BondType.ONEANDAHALF,
        Chem.BondType.QUADRUPLE,
    ):
        mol = Chem.MolFromSmiles("CC", sanitize=False)
        mol.GetBondWithIdx(0).SetBondType(kind)
        mol.UpdatePropertyCache(strict=False)
        Chem.GetSymmSSSR(mol)
        yield f"bond/{kind}", mol, "symmetric"
    # Bond order and atom/bond aromatic flags are independent in imported states.
    # Keep inconsistent and chemically invalid combinations in the oracle.
    for order in (
        Chem.BondType.SINGLE,
        Chem.BondType.DOUBLE,
        Chem.BondType.AROMATIC,
        Chem.BondType.ONEANDAHALF,
    ):
        for atom_aromatic in (False, True):
            for bond_aromatic in (False, True):
                for cache in ("none", "fast", "symmetric"):
                    mol = Chem.MolFromSmiles("C1CCNCC1", sanitize=False)
                    for a in mol.GetAtoms():
                        a.SetIsAromatic(atom_aromatic)
                    for b in mol.GetBonds():
                        b.SetBondType(order)
                        b.SetIsAromatic(bond_aromatic)
                    mol.UpdatePropertyCache(strict=False)
                    if cache == "fast":
                        Chem.FastFindRings(mol)
                    elif cache == "symmetric":
                        Chem.GetSymmSSSR(mol)
                    yield f"flags/{order}/{atom_aromatic}/{bond_aromatic}/{cache}", mol, cache


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
            # Never leave a native helper waiting for input after an assertion.
            stack.callback(child.kill)
        if not args.write_fixture:
            print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
        for name, mol, kind in molecules():
            before = snapshot(mol, kind)
            digest = hashlib.sha256(
                json.dumps(before, sort_keys=True, separators=(",", ":")).encode()
            ).hexdigest()
            for order in ("bonds", "rings", "fractions"):
                key = f"{name}/{order}"
                if child:
                    assert child.stdin and child.stdout
                    child.stdin.write(
                        order + " " + mol.ToBinary(Chem.PropertyPickleOptions.AllProps).hex() + "\n"
                    )
                    child.stdin.flush()
                    expected = json.loads(child.stdout.readline())
                    if args.write_fixture:
                        records[key] = [digest, expected]
                        continue
                else:
                    recorded, expected = records.pop(key)
                    assert recorded == digest, f"Native input changed: {key}"
                print(json.dumps(dict(name=key, before=before, order=order, expected=expected)))
        if child:
            assert child.stdin
            child.stdin.close()
            assert child.wait() == 0
    if args.write_fixture:
        data = json.dumps(records, separators=(",", ":")).encode()
        FIXTURE.write_bytes(gzip.compress(data, mtime=0))
        print(f"Recorded {len(records)} native CIP molecule cases ({FIXTURE.stat().st_size} bytes)")
    elif not args.oracle:
        assert not records, "Unused native fixtures"


if __name__ == "__main__":
    main()
