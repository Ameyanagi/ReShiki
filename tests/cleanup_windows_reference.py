"""Independent original cleanup capture for the Windows x64 CRT math profiles."""

import argparse
import copy
import ctypes
import hashlib
import json
import math
import os
import platform
import struct
import sys
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("output", type=Path)
parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
args = parser.parse_args()
ROOT = args.root
if sys.platform != "win32" or platform.machine().lower() not in ("amd64", "x86_64"):
    raise RuntimeError("Capture using the pinned Windows x64 CPython")
sys.path[:0] = [str(ROOT), str(ROOT / "tests")]
from cleanup_reference import distort, f32
from rdkit import RDLogger, rdBase

from engine import cleanup, worker

RDLogger.DisableLog("rdApp.*")
crt = ctypes.CDLL("ucrtbase")
original_fma = crt._get_FMA3_enable()


def bits(value):
    return struct.pack(">d", value).hex()


def trace(mol, old, fixed, keep):
    new = list(mol.GetConformer().GetPositions())
    if fixed:
        pivot = next(iter(fixed))
        bx, by = old[pivot].x, old[pivot].y
        ax, ay = map(float, new[pivot][:2])
    else:
        bx = sum(p.x for p in old) / len(old)
        by = sum(p.y for p in old) / len(old)
        ax = sum(float(p[0]) for p in new) / len(new)
        ay = sum(float(p[1]) for p in new) / len(new)
    dot_terms = [
        (float(p[0]) - ax) * (q.x - bx) + (float(p[1]) - ay) * (q.y - by) for p, q in zip(new, old)
    ]
    cross_terms = [
        (float(p[0]) - ax) * (q.y - by) - (float(p[1]) - ay) * (q.x - bx) for p, q in zip(new, old)
    ]
    dot, cross = sum(dot_terms), sum(cross_terms)
    angle = math.atan2(cross, dot) if keep and abs(dot) + abs(cross) > 1e-12 else 0.0
    sine, cosine = math.sin(angle), math.cos(angle)
    scalars = dict(
        ax=ax, ay=ay, bx=bx, by=by, dot=dot, cross=cross, angle=angle, sine=sine, cosine=cosine
    )
    return dict(
        old=[dict(x=p.x, y=p.y, z=p.z) for p in old],
        new=[dict(x=float(p[0]), y=float(p[1]), z=float(p[2])) for p in new],
        fixed={str(i): dict(x=p.x, y=p.y) for i, p in fixed.items()},
        keep=keep,
        scalars=scalars,
        bits={k: bits(v) for k, v in scalars.items()},
        dot_terms=dot_terms,
        cross_terms=cross_terms,
    )


profiles = []
original_orient = cleanup.orient
original_analyze = worker.analyze
for fma in [0, 1]:
    if crt._set_FMA3_enable(fma) != fma:
        raise RuntimeError("This capture requires both native CRT profiles")
    doc = worker.handle(
        dict(protocol=1, operation="import", format="smiles", text="F[C@](Cl)(Br)I")
    )["document"]
    distort(doc, 117)
    angle = 0.81
    for atom in doc["atoms"]:
        x, y = atom["position"].values()
        atom["position"] = dict(
            x=x * math.cos(angle) - y * math.sin(angle), y=x * math.sin(angle) + y * math.cos(angle)
        )
    request = dict(
        protocol=1,
        operation="clean",
        document=f32(doc),
        cleanup=dict(scope="selected_atoms", keep_orientation=True),
        selected_ids=[2, 3, 4, 5],
    )
    before = copy.deepcopy(request)
    expected = worker.handle(request)
    assert request == before
    stages = []
    analysis = []

    def orient(mol, old, fixed, keep):
        record = trace(mol, old, fixed, keep)
        result = original_orient(mol, old, fixed, keep)
        record["expected"] = [
            dict(x=p.x, y=p.y, z=p.z)
            for p in (mol.GetConformer().GetAtomPosition(i) for i in range(mol.GetNumAtoms()))
        ]
        stages.append(record)
        return result

    def analyze(mol, **kwargs):
        analysis.extend(
            dict(x=p.x, y=p.y, z=p.z)
            for p in (mol.GetConformer().GetAtomPosition(i) for i in range(mol.GetNumAtoms()))
        )
        return original_analyze(mol, **kwargs)

    cleanup.orient = orient
    worker.analyze = analyze
    try:
        actual = worker.handle(request)
        assert actual == expected
        assert request == before
        profiles.append(dict(fma3=crt._get_FMA3_enable(), stage=stages[0], analysis=analysis))
    finally:
        cleanup.orient = original_orient
        worker.analyze = original_analyze
crt._set_FMA3_enable(original_fma)

header = dict(
    python=sys.version,
    platform=platform.platform(),
    rdkit=rdBase.rdkitVersion,
    ucrt_sha256=hashlib.sha256(
        (Path(os.environ["SystemRoot"]) / "System32" / "ucrtbase.dll").read_bytes()
    ).hexdigest(),
    sources={
        name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
        for name in ["engine/cleanup.py", "engine/worker.py", "tests/cleanup_reference.py"]
    },
)
args.output.write_text(
    json.dumps(
        dict(
            header=header,
            name="F[C@](Cl)(Br)I/0.81/selected_atoms/[2, 3, 4, 5]/True",
            profiles=profiles,
        ),
        indent=2,
    )
    + "\n",
    encoding="utf-8",
    newline="\n",
)
print(json.dumps(header))
