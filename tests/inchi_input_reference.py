"""Observe native InChI adapter arrays; fixture replay does not require C++."""

import argparse
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
    from .build_inchi_input_oracle import ADAPTER_SHA256, HEADER_SHA256
    from .cip_molecule_reference import molecules
    from .perception_reference import snapshot
else:
    from build_inchi_input_oracle import ADAPTER_SHA256, HEADER_SHA256
    from cip_molecule_reference import molecules
    from perception_reference import snapshot

FIXTURE = Path(__file__).parent / "fixtures/inchi-input-native.json.gz"


def corpus():
    yield from molecules()
    for number, isotope, charge, hydrogens in itertools.product(
        range(119), (0, 1, 13, 32767, 65535), (-1, 0, 1), (0, 1)
    ):
        atom = Chem.Atom(number)
        atom.SetIsotope(isotope)
        atom.SetFormalCharge(charge)
        atom.SetNumExplicitHs(hydrogens)
        atom.SetNumRadicalElectrons((number + isotope + hydrogens) % 4)
        atom.SetNoImplicit(number % 2 == 0)
        mol = Chem.RWMol()
        mol.AddAtom(atom)
        mol.UpdatePropertyCache(strict=False)
        yield f"atom/{number}/{isotope}/{charge}/{hydrogens}", mol, "none"
    rng = random.Random(41130)
    for text in (
        "[O-][Cl+3]([O-])([O-])O",
        "[O-][Cl+3]([O-])([O-])[O-]",
        "[O-][Cl+3]([O-])([O-])[O+]",
        "[O-][Cl+3]([O-])([O-])[O-].[O-][Cl+3]([O-])([O-])O",
        "[O-][Cl+3]([O-])([O-])(O)O",
        "[O-][Cl+3]([O-])([O-])([O-])O",
        "[O-][Cl+3]([O-])([O-])O[Cl+3]([O-])([O-])[O-]",
    ):
        original = Chem.MolFromSmiles(text, sanitize=False)
        assert original is not None
        original.UpdatePropertyCache(strict=False)
        for sample in range(60):
            order = list(range(original.GetNumAtoms()))
            rng.shuffle(order)
            mol = Chem.RenumberAtoms(original, order)
            yield f"perchlorate/{text}/{sample}", mol, "none"
    for text in (
        "F[C@](Cl)(Br)I",
        "N[C@@H](C)C(=O)O",
        "C[S@](=O)CC",
        "C[P@](F)Cl",
        "F/C=C/Cl",
        "FC=CC=CF",
        "F/C=C1/CCCCCCC1",
        "ClC(Br)=C(F)I",
        "C1[C@H](O)CCC1",
        "[2H][C@H](F)Cl",
        "*1ccccc1",
    ):
        original = Chem.MolFromSmiles(text)
        assert original is not None
        for sample in range(15):
            order = list(range(original.GetNumAtoms()))
            rng.shuffle(order)
            mol = Chem.RenumberAtoms(original, order)
            if sample:
                conf = Chem.Conformer(mol.GetNumAtoms())
                conf.Set3D(sample % 2 == 0)
                for i in range(mol.GetNumAtoms()):
                    conf.SetAtomPosition(
                        i, tuple(rng.uniform(-3, 3) for _ in range(3)) if sample > 1 else (0, 0, 0)
                    )
                mol.AddConformer(conf)
            yield f"conformer/{text}/{sample}", mol, "symmetric"
    for order, stereo, direction, reverse in itertools.product(
        (1, 2), range(8), range(7), (False, True)
    ):
        mol = Chem.MolFromSmiles("FC(Cl)=C(Br)I")
        assert mol is not None
        bond = mol.GetBondWithIdx(2)
        bond.SetBondType(Chem.BondType.values[order])
        bond.SetBondDir(Chem.BondDir.values[direction])
        bond.SetStereoAtoms(0, 4)
        bond.SetStereo(Chem.BondStereo.values[stereo])
        conf = Chem.Conformer(mol.GetNumAtoms())
        for i in range(mol.GetNumAtoms()):
            conf.SetAtomPosition(i, (i * 1.25, i * i - 3.75, 2.0 - i))
        mol.AddConformer(conf)
        mol.UpdatePropertyCache(strict=False)
        if reverse:
            mol = Chem.RenumberAtoms(mol, list(reversed(range(mol.GetNumAtoms()))))
        yield f"bond/{order}/{stereo}/{direction}/{reverse}", mol, "symmetric"
    for number, degree, tag, hydrogens in itertools.product(
        (6, 7, 15, 16, 33), range(6), (1, 2), range(4)
    ):
        mol = Chem.RWMol()
        atom = Chem.Atom(number)
        atom.SetNoImplicit(True)
        atom.SetNumExplicitHs(hydrogens)
        atom.SetChiralTag(Chem.ChiralType.values[tag])
        mol.AddAtom(atom)
        for i in range(degree):
            mol.AddAtom(Chem.Atom((6, 7, 8, 9, 17)[i]))
            mol.AddBond(0, i + 1, Chem.BondType.SINGLE)
        mol.UpdatePropertyCache(strict=False)
        yield f"tetra/{number}/{degree}/{tag}/{hydrogens}", mol, "none"
    for degree in (19, 20, 21, 30):
        mol = Chem.RWMol()
        mol.AddAtom(Chem.Atom(0))
        for i in range(degree):
            mol.AddAtom(Chem.Atom(0))
            mol.AddBond(0, i + 1, Chem.BondType.SINGLE)
        mol.UpdatePropertyCache(strict=False)
        yield f"degree/{degree}/first", mol, "none"
        yield (
            f"degree/{degree}/last",
            Chem.RenumberAtoms(mol, list(reversed(range(mol.GetNumAtoms())))),
            "none",
        )


