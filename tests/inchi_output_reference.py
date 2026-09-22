"""Replay or regenerate independently captured pinned InchiToMol stage fixtures."""

import argparse
import gzip
import hashlib
import itertools
import json
import os
import random
import re
import subprocess
import sys
from pathlib import Path
from typing import TYPE_CHECKING

import rdkit
from rdkit import Chem, RDConfig, rdBase
from rdkit.Chem import rdinchi

if TYPE_CHECKING or __package__:
    from .perception_reference import snapshot
    from .stereo_reference import state as structural_state
    from .valence_reference import ORDERS
else:
    from perception_reference import snapshot
    from stereo_reference import state as structural_state
    from valence_reference import ORDERS

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests/fixtures/inchi-output.jsonl.gz"


def decode_state(value):
    if value is None:
        return None
    mol = Chem.Mol(bytes.fromhex(value["pickle"]))  # ty: ignore[no-matching-overload]
    # The shared chemical graph uses zero for HYDROGEN. Preserve UNSPECIFIED
    # separately instead of losing the native bond identity in that encoding.
    old = ORDERS[0]
    ORDERS[0] = Chem.BondType.UNSPECIFIED
    try:
        if any(v is None for v in value["valences"]):
            return dict(native_cache_boundary=True, structural_state=structural_state(mol))
        state = snapshot(mol, value["ring_kind"])
    finally:
        ORDERS[0] = old
    state["valences"] = value["valences"]
    return state


def native_command(raw, sanitize=False, remove=False):
    tokens = [
        "S",
        int(sanitize),
        int(remove),
        raw.get("status", 0),
        len(raw["atoms"]),
        len(raw.get("stereo", [])),
    ]
    for a in raw["atoms"]:
        tokens += [a["element"], a.get("isotopic_mass", 0), a.get("charge", 0), a.get("radical", 0)]
        tokens += a.get("hydrogens", [0, 0, 0, 0])
        tokens += [len(a.get("bonds", []))]
        for b in a.get("bonds", []):
            tokens += [b["neighbor"], b["kind"], b.get("stereo", 0)]
    for s in raw.get("stereo", []):
        tokens += [
            s["central_atom"] if s["central_atom"] is not None else -1,
            s["kind"],
            s["parity"],
        ]
        tokens += s["neighbors"]
    return " ".join(map(str, tokens))


