"""Independent cleanup.orient, CPython sum and math.hypot captures."""

import json
import math
import random
import sys
from pathlib import Path

from rdkit import Chem
from rdkit.Geometry import Point2D, Point3D

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from engine import cleanup

rng = random.Random(48127)
orientations = []
for n in [1, 2, 3, 8, 63]:
    for scale in [1e-12, 1.0, 1e12, 1e35]:
        old = [
            dict(x=rng.uniform(-3, 3) * scale, y=rng.uniform(-3, 3) * scale, z=0.0)
            for _ in range(n)
        ]
        new = [
            dict(x=rng.uniform(-3, 3) * scale, y=rng.uniform(-3, 3) * scale, z=0.0)
            for _ in range(n)
        ]
        for pins in [0, 1, 2]:
            fixed = {i: Point2D(p["x"], p["y"]) for i, p in enumerate(old[:pins])}
            for keep in [False, True]:
                molecule = Chem.RWMol()
                for _ in range(n):
                    molecule.AddAtom(Chem.Atom("C"))
                conf = Chem.Conformer(n)
                for i, p in enumerate(new):
                    conf.SetAtomPosition(i, (p["x"], p["y"], 0.0))
                molecule.AddConformer(conf)
                cleanup.orient(molecule, [Point3D(p["x"], p["y"], 0.0) for p in old], fixed, keep)
                conf = molecule.GetConformer()
                expected = [
                    dict(x=p.x, y=p.y, z=p.z) for p in (conf.GetAtomPosition(i) for i in range(n))
                ]
                orientations.append(
                    dict(
                        old=old,
                        new=new,
                        fixed={str(i): dict(x=p.x, y=p.y) for i, p in fixed.items()},
                        keep=keep,
                        expected=expected,
                    )
                )
sums = []
for values in [
    [1e16, 1.0, -1e16],
    [-0.0],
    [1e300, 1e-300, -1e300],
    [1e-300] * 100,
    [rng.uniform(-1e20, 1e20) for _ in range(1001)],
]:
    sums.append(dict(values=values, expected=sum(values)))
hypots = []
for x, y in [
    (0.0, -0.0),
    (1e-6, 0.0),
    (math.nextafter(1e-6, 0.0), 1e-15),
    (1e-200, 1e-200),
    (5e-324, 5e-324),
    (1e300, -1e300),
] + [(rng.uniform(-1e10, 1e10), rng.uniform(-1e10, 1e10)) for _ in range(1000)]:
    hypots.append(dict(x=x, y=y, expected=math.hypot(x, y)))
print(json.dumps(dict(orientations=orientations, sums=sums, hypots=hypots)))
