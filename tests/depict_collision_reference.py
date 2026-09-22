"""Independent native collision-stage captures with exact input/output bits."""

import argparse
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

from build_depict_collision_oracle import PIN, SOURCE_SHA256

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests/fixtures/depict-collision-linux-native.json.gz"


def bits(value):
    return struct.pack(">d", value).hex()


def cases():
    from perception_reference import snapshot
    from rdkit import Chem, rdBase
    from rdkit.Chem import rdDepictor

    assert rdBase.rdkitVersion == "2026.03.6"
    rng = random.Random(723649)
    molecules = [
        "",
        "C",
        "CC",
        "CCC",
        "CCCC",
        "CCCCCC",
        "CCCCCCCCCCCCCC",
        "CC(C)CC(C)(C)C",
        "NC(=O)CC(O)C",
        "C/C=C/C",
        "C/C=C\\C",
        "C=C=C",
        "C[C@H](O)[C@@H](N)C(F)(Cl)Br",
        "C1CC1",
        "C1CCC1",
        "C1CCCC1",
        "c1ccccc1",
        "CC1=CC(C)=CC=C1O",
        "C1CCCCCCCCCCC1",
        "C1CCCCCCCCCCCCCCCCCCC1",
        "CC1CCC2(CC1)CCC(C)C2",
        "C1CCC2(CC1)CCCC2",
        "CC1CCC2CC(C)CCC2C1",
        "C1CC2CCC1C2",
        "CC1CC2CCC1C2O",
        "C12C3C4C1C5C2C3C45",
        "C1C2CC3CC1CC(C2)C3",
        "CC.CCCC",
        "C.C",
        "[Na+].[Cl-]",
        "[Pt@SP1](Cl)(Br)(N)I",
        "[Fe](C)(C)(C)(C)(C)C",
        "OC1CCC2(CC1)CCC3(CC2)CC(C)CC3",
        "C1CCC2(CC1)CC3CCC2C3",
    ]
    for index, text in enumerate(molecules):
        molecule = Chem.MolFromSmiles(text)
        assert molecule is not None, text
        orders = [list(range(molecule.GetNumAtoms()))]
        if molecule.GetNumAtoms() > 3:
            orders.append(list(reversed(orders[0])))
            order = orders[0].copy()
            rng.shuffle(order)
            orders.append(order)
        for permutation, order in enumerate(orders):
            mol = Chem.RenumberAtoms(molecule, order) if order else Chem.Mol(molecule)
            mol.UpdatePropertyCache(strict=False)
            Chem.GetSymmSSSR(mol)
            rdDepictor.Compute2DCoords(mol, forceRDKit=True, useRingTemplates=True)
            original = [
                (mol.GetConformer().GetAtomPosition(i).x, mol.GetConformer().GetAtomPosition(i).y)
                for i in range(mol.GetNumAtoms())
            ]
            for mode in (
                "native",
                "compressed",
                "vertical",
                "grid",
                "crossed",
                "coincident",
                "near_threshold",
            ):
                for fixed in ("none", "some", "all"):
                    if fixed != "none" and mode == "native":
                        continue
                    coords = []
                    for i, (x, y) in enumerate(original):
                        if mode == "compressed":
                            x, y = 0.31 * x, 0.47 * y
                        elif mode == "vertical":
                            x, y = 0.16 * x, 1.1 * y
                        elif mode == "grid":
                            x, y = (i % 4) * 0.41, (i // 4) * 0.53
                        elif mode == "crossed":
                            angle = i * 2.7
                            x, y = math.cos(angle) * 1.5, math.sin(angle) * 1.5
                        elif mode == "coincident":
                            x, y = 0.0, 0.0
                        elif mode == "near_threshold":
                            x, y = i * math.nextafter(0.7, 0.0), (i % 2) * 1e-8
                        coords.append((x + 0.1, y - 0.2))
                    atoms = []
                    for i, (x, y) in enumerate(coords):
                        neighbors = [n.GetIdx() for n in mol.GetAtomWithIdx(i).GetNeighbors()]
                        is_fixed = fixed == "all" or (fixed == "some" and i % 3 == 0)
                        atoms.append(
                            {
                                "ints": [
                                    i,
                                    i,
                                    neighbors[0] if neighbors else -1,
                                    neighbors[-1] if len(neighbors) > 1 else -1,
                                    -1,
                                    i % 2,
                                    0,
                                    int(is_fixed),
                                ],
                                "values": list(map(bits, (x, y, 0.6, -0.8, 0.5, 2.5))),
                                "neighbors": neighbors[:1],
                            }
                        )
                    yield {
                        "name": f"{index}/{text}/{permutation}/{mode}/{fixed}",
                        "state": snapshot(mol, "symmetric"),
                        "pickle": mol.ToBinary(Chem.PropertyPickleOptions.AllProps).hex(),
                        "fragment": {
                            "done": True,
                            "bounds": list(map(bits, (3.0, 4.0, -5.0, 6.0))),
                            "atoms": atoms,
                            "attachment_points": list(range(len(atoms)))[:2],
                        },
                    }


def request(case):
    fragment = case["fragment"]
    values = [
        case["pickle"],
        str(len(fragment["atoms"])),
        str(int(fragment["done"])),
        *fragment["bounds"],
    ]
    for atom in fragment["atoms"]:
        values += list(map(str, atom["ints"])) + atom["values"]
        values += [str(len(atom["neighbors"])), *map(str, atom["neighbors"])]
    values += [str(len(fragment["attachment_points"])), *map(str, fragment["attachment_points"])]
    return " ".join(values) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--oracle", type=Path)
    parser.add_argument("--rdkit-source", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--fixture", type=Path, default=FIXTURE)
    parser.add_argument("--replay", action="store_true")
    args = parser.parse_args()
    if args.oracle is None:
        print(gzip.decompress(args.fixture.read_bytes()).decode(), end="")
        return
    import rdkit
    from rdkit import rdBase

    assert rdBase.rdkitVersion == "2026.03.6"
    assert args.rdkit_source is not None
    for name, digest in SOURCE_SHA256.items():
        assert hashlib.sha256((args.rdkit_source / name).read_bytes()).hexdigest() == digest
    if args.replay:
        original = [
            json.loads(line) for line in gzip.decompress(args.fixture.read_bytes()).splitlines()
        ]
        assert original[0]["provenance"]["commit"] == PIN
        inputs = original[1:]
    else:
        inputs = list(cases())
    package = Path(rdkit.__file__).parent
    libs = (
        package.parent / "rdkit.libs"
        if platform.system() in ("Linux", "Windows")
        else package / ".dylibs"
    )
    native_env = (
        {**os.environ, "DYLD_LIBRARY_PATH": str(libs)}
        if platform.system() == "Darwin"
        else {**os.environ, "PATH": str(libs) + os.pathsep + os.environ.get("PATH", "")}
        if platform.system() == "Windows"
        else os.environ
    )
    result = subprocess.run(
        [str(args.oracle.resolve())],
        input="".join(map(request, inputs)),
        text=True,
        capture_output=True,
        check=True,
        timeout=180,
        env=native_env,
    )
    outputs = result.stdout.splitlines()
    assert len(outputs) == len(inputs), (len(outputs), len(inputs), result.stderr)
    paths = sorted(
        p
        for p in libs.iterdir()
        if any(
            name in p.name
            for name in ("RDKitDepictor", "RDKitGraphMol", "RDKitRDGeometryLib", "RDKitRDGeneral")
        )
    )
    header = {
        "provenance": {
            "version": "2026.03.6",
            "commit": PIN,
            "source_sha256": SOURCE_SHA256,
            "platform": platform.platform(),
            "library_sha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in paths},
            "observer_source_sha256": hashlib.sha256(
                (ROOT / "tests/depict_collision_reference.cpp").read_bytes()
            ).hexdigest(),
            "fragment_observer_sha256": hashlib.sha256(
                (ROOT / "tests/depict_attachment_reference.cpp").read_bytes()
            ).hexdigest(),
            "oracle_sha256": hashlib.sha256(args.oracle.read_bytes()).hexdigest(),
            "cases": len(inputs),
            "observation": "Single private: access changed to public: in EmbeddedFrag.h; original wheel functions unchanged; totalDensity source fold used on Windows because symbol is not exported, checked against native on Linux/macOS",
        }
    }
    lines = [json.dumps(header, separators=(",", ":"))]
    for case, output in zip(inputs, outputs, strict=True):
        case["expected"] = json.loads(output)
        lines.append(json.dumps(case, separators=(",", ":")))
    data = ("\n".join(lines) + "\n").encode()
    if args.output:
        args.output.write_bytes(gzip.compress(data, mtime=0))
        print(
            f"{len(inputs)} native collision cases; JSONL SHA256 {hashlib.sha256(data).hexdigest()}"
        )
    else:
        print(data.decode(), end="")


if __name__ == "__main__":
    main()