def cases(source):
    for status, sanitize, remove in itertools.product(
        (-100, -2, -1, 0, 1, 2, 3, 4, 5), (False, True), (False, True)
    ):
        yield (
            dict(
                name=f"empty/{status}/{sanitize}/{remove}",
                operation="synthetic",
                sanitize=sanitize,
                remove=remove,
            ),
            native_command(dict(status=status, atoms=[]), sanitize, remove),
        )
    identifiers = {
        "",
        "invalid",
        "InChI=1S/",
        "InChI=1S/invalid",
        "InChI=1S/CH4/h1H4",
        "InChI=1/CH4/h1H4",
    }
    native_tests = (source / "External/INCHI-API/test.cpp").read_text()
    # Adjacent C++ string literals are joined before extracting complete IDs.
    native_tests = re.sub(r'"\s*"', "", native_tests)
    identifiers.update(re.findall(r'"(InChI=[^"\n]+)"', native_tests))
    smiles = [
        "[H]",
        "[H][H]",
        "[2H][3H]",
        "[1H]C([2H])([3H])F",
        "[13CH3]C",
        "[CH3]",
        "[CH2]",
        "[CH]",
        "[O]",
        "[O-]",
        "[Na+].[Cl-]",
        "[Fe+2]",
        "N[C@@H](C)C(=O)O",
        "C[C@H](O)[C@H](O)C",
        "C[C@H](O)[C@@H](O)C",
        "F/C=C/Cl",
        "F/C=C\\Cl",
        "F/C(Cl)=C(Br)/I",
        "C/C=C/C=C/C",
        "[2H]/C(F)=C(Cl)/[3H]",
        "C[S@](=O)CC",
        "C[P@](F)(=O)O",
        "C1=CC=CC=C1",
        "c1cc[nH]c1",
        "[N+](=O)([O-])C",
        "N#N",
        "[NH4+]",
        "[NH2-]",
        "[C-]#[O+]",
        "O=S(=O)(O)O",
        "[O-][Cl+3]([O-])([O-])[O-]",
    ]
    smiles += [
        line.split()[0]
        for line in (Path(RDConfig.RDDataDir) / "NCI/first_5K.smi").read_text().splitlines()[:180]
    ]
    for text in smiles:
        mol = Chem.MolFromSmiles(text)
        if mol:
            for options in ("", "/FixedH"):
                inchi = rdinchi.MolToInchi(mol, options)[0]
                if inchi:
                    identifiers.add(inchi)
    for text in sorted(identifiers):
        for sanitize, remove in itertools.product((False, True), repeat=2):
            yield (
                dict(
                    name=f"inchi/{text}/{sanitize}/{remove}",
                    operation="import",
                    sanitize=sanitize,
                    remove=remove,
                    inchi=text,
                ),
                f"I {int(sanitize)} {int(remove)} {text}",
            )

    for number, isotope, radical, hs in itertools.product(
        range(119), (0, 9999, 10000, 10001), (0, 2, 3), (0, 1)
    ):
        raw = dict(
            atoms=[
                dict(
                    element=Chem.GetPeriodicTable().GetElementSymbol(number),
                    isotopic_mass=isotope,
                    radical=radical,
                    hydrogens=[hs, 0, 0, 0],
                )
            ]
        )
        yield (
            dict(
                name=f"atom/{number}/{isotope}/{radical}/{hs}",
                operation="synthetic",
                sanitize=False,
                remove=False,
            ),
            native_command(raw),
        )
    for hydrogens in itertools.product((0, 1, 2), repeat=4):
        raw = dict(atoms=[dict(element="C", hydrogens=list(hydrogens))])
        yield (
            dict(
                name=f"hydrogens/{hydrogens}", operation="synthetic", sanitize=False, remove=False
            ),
            native_command(raw),
        )
    for number, mass, hs in itertools.product(
        (1, 6, 7, 50, 118), (-32768, -1, 1, 32767), (-128, -1, 127)
    ):
        raw = dict(
            atoms=[
                dict(
                    element=Chem.GetPeriodicTable().GetElementSymbol(number),
                    isotopic_mass=mass,
                    hydrogens=[hs, 0, 0, 0],
                )
            ]
        )
        yield (
            dict(
                name=f"stored-width/{number}/{mass}/{hs}",
                operation="synthetic",
                sanitize=False,
                remove=False,
            ),
            native_command(raw),
        )
    for kind, direction, reverse, sanitize in itertools.product(
        range(-1, 6), (-6, -4, -1, 0, 1, 3, 4, 6, 7), (False, True), (False, True)
    ):
        rows = [dict(element="C", bonds=[]), dict(element="N", bonds=[])]
        rows[int(reverse)]["bonds"] = [dict(neighbor=int(not reverse), kind=kind, stereo=direction)]
        # The second copy deliberately disagrees; first occurrence must win.
        rows[int(not reverse)]["bonds"] = [dict(neighbor=int(reverse), kind=1, stereo=0)]
        yield (
            dict(
                name=f"bond/{kind}/{direction}/{reverse}/{sanitize}",
                operation="synthetic",
                sanitize=sanitize,
                remove=False,
            ),
            native_command(dict(atoms=rows), sanitize),
        )
    for number, degree, parity in itertools.product((6, 7, 15, 16), (3, 4), range(7)):
        for order in itertools.permutations(range(1, degree + 1)):
            atoms = [
                dict(
                    element=Chem.GetPeriodicTable().GetElementSymbol(number),
                    hydrogens=[int(degree == 3 and number == 6), 0, 0, 0],
                    bonds=[dict(neighbor=i, kind=1) for i in range(1, degree + 1)],
                )
            ]
            atoms += [dict(element=e) for e in ("F", "Cl", "Br", "I")[:degree]]
            neighbors = ([0] if degree == 3 else []) + list(order)
            raw = dict(
                atoms=atoms,
                stereo=[dict(central_atom=0, kind=2, parity=parity, neighbors=neighbors)],
            )
            yield (
                dict(
                    name=f"tetra/{number}/{degree}/{parity}/{order}",
                    operation="synthetic",
                    sanitize=False,
                    remove=False,
                ),
                native_command(raw),
            )
    for kind, parity in itertools.product((-1, 0, 1, 3, 4), range(7)):
        atoms = [
            dict(element="C", bonds=[dict(neighbor=1, kind=2), dict(neighbor=2, kind=1)]),
            dict(element="C", bonds=[dict(neighbor=3, kind=1)]),
            dict(element="F"),
            dict(element="Cl"),
        ]
        raw = dict(
            atoms=atoms,
            stereo=[dict(central_atom=None, kind=kind, parity=parity, neighbors=[2, 0, 1, 3])],
        )
        yield (
            dict(
                name=f"stereo/{kind}/{parity}", operation="synthetic", sanitize=False, remove=False
            ),
            native_command(raw),
        )
    # Standalone exact native cleanup captures exercise rare rules before the
    # Rust port; unlike InChI roundtrips, these preserve their triggering graphs.
    for parity, sanitize, remove in itertools.product((1, 3), (False, True), (False, True)):
        atoms = [
            dict(element="C", bonds=[dict(neighbor=1, kind=2), dict(neighbor=2, kind=1)]),
            dict(element="C", bonds=[dict(neighbor=3, kind=1)]),
            dict(element="F"),
            dict(element="Cl"),
        ]
        record = dict(central_atom=None, kind=1, parity=parity, neighbors=[2, 0, 1, 3])
        raw = dict(atoms=atoms, stereo=[record, record])
        yield (
            dict(
                name=f"duplicate-boundary/{parity}/{sanitize}/{remove}",
                operation="duplicate_boundary",
                sanitize=sanitize,
                remove=remove,
            ),
            native_command(raw, sanitize, remove),
        )
    cleanup = [
        "C1=NN=[N-]=N1",
        "C[N](=C)(=O)",
        "C[N](=N)(=O)",
        "C[N](=C)(=C)",
        "C[N](=[Si-])=[Si-]",
        "CN1=CCN=CC=1",
        "CN1=NCOCC=1",
        "[N]=C1N=CN=N1",
        "[N]=C1C=CN=N1",
        "C[S-](=O)(=O)=O",
        "[S-](=N)(=C)(=C)C",
        "[Cl-](=O)(=O)(=O)=O",
        "Cl#S",
        "Br#[Se]",
        "CC(C1=CC=CC=N1=C2C(OC)=O)CC2=[OH+]",
        "[N-](=N)(C)C",
        "[N](=[N+])(C)(C)C",
        "[N](#C[N-])(C)C",
        "[N](=N)(C)(C)C",
        "[N](=O)(C)(C)C",
        "[N](=CC=O)(C)(C)C",
        "CN1=NCOC(=O)C=1",
        "[NH3]=C1N=CN=N1",
        "[NH3]=C1C=CN=N1",
        "[N](=CC=N=N)(C)(C)C",
        "[N](=C)(C)(C)C",
        "[N](=CC=O)(=CC=[OH+])C",
        "[S-](=CC#N)(C)(C)(C)(C)C",
        "[S-](=N)(C)(C)(C)(C)C",
        "[S-](=CC=N)(C)(C)(C)(C)C",
        "[S-](=O)(=O)(=O)F",
    ]
    rng = random.Random(738142)
    for text in cleanup:
        mol = Chem.MolFromSmiles(text, sanitize=False)
        if mol is None:
            raise ValueError(text)
        mol.UpdatePropertyCache(strict=False)
        for _ in range(25):
            order = list(range(mol.GetNumAtoms()))
            rng.shuffle(order)
            ordered = Chem.RenumberAtoms(mol, order)
            bonds = list(ordered.GetBonds())
            rng.shuffle(bonds)
            shuffled = Chem.RWMol()
            for atom in ordered.GetAtoms():
                shuffled.AddAtom(Chem.Atom(atom))
            for bond in bonds:
                a, b = bond.GetBeginAtomIdx(), bond.GetEndAtomIdx()
                if rng.randrange(2):
                    a, b = b, a
                shuffled.AddBond(a, b, bond.GetBondType())
                shuffled.GetBondBetweenAtoms(a, b).SetIsAromatic(bond.GetIsAromatic())
            shuffled.UpdatePropertyCache(strict=False)
            yield (
                dict(name=f"cleanup/{text}/{order}", operation="cleanup"),
                "C " + shuffled.ToBinary(Chem.PropertyPickleOptions.AllProps).hex(),
            )
    for filler_count in (999, 1000):
        mol = Chem.RWMol()
        for _ in range(filler_count):
            a = mol.AddAtom(Chem.Atom(7))
            b = mol.AddAtom(Chem.Atom(7))
            mol.AddBond(a, b, Chem.BondType.DOUBLE)
        tail = Chem.MolFromSmiles("[NH3]=CC=N=N", sanitize=False)
        offset = mol.GetNumAtoms()
        for atom in tail.GetAtoms():
            mol.AddAtom(Chem.Atom(atom))
        for bond in tail.GetBonds():
            mol.AddBond(
                offset + bond.GetBeginAtomIdx(), offset + bond.GetEndAtomIdx(), bond.GetBondType()
            )
        mol.UpdatePropertyCache(strict=False)
        yield (
            dict(name=f"cleanup/native-match-limit/{filler_count}", operation="cleanup"),
            "C " + mol.ToBinary(Chem.PropertyPickleOptions.AllProps).hex(),
        )