def generate(oracle):
    process = subprocess.Popen(
        [str(oracle)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
    )
    assert process.stdin is not None and process.stdout is not None
    rows = []
    try:
        for name, mol, rings in corpus():
            positions = (
                [
                    dict(x=p.x, y=p.y, z=p.z)
                    for p in (
                        mol.GetConformer().GetAtomPosition(i) for i in range(mol.GetNumAtoms())
                    )
                ]
                if mol.GetNumConformers()
                else None
            )
            undefined = any(
                a.GetChiralTag()
                in (Chem.ChiralType.CHI_TETRAHEDRAL_CW, Chem.ChiralType.CHI_TETRAHEDRAL_CCW)
                and 3 <= a.GetTotalDegree() <= 4
                and a.GetDegree() < 3
                for a in mol.GetAtoms()
            )
            if undefined:
                expected = None
            else:
                flags = (
                    Chem.PropertyPickleOptions.AllProps | Chem.PropertyPickleOptions.CoordsAsDouble
                )
                process.stdin.write(mol.ToBinary(flags).hex() + "\n")
                process.stdin.flush()
                line = process.stdout.readline()
                if not line:
                    raise RuntimeError(f"Native oracle stopped on {name}")
                expected = json.loads(line)
                if expected is not None:
                    expected["has_coordinates"] = positions is not None
            rows.append(
                dict(
                    name=name,
                    state=snapshot(mol, rings),
                    positions=positions,
                    undefined_native=undefined,
                    expected=expected,
                )
            )
    finally:
        process.stdin.close()
        process.wait()
    assert process.returncode == 0
    return dict(
        rdkit_version=rdBase.rdkitVersion,
        rdkit_commit="0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985",
        adapter_sha256=ADAPTER_SHA256,
        header_sha256=HEADER_SHA256,
        rows=rows,
    )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--oracle", type=Path)
    parser.add_argument("--write-fixture", action="store_true")
    parser.add_argument("--verify-fixture", action="store_true")
    args = parser.parse_args()
    RDLogger.DisableLog("rdApp.*")
    assert rdBase.rdkitVersion == "2026.03.6"
    if args.oracle:
        data = generate(args.oracle)
        if args.verify_fixture:
            # Native ring tuples become arrays in the JSON transport.
            assert json.loads(json.dumps(data)) == json.loads(gzip.decompress(FIXTURE.read_bytes()))
            print(f"Verified {len(data['rows'])} native adapter cases")
            return
        if args.write_fixture:
            FIXTURE.write_bytes(
                gzip.compress(json.dumps(data, separators=(",", ":")).encode(), mtime=0)
            )
            print(f"Wrote {len(data['rows'])} native adapter cases")
            return
    else:
        data = json.loads(gzip.decompress(FIXTURE.read_bytes()))
    print(
        json.dumps(
            {key: value for key, value in data.items() if key != "rows"}
            | {"fixture_sha256": hashlib.sha256(FIXTURE.read_bytes()).hexdigest()}
        )
    )
    for row in data["rows"]:
        print(json.dumps(row, separators=(",", ":")))


if __name__ == "__main__":
    main()
