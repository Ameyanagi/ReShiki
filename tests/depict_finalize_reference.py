"""Direct pinned-native finalization captures; default replay needs only Python stdlib."""

import argparse
import copy
import gzip
import hashlib
import json
import math
import os
import platform
import random
import struct
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PIN = "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985"
FIXTURE = ROOT / "tests/fixtures/depict-finalize-linux-native.json.gz"
SOURCES = (
    "Code/GraphMol/Depictor/RDDepictor.cpp",
    "Code/GraphMol/Depictor/EmbeddedFrag.cpp",
    "Code/GraphMol/Depictor/EmbeddedFrag.h",
    "Code/GraphMol/Conformer.h",
    "Code/GraphMol/ROMol.cpp",
    "Code/Geometry/Transform2D.cpp",
    "Code/Geometry/point.h",
)


def bits(value):
    return struct.pack(">d", value).hex()


def value(encoded):
    return struct.unpack(">d", bytes.fromhex(encoded))[0]


def read(path):
    return [json.loads(line) for line in gzip.decompress(path.read_bytes()).splitlines()]


def fragment(points, offset=0):
    atoms = []
    for index, (x, y) in enumerate(points):
        atom = index + offset
        atoms.append(
            dict(
                ints=[atom, 0 if index % 3 == 0 else atom, -1, -1, -1, index % 2, -1, index % 2],
                values=list(map(bits, (x, y, 0.6, -0.8, 1.25, -1.0))),
                neighbors=[],
            )
        )
    return dict(
        done=True, bounds=list(map(bits, (3.0, -4.0, 5.0, -6.0))), atoms=atoms, attachment_points=[]
    )


def cases():
    rng = random.Random(732858)

    def emit(name, fragments, count=None, coordinates=None, canonical=True, existing=0):
        maximum = max((a["ints"][0] for f in fragments for a in f["atoms"]), default=-1) + 1
        return dict(
            name=name,
            atom_count=maximum if count is None else count,
            fragments=fragments,
            coordinates=coordinates,
            canonical=canonical,
            existing=existing,
        )

    geometry = ROOT / "tests/fixtures/depict-geometry-linux-native.json.gz"
    for c in read(geometry)[1:]:
        if c["op"] != "canonical":
            continue
        points = list(zip(map(value, c["values"][::2]), map(value, c["values"][1::2]), strict=True))
        f = fragment(points)
        for atom, atom_id in zip(f["atoms"], c["ids"], strict=True):
            atom["ints"][0] = atom_id
            atom["ints"][1] = atom_id
        yield emit("canonical/" + c["name"], [f], existing=2)

    samples = [
        [],
        [(0.0, -0.0)],
        [(-0.0, 0.0), (0.0, -0.0)],
        [(2.0, 3.0), (2.0, 3.0)],
        [(-2.0, 1.0), (3.0, -1.0)],
        [(0.0, 0.0), (1e-4, 1e-4)],
        [(1e9, 2e9), (1e9 + 2, 2e9 - 2)],
    ]
    for index, points in enumerate(samples):
        for reverse in (False, True):
            first = fragment(points)
            second = fragment([(3.0, -2.0), (-1.0, 5.0)], len(points))
            fragments = [first, second] if not reverse else [second, first]
            count = len(points) + 5
            for canonical in (False, True):
                for coords in (
                    None,
                    [],
                    [[count - 1, bits(-0.0), bits(7.5)]],
                    [[0, bits(10.0), bits(-5.0)], [count - 1, bits(-3.0), bits(2.0)]],
                ):
                    yield emit(
                        f"packing/{index}/{reverse}/{canonical}/{coords is None}/{len(coords or [])}",
                        fragments,
                        count,
                        coords,
                        canonical,
                        3,
                    )
    for width in (0.0, 1.0, 1.0 + 2**-52, 1.0 - 2**-53, 1e8, -0.0):
        for height in (1.0, 1e8):
            for order in range(3):
                fragments = [
                    fragment([(-width, -height), (width, height)]),
                    fragment([(0.0, 0.0)], 2),
                    fragment([(1.0, 2.0), (3.0, 4.0)], 3),
                ]
                fragments = fragments[order:] + fragments[:order]
                yield emit(f"axis/{width}/{height}/{order}", fragments, canonical=False)
    for count in (0, 1, 3, 9):
        for canonical in (False, True):
            yield emit(f"missing/{count}/{canonical}", [], count, [], canonical)
    for index in range(480):
        fragments = []
        offset = 0
        for _ in range(rng.randrange(0, 7)):
            points = [
                (rng.uniform(-12, 12), rng.uniform(-12, 12)) for _ in range(rng.randrange(0, 11))
            ]
            f = fragment(points, offset)
            f["done"] = bool(rng.randrange(2))
            for a in f["atoms"]:
                a["values"][2:4] = list(map(bits, (rng.uniform(-3, 3), rng.uniform(-3, 3))))
                a["ints"][2:5] = [offset, offset, offset]
                a["neighbors"] = [offset]
            f["attachment_points"] = [offset] if points else []
            fragments.append(f)
            offset += len(points)
        # Duplicate IDs in later fragments exercise native last-write semantics.
        if index % 11 == 0 and fragments:
            fragments.append(copy.deepcopy(fragments[0]))
        count = offset + 2
        coords = None if index % 4 == 0 else []
        if index % 4 == 2:
            coords = [[count - 1, bits(3.0), bits(-4.0)]]
        if index % 4 == 3:
            coords = [[0, bits(1.0), bits(2.0)], [count - 1, bits(3.0), bits(4.0)]]
        yield emit(f"random/{index}", fragments, count, coords, bool(index % 3), index % 4)

    for component in ("rings", "attachment", "seeds"):
        selected = 0
        for index, c in enumerate(
            read(ROOT / f"tests/fixtures/depict-{component}-linux-native.json.gz")[1:]
        ):
            if index % 7:
                continue
            expected = c["expected"]
            if component == "attachment":
                f = expected["steps"][-1]["fragment"]
            else:
                f = expected.get("fragment")
            if not f or not all(math.isfinite(value(v)) for a in f["atoms"] for v in a["values"]):
                continue
            f = copy.deepcopy(f)
            f.setdefault("attachment_points", [])
            yield emit(
                f"native/{component}/{c['name']}",
                [f],
                len(c["state"]["graph"]["atoms"]),
                existing=1,
            )
            selected += 1
            if selected == 100:
                break
    # Native finite input can overflow without throwing; Rust classifies it.
    yield emit("numeric/overflow-center", [fragment([(1e308, 1e308), (1e308, 1e308)])])
    normal = fragment([(0.0, 0.0), (1.0, 2.0)])
    normal["atoms"][0]["values"][2:4] = [bits(1.7e308), bits(1.7e308)]
    yield emit("numeric/overflow-normal", [normal])
    yield emit(
        "numeric/overflow-pack",
        [fragment([(1e308, 1e308)]), fragment([(1e308, 1e308)], 1)],
        canonical=False,
    )
    yield emit(
        "numeric/overflow-anchor",
        [fragment([(1e308, 0.0)])],
        coordinates=[[0, bits(-1e308), bits(0.0)]],
    )


