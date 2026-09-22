"""Read raw native fixtures, or regenerate using independent C++ API calls.

No Rust implementation participates in fixture generation. Inputs and outputs
use IEEE-754 hexadecimal bits, preserving signed zeros and nonfinite results.
Python RDKit supplies additional native-layout input fragments only; expected
geometry always comes from the C++ helper's five direct native API calls.
"""

import argparse
import gzip
import hashlib
import json
import math
import platform
import random
import struct
import subprocess
from pathlib import Path

PIN = "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985"
FIXTURE = Path(__file__).parent / "fixtures/depict-geometry-native.json.gz"
SOURCES = (
    "Code/GraphMol/Depictor/DepictUtils.cpp",
    "Code/GraphMol/Depictor/DepictUtils.h",
    "Code/GraphMol/Depictor/EmbeddedFrag.cpp",
    "Code/GraphMol/Depictor/EmbeddedFrag.h",
    "Code/Geometry/Transform2D.cpp",
    "Code/Geometry/Transform2D.h",
    "Code/Geometry/point.h",
)


def bits(value):
    return struct.pack(">d", value).hex()


def case(name, op, ids, values):
    return {"name": name, "op": op, "ids": ids, "values": [bits(value) for value in values]}


def cases():
    rng = random.Random(731995)
    for size in range(33):
        for length in (0.0, -0.0, 1e-10, 0.5, 1.5, 28.0, -1.5, 1e10):
            ids = list(range(size))
            for ordering in range(3):
                if ordering == 1:
                    rng.shuffle(ids)
                elif ordering == 2 and ids:
                    ids[-1] = ids[0]
                yield case(f"ring/{size}/{length}/{ordering}", "ring", ids[:], [length])
    for size in (100, 1000, 4096):
        yield case(f"large-ring/{size}", "ring", list(range(size)), [1.5])
    for angle in (
        -10.0,
        0.0,
        math.pi,
        math.nextafter(math.pi, 0),
        math.nextafter(math.pi, math.inf),
        7.0,
    ):
        for offset in (0.0, 1.0, 1e6):
            yield case(
                f"bisect/{angle}/{offset}",
                "bisect",
                [],
                [offset, -offset, angle, 2.0, 4.0, 7.0, -8.0],
            )
    axes = [
        (0.0, 0.0, 0.0, 0.0),
        (2.0, 3.0, 2.0, 3.0),
        (0.0, 0.0, 1e-200, 1e-200),
        (0.0, 0.0, 0.0, 1.0),
        (0.0, 0.0, 1.0, 0.0),
        (3.0, 8.0, -2.0, -1.0),
        (1.0, 1.0, -1.0, -1.0),
        (1.0, -1.0, -1.0, 1.0),
    ]
    for axis in axes:
        for point in ((0.0, 0.0), (2.0, 3.0), (-4.0, -0.0), (1e-100, 1e-100)):
            yield case(f"axis/{axis}/{point}", "reflect", [], [*point, *axis])
    fragments = [
        [],
        [(2.0, 3.0)],
        [(2.0, 3.0)] * 3,
        [(-1.0, 0.0), (1.0, 0.0)],
        [(0.0, -1.0), (0.0, 1.0)],
        [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)],
        [(0.0, -0.0), (-0.0, 0.0)],
        [(2e8, 3e8), (4e8, 5e8)],
        [(-2e8, -3e8), (-4e8, -5e8)],
        [(1e8, -1e8)],
    ]
    # eig1.length() is 4*t*t for these two vertical points. Bracket the
    # exact 1e-4 early-return threshold, without an orientation alignment.
    threshold = 0.005
    for value in (math.nextafter(threshold, 0), threshold, math.nextafter(threshold, math.inf)):
        fragments.append([(0.0, -value), (0.0, value)])
    for index, points in enumerate(fragments):
        for op in ("canonical", "box"):
            yield case(
                f"fragment/{index}", op, list(range(len(points))), [v for p in points for v in p]
            )
    # Bracket the same eigenvector threshold away from the coordinate axes.
    # Native build contraction can change which side of the branch is taken.
    for index in range(1, 32):
        angle = math.pi * index / 32
        radius = 0.005 / math.sqrt(math.sin(angle))
        for step in range(-4, 5):
            value = radius
            for _ in range(abs(step)):
                value = math.nextafter(value, 0.0 if step < 0 else math.inf)
            x, y = value * math.cos(angle), value * math.sin(angle)
            yield case(f"threshold/{index}/{step}", "canonical", [0, 1], [-x, -y, x, y])
    for index in range(1000):
        scale = 2.0 ** rng.randint(-12, 16)
        values = [rng.randrange(-10000, 10001) / 1024 * scale for _ in range(7)]
        yield case(f"random-bisect/{index}", "bisect", [], values)
        yield case(f"random-reflect/{index}", "reflect", [], values[:6])
        size = rng.randrange(2, 26)
        ids = list(range(size))
        rng.shuffle(ids)
        coords = [rng.randrange(-10000, 10001) / 1024 * scale for _ in range(size * 2)]
        for op in ("canonical", "box"):
            yield case(f"random-fragment/{index}", op, ids, coords)
    # These are source-native fragments from explicit pinned public defaults.
    from rdkit import Chem, rdBase
    from rdkit.Chem import rdDepictor

    assert rdBase.rdkitVersion == "2026.03.6"
    texts = (
        "C",
        "CC",
        "CCC",
        "CCCCCC",
        "C1CC1",
        "C1CCC1",
        "C1CCCCC1",
        "c1ccccc1",
        "c1ccc2ccccc2c1",
        "C1CC2CCC1C2",
        "C1C2CC3CC1CC(C2)C3",
        "C1CCCCC1.CC",
        "CC(C)(C)CC(C)C",
        "F/C=C/F",
        "F/C=C\\F",
        "N[C@@H](C)C(=O)O",
        "CC1=CC=CC=C1O",
        "C1CCCCCCCCCCC1",
        "O=C(O)C(O)C(O)C(=O)O",
        "[Na+].[Cl-]",
    )
    for index, text in enumerate(texts):
        mol = Chem.MolFromSmiles(text)
        assert mol is not None
        for canonical in (False, True):
            rdDepictor.Compute2DCoords(
                mol,
                canonOrient=canonical,
                clearConfs=True,
                coordMap={},
                nFlipsPerSample=0,
                nSample=0,
                sampleSeed=0,
                permuteDeg4Nodes=False,
                bondLength=1.5,
                forceRDKit=True,
                useRingTemplates=False,
            )
            conf = mol.GetConformer()
            coords = [
                (conf.GetAtomPosition(i).x, conf.GetAtomPosition(i).y)
                for i in range(mol.GetNumAtoms())
            ]
            for ordering in range(2):
                ids = list(range(len(coords)))
                if ordering:
                    rng.shuffle(ids)
                for op in ("canonical", "box"):
                    yield case(
                        f"native/{index}/{canonical}/{ordering}",
                        op,
                        ids,
                        [v for p in coords for v in p],
                    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--oracle", type=Path)
    parser.add_argument("--rdkit-source", type=Path)
    parser.add_argument("--fixture", type=Path, default=FIXTURE)
    parser.add_argument(
        "--replay", action="store_true", help="Keep exactly the fixture's input bits"
    )
    parser.add_argument("--output", type=Path)
    parser.add_argument("--write-fixture", action="store_true")
    args = parser.parse_args()
    if args.oracle:
        assert args.rdkit_source is not None
        source_hashes = {
            name: hashlib.sha256((args.rdkit_source / name).read_bytes()).hexdigest()
            for name in SOURCES
        }
        if args.replay:
            original = [
                json.loads(line) for line in gzip.decompress(args.fixture.read_bytes()).splitlines()
            ]
            assert original[0]["provenance"]["commit"] == PIN
            assert original[0]["provenance"]["source_sha256"] == source_hashes
            inputs = original[1:]
        else:
            assert (
                subprocess.check_output(
                    ["git", "-C", str(args.rdkit_source), "rev-parse", "HEAD"], text=True
                ).strip()
                == PIN
            )
            inputs = list(cases())
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
            and any(name in path.name for name in ("RDKitDepictor", "RDKitRDGeometryLib"))
        )
        requests = "".join(
            " ".join([c["op"], str(len(c["ids"])), *map(str, c["ids"]), *c["values"]]) + "\n"
            for c in inputs
        )
        result = subprocess.run(
            [str(args.oracle.resolve())],
            input=requests,
            text=True,
            capture_output=True,
            check=True,
            timeout=120,
            cwd=Path(__file__).resolve().parents[1],
        )
        outputs = result.stdout.splitlines()
        assert len(outputs) == len(inputs), result.stderr
        lines = [
            json.dumps(
                {
                    "provenance": {
                        "version": "2026.03.6",
                        "commit": PIN,
                        "platform": platform.platform(),
                        "source_sha256": source_hashes,
                        "library_sha256": {
                            path.name: hashlib.sha256(path.read_bytes()).hexdigest()
                            for path in libraries
                        },
                        "oracle_sha256": hashlib.sha256(args.oracle.read_bytes()).hexdigest(),
                        "cases": len(inputs),
                        "generator": "direct native C++ APIs; binary64 inputs and outputs",
                    }
                },
                separators=(",", ":"),
            )
        ]
        for item, expected in zip(inputs, outputs, strict=True):
            item["expected"] = json.loads(expected)
            lines.append(json.dumps(item, separators=(",", ":")))
        data = ("\n".join(lines) + "\n").encode()
        if args.write_fixture:
            output = args.output or args.fixture
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_bytes(gzip.compress(data, mtime=0))
            print(
                f"{len(inputs)} native cases; uncompressed SHA256 {hashlib.sha256(data).hexdigest()}"
            )
            return
    else:
        data = gzip.decompress(args.fixture.read_bytes())
    print(data.decode(), end="")


if __name__ == "__main__":
    main()
