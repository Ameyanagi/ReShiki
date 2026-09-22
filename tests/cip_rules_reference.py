"""Fresh RDKit inputs with independently recorded native sequence-rule results."""

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
    from .cip_digraph_reference import inputs
    from .perception_reference import snapshot
else:
    from cip_digraph_reference import inputs
    from perception_reference import snapshot

FIXTURE = Path(__file__).parent / "fixtures/cip-rules-native.json.gz"


def cases():
    for index, (name, mol, kind, root, atrop) in enumerate(inputs()):
        if name.startswith("dense/"):
            continue
        # Original, permuted, special-bond and inconsistent aromatic states.
        schemes = [8, index % 8, 9]
        if name.startswith("special/"):
            schemes = list(range(10))
        for scheme in schemes:
            for operation in range(3):
                yield (
                    mol,
                    dict(
                        name=f"{name}/{scheme}/{operation}",
                        before=snapshot(mol, kind),
                        root=root,
                        atrop=atrop,
                        seed=index * 31 + 17,
                        scheme=scheme,
                        aux=index % 3,
                        reroot=index % 4 == 0,
                        deep=(index + operation) % 2 == 0,
                        budget=[2000, 2000, 2000, 1, 2, 7, 31, 0][index % 8],
                        operation=operation,
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
                keys = (
                    "root",
                    "atrop",
                    "seed",
                    "scheme",
                    "aux",
                    "reroot",
                    "deep",
                    "budget",
                    "operation",
                )
                params = " ".join(str(int(case[key])) for key in keys)
                binary = mol.ToBinary(Chem.PropertyPickleOptions.AllProps).hex()
                child.stdin.write(f"{params} {binary}\n")
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
        print(f"Recorded {len(records)} native CIP rule cases ({FIXTURE.stat().st_size} bytes)")
    elif not args.oracle:
        assert not records, "Unused native fixtures"


if __name__ == "__main__":
    main()