def request(case):
    coords = case["coordinates"] or []
    fields = [
        case["atom_count"],
        int(case["canonical"]),
        case["existing"],
        len(case["fragments"]),
        int(case["coordinates"] is not None),
        len(coords),
    ]
    for coord in coords:
        fields.extend(coord)
    for f in case["fragments"]:
        fields += [int(f["done"]), *f["bounds"], len(f["atoms"])]
        for a in f["atoms"]:
            fields += [*a["ints"], *a["values"], len(a["neighbors"]), *a["neighbors"]]
        fields += [len(f["attachment_points"]), *f["attachment_points"]]
    return " ".join(map(str, fields)) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--oracle", type=Path)
    parser.add_argument("--rdkit-source", type=Path)
    parser.add_argument("--fixture", type=Path, default=FIXTURE)
    parser.add_argument("--replay", action="store_true")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--build-provenance", type=Path)
    args = parser.parse_args()
    if not args.oracle:
        print(gzip.decompress(args.fixture.read_bytes()).decode(), end="")
        return
    import rdkit
    from rdkit import rdBase

    assert rdBase.rdkitVersion == "2026.03.6"
    assert args.rdkit_source is not None
    hashes = {n: hashlib.sha256((args.rdkit_source / n).read_bytes()).hexdigest() for n in SOURCES}
    if args.replay:
        original = read(args.fixture)
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
    run = subprocess.run(
        [str(args.oracle.resolve())],
        input="".join(map(request, inputs)),
        capture_output=True,
        text=True,
        check=True,
        timeout=120,
        cwd=ROOT,
        env=os.environ,
    )
    outputs = run.stdout.splitlines()
    assert len(outputs) == len(inputs), run.stderr
    library_dir = (
        Path(rdkit.__file__).parent / ".dylibs"
        if platform.system() == "Darwin"
        else Path(rdkit.__file__).parent.parent / "rdkit.libs"
    )
    libraries = [
        p
        for p in library_dir.glob("*")
        if p.is_file()
        and any(
            n in p.name
            for n in ("RDKitDepictor", "RDKitGraphMol", "RDKitRDGeometryLib", "RDKitRDGeneral")
        )
    ]
    provenance = dict(
        version=rdBase.rdkitVersion,
        commit=PIN,
        platform=platform.platform(),
        source_sha256=hashes,
        library_sha256={p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in libraries},
        oracle_sha256=hashlib.sha256(args.oracle.read_bytes()).hexdigest(),
        cases=len(inputs),
    )
    if args.build_provenance:
        provenance["native_build"] = json.loads(args.build_provenance.read_text())
    lines = [json.dumps(dict(provenance=provenance), separators=(",", ":"))]
    for case, output in zip(inputs, outputs, strict=True):
        case["expected"] = json.loads(output)
        lines.append(json.dumps(case, separators=(",", ":")))
    data = ("\n".join(lines) + "\n").encode()
    if args.output:
        args.output.write_bytes(gzip.compress(data, mtime=0))
        print(
            len(inputs), "native finalization cases; JSONL SHA256", hashlib.sha256(data).hexdigest()
        )
    else:
        print(data.decode(), end="")


if __name__ == "__main__":
    main()