def generate(args):
    libs = (
        Path(rdkit.__file__).parent / ".dylibs"
        if sys.platform == "darwin"
        else Path(rdkit.__file__).parent.parent / "rdkit.libs"
    )
    loader = "DYLD_LIBRARY_PATH" if sys.platform == "darwin" else "LD_LIBRARY_PATH"
    process = subprocess.Popen(
        [str(args.oracle)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
        env={**os.environ, loader: str(libs)},
    )
    assert process.stdin is not None and process.stdout is not None
    records = [
        dict(
            rdkit_version=rdBase.rdkitVersion,
            rdkit_source="0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985",
            inchi_version="1.07.3",
            adapter_sha256="68c9b20d1d5920ed602ea931c1429395280c3d040971593073917618015183d1",
            capture_sha256=hashlib.sha256(
                (ROOT / "tests/inchi_output_reference.cpp").read_bytes()
            ).hexdigest(),
        )
    ]
    for case, command in cases(args.rdkit_source):
        process.stdin.write(command + "\n")
        process.stdin.flush()
        line = process.stdout.readline()
        if not line:
            raise RuntimeError(f"Native capture exited at {case['name']}")
        captured = json.loads(line)
        case["raw"] = captured["raw"]
        case["stages"] = {name: decode_state(state) for name, state in captured["stages"].items()}
        case["expected"] = decode_state(captured["final"])
        case["error"] = captured["error"]
        case["cleanup_rules"] = captured["cleanup_rules"]
        if case["operation"] == "import":
            try:
                mol, status, message, log = rdinchi.InchiToMol(
                    case["inchi"], sanitize=case["sanitize"], removeHs=case["remove"]
                )
            except (ValueError, RuntimeError):
                if case["expected"] is not None:
                    raise
            else:
                assert (status, message, log) == (
                    case["raw"]["status"],
                    case["raw"]["message"],
                    case["raw"]["log"],
                ), case["name"]
                if mol is None:
                    assert case["expected"] is None, case["name"]
                else:
                    assert snapshot(mol, "symmetric") == case["expected"], case["name"]
        records.append(case)
    process.stdin.close()
    assert process.wait() == 0
    data = "".join(json.dumps(record, separators=(",", ":")) + "\n" for record in records).encode()
    FIXTURE.write_bytes(gzip.compress(data, mtime=0))
    print(f"Captured {len(records) - 1} cases; {len(data)} decoded bytes", file=sys.stderr)


def main():
    rdBase.DisableLog("rdApp.*")
    assert rdBase.rdkitVersion == "2026.03.6"
    parser = argparse.ArgumentParser()
    parser.add_argument("--oracle", type=Path)
    parser.add_argument("--rdkit-source", type=Path)
    args = parser.parse_args()
    if args.oracle:
        generate(args)
    else:
        with gzip.open(FIXTURE, "rt") as fixture:
            for line in fixture:
                sys.stdout.write(line)


if __name__ == "__main__":
    main()
