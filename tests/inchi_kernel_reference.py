"""Capture version changes from two independent official C InChI kernels.

Development-only oracle; no application imports this module. Inputs are the
previously captured native RDKit arrays, never arrays from the Rust port.
Compile the pinned official 1.07.5 source separately and pass both libraries.
"""

import argparse
import ctypes as c
import gzip
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REVISION = "11a87982bb518f57ac013f0b258c283655e1ea1d"
SOURCE_SHA256 = "7144e187e453c14bb3bc63f2f8215c527f6126054c592be18347e6797f13ed80"


class Atom(c.Structure):
    _fields_ = [
        ("x", c.c_double),
        ("y", c.c_double),
        ("z", c.c_double),
        ("neighbor", c.c_int16 * 20),
        ("bond_type", c.c_int8 * 20),
        ("bond_stereo", c.c_int8 * 20),
        ("element", c.c_char * 6),
        ("num_bonds", c.c_int16),
        ("hydrogens", c.c_int8 * 4),
        ("isotope", c.c_int16),
        ("radical", c.c_int8),
        ("charge", c.c_int8),
    ]


class Stereo(c.Structure):
    _fields_ = [
        ("neighbors", c.c_int16 * 4),
        ("central", c.c_int16),
        ("kind", c.c_int8),
        ("parity", c.c_int8),
    ]


class Input(c.Structure):
    _fields_ = [
        ("atoms", c.POINTER(Atom)),
        ("stereo", c.POINTER(Stereo)),
        ("options", c.c_char_p),
        ("atom_count", c.c_int16),
        ("stereo_count", c.c_int16),
    ]


class Output(c.Structure):
    _fields_ = [
        ("inchi", c.c_char_p),
        ("auxiliary", c.c_char_p),
        ("message", c.c_char_p),
        ("log", c.c_char_p),
    ]


def kernel(path):
    library = c.CDLL(str(path))
    library.GetINCHI.argtypes = [c.POINTER(Input), c.POINTER(Output)]
    library.GetINCHI.restype = c.c_int
    library.FreeINCHI.argtypes = [c.POINTER(Output)]
    library.FreeINCHI.restype = None
    return library


def generate(library, raw):
    atoms = (Atom * len(raw["atoms"]))()
    stereo = (Stereo * len(raw["stereo"]))()
    for atom, record in zip(atoms, raw["atoms"]):
        atom.x, atom.y, atom.z = record["position"]
        atom.element = record["element"].encode("ascii")
        atom.num_bonds = len(record["bonds"])
        atom.hydrogens[:] = record["hydrogens"]
        atom.isotope, atom.radical, atom.charge = (
            record["isotopic_mass"],
            record["radical"],
            record["charge"],
        )
        for i, bond in enumerate(record["bonds"]):
            atom.neighbor[i], atom.bond_type[i], atom.bond_stereo[i] = (
                bond["neighbor"],
                bond["kind"],
                bond["stereo"],
            )
    for item, record in zip(stereo, raw["stereo"]):
        item.neighbors[:] = record["neighbors"]
        item.central = record["central_atom"] if record["central_atom"] is not None else -1
        item.kind, item.parity = record["kind"], record["parity"]
    value = Input(atoms, stereo, b"", len(atoms), len(stereo))
    output = Output()
    status = library.GetINCHI(c.byref(value), c.byref(output))
    try:
        return dict(
            status=status,
            **{name: (getattr(output, name) or b"").decode() for name, _ in Output._fields_},
        )
    finally:
        library.FreeINCHI(c.byref(output))


def normalize_log(value):
    return "\n".join(line for line in value.splitlines() if " Build (" not in line).replace(
        "1.07.3", "1.07.5"
    )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--old-library", required=True, type=Path)
    parser.add_argument("--new-library", required=True, type=Path)
    parser.add_argument("--source-archive", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    assert hashlib.sha256(args.source_archive.read_bytes()).hexdigest() == SOURCE_SHA256
    fixture = ROOT / "tests/fixtures/inchi-input-native.json.gz"
    rows = json.loads(gzip.decompress(fixture.read_bytes()))["rows"]
    old, new = kernel(args.old_library), kernel(args.new_library)
    for library, version in ((old, "1.07.3"), (new, "1.07.5")):
        assert (
            f"Software {version} (API Library)"
            in generate(library, dict(atoms=[], stereo=[]))["log"]
        )
    changes = {}
    regressions = {}
    checked = 0
    for row in rows:
        raw = row["expected"]
        if raw is None:
            continue
        # These arrays invoke undefined stereo handling in the native adapter.
        # The main generation oracle excludes the same low-neighbor centers.
        state = row["state"]
        if any(
            a["chiral_tag"] in (1, 2)
            and sum(b["a"] == i or b["b"] == i for b in state["graph"]["bonds"]) < 3
            and 3
            <= sum(b["a"] == i or b["b"] == i for b in state["graph"]["bonds"])
            + state["graph"]["atoms"][i]["explicit_hydrogens"]
            + state["valences"][i]["implicit_hydrogens"]
            <= 4
            for i, a in enumerate(state["metadata"]["atoms"])
        ):
            continue
        a, b = generate(old, raw), generate(new, raw)
        a["log"], b["log"] = normalize_log(a["log"]), normalize_log(b["log"])
        checked += 1
        if row["name"].startswith(("perchlorate/", "bond/1/", "bond/2/")) or (
            row["name"].startswith("atom/")
            and len(state["graph"]["atoms"]) == 1
            and 81 <= state["graph"]["atoms"][0]["atomic_number"] <= 88
            and state["graph"]["atoms"][0]["isotope"] == 0
        ):
            regressions[row["name"]] = dict(state=state, positions=row["positions"], expected=b)
        if a != b:
            changes[row["name"]] = dict(old=a, new=b)
    capture = dict(
        old_library_sha256=hashlib.sha256(args.old_library.read_bytes()).hexdigest(),
        new_library_sha256=hashlib.sha256(args.new_library.read_bytes()).hexdigest(),
        old_version="1.07.3",
        new_version="1.07.5",
        official_revision=REVISION,
        archive_url=f"https://codeload.github.com/IUPAC-InChI/InChI/tar.gz/{REVISION}",
        archive_sha256=SOURCE_SHA256,
        input_sha256=hashlib.sha256(fixture.read_bytes()).hexdigest(),
        capture_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        checked=checked,
        changes=changes,
        regressions=regressions,
    )
    args.output.write_bytes(
        gzip.compress((json.dumps(capture, sort_keys=True) + "\n").encode(), mtime=0)
    )
    print(f"{checked} independent kernel comparisons; {len(changes)} version-specific changes")


if __name__ == "__main__":
    main()
