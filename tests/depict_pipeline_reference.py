"""Bounded public Compute2DCoords reference; never invokes the application engine.

Checked inputs are portable native pickles plus independent chemical snapshots.
Live expected results are generated on the current host. Native failures end a
worker immediately: the pinned wrapper does not restore BOND_LEN on exception.
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
import sys
import tempfile
import time
from pathlib import Path

PIN = "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985"
VERSION = "2026.03.6"
ROOT = Path(__file__).resolve().parents[1]
INPUTS = ROOT / "tests/fixtures/depict-pipeline-inputs.json.gz"
MAX_CASES = 8192
MAX_ATOMS = 1024
MAX_BONDS = 4096
MAX_RECORD = 2 * 1024 * 1024
MAX_INPUT = 64 * 1024 * 1024
MAX_OUTPUT = 256 * 1024 * 1024
BATCH = 64
SOURCE_SHA256 = {
    "license.txt": "daeb8d194502cbcf34c05c39541a0d02be65bc9bada5b891c1974cd24e9fca30",
    "Code/GraphMol/Depictor/Wrap/rdDepictor.cpp": "062370d8b07b3b0a0d8834da4e1f97d4d7cbd9ab19736c4607621960dd4b6d38",
    "Code/GraphMol/Depictor/RDDepictor.cpp": "f1ae9e705405d5254575c595c09c5d2ee240d4d2abf05209dd42dad5ae99b31f",
    "Code/GraphMol/Depictor/RDDepictor.h": "b73674bd9bee4e1d7db337bc7361d7197e73cbeba3b10201f559c7c7b59608cb",
    "Code/GraphMol/Depictor/EmbeddedFrag.cpp": "a3c55426a09deb23443e53cb2b59b4056e9d312b8fdb1c749da5033bc5a1eb33",
    "Code/GraphMol/Depictor/EmbeddedFrag.h": "a45692bbffd2dff60aa608565dc98d366a2aae7cfb75eeb372f1cf35a8f3f0f4",
    "Code/GraphMol/Depictor/DepictUtils.cpp": "ba3868f339635889b1533fc6a706a5a1d8f6739846e1e25e66e7efa0617eddd5",
    "Code/GraphMol/Depictor/TemplateSmarts.h": "69530df08d9e532a2ce89275359333fe24a2c50868fe91f1942970bc03ee890b",
    "Code/GraphMol/Depictor/Templates.cpp": "28f458d261fedddca7582c3ad175d225a5b1d820f2dc5a14b66e9b0d0c62790f",
    "Code/GraphMol/Depictor/Templates.h": "a4870cc64682e1812dc5cd83028d19410dd4bdfd1876e59c719afba994879106",
    "Code/GraphMol/Matrices.cpp": "fd69c704c084f7e9fd8c2279024f94e9421338bcf556ca45a55aba63937bb8ed",
    "Code/Geometry/Transform2D.cpp": "c8cf18d72544c276836d74a17312b7d425ed2cbeed67409ca09fe9b5c80d1a4f",
    "Code/Geometry/point.h": "aa985364528748f3c94b0e0f969fe63cbeb2b830e93df26c5b22f8cee8bfb393",
    "Code/Numerics/Vector.h": "e8821d0e72cca0b861339ba3b3074e957f436ca8687777593c0ec0fd40cfaa2a",
}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def bits(value):
    return struct.pack(">d", value).hex()


def number(text):
    if not isinstance(text, str) or len(text) != 16:
        raise ValueError("Invalid f64 encoding")
    value = struct.unpack(">d", bytes.fromhex(text))[0]
    if not math.isfinite(value) or abs(value) > 1e8:
        raise ValueError("Native reference inputs require finite coordinates within 1e8")
    return value


def encoded(value):
    result = json.dumps(value, separators=(",", ":"), ensure_ascii=True, allow_nan=False).encode()
    if len(result) > MAX_RECORD:
        raise ValueError("Reference record exceeds its transport limit")
    return result + b"\n"


def conformers(mol):
    return [
        {
            "id": conf.GetId(),
            "is_3d": conf.Is3D(),
            "positions": [
                list(map(bits, conf.GetAtomPosition(index))) for index in range(mol.GetNumAtoms())
            ],
        }
        for conf in mol.GetConformers()
    ]


def properties(mol):
    def props(obj):
        return {
            name: obj.GetProp(name)
            for name in sorted(obj.GetPropNames(includePrivate=True, includeComputed=False))
        }

    return {
        "molecule": props(mol),
        "atoms": [props(atom) for atom in mol.GetAtoms()],
        "bonds": [props(bond) for bond in mol.GetBonds()],
    }


def atom_data(mol):
    return [
        {
            "hybridization": str(atom.GetHybridization()),
            "cip_rank": atom.GetUnsignedProp("_CIPRank") if atom.HasProp("_CIPRank") else None,
            "chiral_rank": atom.GetUnsignedProp("_chiralAtomRank")
            if atom.HasProp("_chiralAtomRank")
            else None,
        }
        for atom in mol.GetAtoms()
    ]


def check_request(case):
    if not isinstance(case, dict) or not isinstance(case.get("name"), str):
        raise ValueError("Invalid named request")
    state = case["state"]
    count = len(state["graph"]["atoms"])
    if count > MAX_ATOMS or len(state["graph"]["bonds"]) > MAX_BONDS:
        raise ValueError("Native reference graph limit")
    pickle = case["pickle"]
    if not isinstance(pickle, str) or len(pickle) > MAX_RECORD // 2 or len(pickle) % 2:
        raise ValueError("Native pickle limit")
    options = case["options"]
    for key in ("canon_orient", "use_ring_templates"):
        if not isinstance(options[key], bool):
            raise ValueError("Boolean layout option required")
    if options["clear_confs"] is not True or options["force_rdkit"] is not True:
        raise ValueError("Only the original RDKit clear-conformer path is captured")
    if options["n_samples"] != 0 or options["n_flips_per_sample"] != 0:
        raise ValueError("Random sampling is outside this reference contract")
    if options["bond_length"] is not None and not 1e-12 <= number(options["bond_length"]) <= 1e6:
        raise ValueError("Native reference requires a positive bounded bond length")
    coordinates = options["coordinates"]
    if coordinates is not None:
        if not isinstance(coordinates, list) or len(coordinates) > count:
            raise ValueError("Coordinate-map limit")
        seen = set()
        for index, x, y in coordinates:
            if type(index) is not int or not 0 <= index < count or index in seen:
                raise ValueError("Coordinate map requires unique in-range atom indices")
            seen.add(index)
            number(x)
            number(y)
    return count


def rejection_kind(exception_type, message):
    """Classify native causes without platform paths, source lines or Boost text."""
    if exception_type == "RuntimeError" and message == "Cannot normalize a zero length vector":
        return "zero_length_vector"
    if (
        exception_type == "RuntimeError"
        and message.startswith("Range Error\n")
        and "ROMol.cpp" in message
        and "Failed Expression: 4294967295 < " in message
    ):
        return "missing_fragment_neighbor"
    return "unclassified_native_exception"


def observe(case):
    from perception_reference import snapshot
    from rdkit import Chem, rdBase
    from rdkit.Chem import rdDepictor
    from rdkit.Geometry import Point2D

    if rdBase.rdkitVersion != VERSION:
        raise ValueError("Native version differs from the pinned reference")
    count = check_request(case)
    molecule = Chem.Mol(bytes.fromhex(case["pickle"]))
    if molecule is None or molecule.GetNumAtoms() != count:
        raise ValueError("Invalid native reference pickle")
    if json.loads(encoded(snapshot(molecule, case["ring_kind"]))) != case["state"]:
        raise ValueError(f"Pickle and original chemical snapshot differ: {case['name']}")
    if (
        conformers(molecule) != case["before_conformers"]
        or properties(molecule) != case["before_properties"]
    ):
        raise ValueError(f"Pickle and original coordinates/properties differ: {case['name']}")
    options = case["options"]
    kwargs = {
        "canonOrient": options["canon_orient"],
        "clearConfs": True,
        "forceRDKit": True,
        "useRingTemplates": options["use_ring_templates"],
        "nSample": 0,
        "nFlipsPerSample": 0,
    }
    if options["bond_length"] is not None:
        kwargs["bondLength"] = number(options["bond_length"])
    if options["coordinates"] is not None:
        kwargs["coordMap"] = {
            index: Point2D(number(x), number(y)) for index, x, y in options["coordinates"]
        }
    result = {
        "success": True,
        "conformer_id": None,
        "exception_type": None,
        "message": None,
        "rejection_kind": None,
    }
    try:
        result["conformer_id"] = rdDepictor.Compute2DCoords(molecule, **kwargs)
    except (ValueError, RuntimeError, OverflowError) as error:
        result.update(success=False, exception_type=type(error).__name__, message=str(error)[:4096])
        result["rejection_kind"] = rejection_kind(result["exception_type"], result["message"])
    result["conformers"] = conformers(molecule)
    result["state"] = snapshot(molecule, case["ring_kind"])
    result["properties"] = properties(molecule)
    result["atom_data"] = atom_data(molecule)
    result["finite"] = all(
        math.isfinite(struct.unpack(">d", bytes.fromhex(value))[0])
        for conf in result["conformers"]
        for position in conf["positions"]
        for value in position
    )
    return result


def worker():
    from rdkit import RDLogger

    RDLogger.DisableLog("rdApp.*")
    for _ in range(BATCH):
        line = sys.stdin.buffer.readline(MAX_RECORD + 1)
        if not line:
            return
        if len(line) > MAX_RECORD or not line.endswith(b"\n"):
            raise ValueError("Worker input record limit")
        result = observe(json.loads(line))
        sys.stdout.buffer.write(encoded(result))
        sys.stdout.buffer.flush()
        if not result["success"]:
            # The native Python wrapper fails to restore global BOND_LEN on
            # exception. Exit before another request can observe that state.
            return
    if sys.stdin.buffer.read(1):
        raise ValueError("Worker batch limit")


def read_records(path, max_expanded=MAX_OUTPUT):
    if path.stat().st_size > MAX_INPUT:
        raise ValueError("Compressed input limit")
    with gzip.open(path, "rb") as stream:
        raw = stream.read(max_expanded + 1)
    if len(raw) > max_expanded:
        raise ValueError("Expanded input limit")
    lines = raw.splitlines()
    if not lines or len(lines) > MAX_CASES + 1 or any(len(line) > MAX_RECORD for line in lines):
        raise ValueError("Input record count or size limit")
    header, *cases = (json.loads(line) for line in lines)
    if (
        header["provenance"]["commit"] != PIN
        or header["provenance"]["source_sha256"] != SOURCE_SHA256
    ):
        raise ValueError("Pinned input provenance mismatch")
    return header, cases


def run_batch(batch):
    if not batch or len(batch) > BATCH:
        raise ValueError("Native batch size limit")
    request = b"".join(encoded(case) for case in batch)
    with tempfile.TemporaryFile() as output, tempfile.TemporaryFile() as errors:
        process = subprocess.run(
            [sys.executable, str(Path(__file__).resolve()), "--worker"],
            input=request,
            stdout=output,
            stderr=errors,
            timeout=60,
            check=False,
        )
        size = output.tell()
        if size > BATCH * MAX_RECORD:
            raise ValueError("Native output transport limit")
        errors.seek(0)
        diagnostic = errors.read(8192).decode(errors="replace")
        if process.returncode != 0:
            raise RuntimeError(f"Native worker failed at {batch[0]['name']}: {diagnostic}")
        output.seek(0)
        results = [json.loads(line) for line in output.read().splitlines()]
    if not results or len(results) > len(batch):
        raise ValueError("Native worker returned an invalid record count")
    if len(results) < len(batch) and results[-1]["success"]:
        raise ValueError("Native worker stopped without recording an exception")
    return results, size


def has_coordination_seed(case):
    return any(atom["chiral_tag"] in (6, 7, 8) for atom in case["state"]["metadata"]["atoms"])


def live(cases):
    position = 0
    started = time.monotonic()
    total_output = 0
    while position < len(cases):
        if time.monotonic() - started > 600:
            raise TimeoutError("Reference run exceeded ten minutes")
        batch = cases[position : position + BATCH]
        # SP/TBP/OH idealPoints are process-local statics initialized with the
        # first BOND_LEN. Give each such request a fresh process. Ordinary
        # requests may share a worker only until the next coordination seed.
        for index, case in enumerate(batch):
            if has_coordination_seed(case):
                batch = batch[: max(index, 1)]
                break
        results, size = run_batch(batch)
        total_output += size
        if total_output > MAX_OUTPUT:
            raise ValueError("Native output transport limit")
        for case, expected in zip(batch, results, strict=False):
            yield {**case, "expected": expected}
        position += len(results)


def seed_cache_audit(cases):
    by_name = {case["name"]: case for case in cases}
    for kind, index in [("sp", 36), ("tbp", 38), ("oh", 39)]:
        original = by_name[f"curated/{index}/True/False/None"]
        variants = []
        for length in (0.5, None, 2.75):
            case = json.loads(encoded(original))
            case["name"] = f"seed-cache/{kind}/{length}"
            case["options"]["bond_length"] = None if length is None else bits(length)
            variants.append(case)
        fresh = [run_batch([case])[0][0] for case in variants]
        sequences = []
        for order in ([0, 1, 2], [2, 1, 0]):
            results, _ = run_batch([variants[i] for i in order])
            if len(results) != len(order) or any(not value["success"] for value in results):
                raise ValueError("Seed cache audit must contain successful native calls")
            sequences.append(
                {
                    "order": order,
                    "results": results,
                    "matches_fresh": [
                        value == fresh[i] for value, i in zip(results, order, strict=True)
                    ],
                }
            )
        yield {
            "name": f"seed-cache/{kind}",
            "inputs": variants,
            "fresh": fresh,
            "sequences": sequences,
        }


def requests(source):
    from perception_reference import snapshot
    from rdkit import Chem, RDConfig, RDLogger, rdBase

    if rdBase.rdkitVersion != VERSION:
        raise ValueError("Generate inputs with the pinned native version")
    RDLogger.DisableLog("rdApp.*")
    rng = random.Random(637821)

    def emit(
        mol,
        name,
        canonical=True,
        templates=False,
        length=None,
        coords=None,
        old=False,
        kind="symmetric",
    ):
        mol = Chem.Mol(mol)
        mol.UpdatePropertyCache(strict=False)
        if kind == "none":
            mol.ClearComputedProps(includeRings=True)
            mol.UpdatePropertyCache(strict=False)
        else:
            Chem.GetSymmSSSR(mol)
        if old:
            for ordinal in range(2):
                conf = Chem.Conformer(mol.GetNumAtoms())
                conf.SetId(7 + ordinal * 12)
                conf.Set3D(ordinal == 1)
                for index in range(mol.GetNumAtoms()):
                    conf.SetAtomPosition(
                        index, (index * 0.75 + 0.1, (index % 3) * 0.4 - 0.2, ordinal * 0.25)
                    )
                mol.AddConformer(conf, assignId=False)
            mol.SetProp("_Name", "original drawing")
            mol.SetProp("layout_test_note", "保存する")
            if mol.GetNumAtoms():
                mol.GetAtomWithIdx(0).SetProp("atomLabel", "R1")
                mol.GetAtomWithIdx(0).SetProp("reshiki_id", "51")
        case = {
            "name": name,
            "ring_kind": kind,
            "state": snapshot(mol, kind),
            "atom_data": atom_data(mol),
            "pickle": mol.ToBinary(
                Chem.PropertyPickleOptions.AllProps | Chem.PropertyPickleOptions.CoordsAsDouble
            ).hex(),
            "options": {
                "canon_orient": canonical,
                "use_ring_templates": templates,
                "bond_length": None if length is None else bits(length),
                "coordinates": None
                if coords is None
                else [[i, bits(x), bits(y)] for i, x, y in coords],
                "clear_confs": True,
                "force_rdkit": True,
                "n_samples": 0,
                "n_flips_per_sample": 0,
            },
            "before_conformers": conformers(mol),
            "before_properties": properties(mol),
        }
        check_request(case)
        return case

    curated = [
        "",
        "C",
        "O",
        "[Na+]",
        "CC",
        "CCC",
        "CCCCCC",
        "CC(C)CC(C)(C)C",
        "COC(N)=O",
        "C/C=C/C",
        "C/C=C\\C",
        "C/C=C/C=C\\C",
        "FC(Cl)=C(Br)I",
        "C=C=C",
        "N#CC#N",
        "C[C@H](O)[C@@H](N)C(F)(Cl)Br",
        "C[C@H]1CCC[C@@H](C)C1",
        "C[C@H](O)C(=O)O |a:1|",
        "C[C@H](O)[C@@H](F)Cl |o1:1,3|",
        "c1ccccc1",
        "n1ccccc1",
        "c1ccc2[nH]ccc2c1",
        "CC(=O)Oc1ccccc1C(=O)O",
        "C1CC1",
        "C1CCC1",
        "C1CCCC1",
        "C1CCCCC1",
        "C1CCCCCC1",
        "C1CCCCCCCCCCC1",
        "C1CCCCCCCCCCCCCCCCCCC1",
        "CC1CCC2(CC1)CCC(C)C2",
        "C1CCC2(CC1)CCCC2",
        "C1CC2CCC1C2",
        "C12C3C4C1C5C2C3C45",
        "C1C2CC3CC1CC(C2)C3",
        "C1CCC2CCC3CCC4CCC1C2C34",
        "[Pt@SP1](Cl)(Br)(N)I",
        "[Pt@SP3](Cl)(Br)(N)I",
        "[As@TB1](F)(Cl)(Br)(I)N",
        "[Co@OH1](F)(Cl)(Br)(I)(N)O",
        "[Co@OH30](F)(Cl)(Br)(I)(N)O",
        "[Cu+2](<-N)(<-N)(<-N)<-N",
        "[Fe](C)(C)(C)(C)(C)C",
        "CC.CCCC",
        "[Na+].[Cl-].O",
        "C.C.C.C",
        "CC.[Pt@SP1](Cl)(Br)(N)I",
        "CC.O=c1cc[nH]c(=O)[nH]1.C[C@@H](O)C(=O)O",
        "[2H]OC([13CH3])=O",
        "[CH3:5][OH:8]",
        "CCO |(0,0,;1,0,;2,1,),$Me;;OH$,atomProp:1.note.anchor|",
        "C[C@H](F)O |(0,0,1;1,0,2;1,1,3;2,0,4),atomProp:0.note.keep|",
    ]
    for index, text in enumerate(curated):
        mol = Chem.MolFromSmiles(text)
        if mol is None:
            raise ValueError(f"Invalid curated input {text}")
        for canonical in (False, True):
            for templates in (False, True):
                for map_kind in (None, []):
                    yield emit(
                        mol,
                        f"curated/{index}/{canonical}/{templates}/{map_kind}",
                        canonical,
                        templates,
                        coords=map_kind,
                    )
        for length in (1.5, 14.4 / 28.0, 1e-6, 1e6):
            yield emit(mol, f"scale/{index}/{length}", False, True, length=length, old=True)
        count = mol.GetNumAtoms()
        if count:
            maps = [
                ("single", [(count - 1, 2.25, -3.5)]),
                ("all", [(i, i * 0.75 + 0.1, (i % 3) * 0.4 - 0.2) for i in range(count)]),
            ]
            if count > 1:
                maps += [
                    ("bond", [(0, 0.1, -0.2), (1, 1.3, 0.7)]),
                    (
                        "cleanup",
                        [
                            (i, i * 0.75 + 0.1, (i % 3) * 0.4 - 0.2)
                            for i in range(count)
                            if i % 3 != 1
                        ],
                    ),
                    ("coincident", [(0, 0.1, -0.2), (1, 0.1, -0.2)]),
                ]
            for label, coordinates in maps:
                for canonical in (False, True):
                    yield emit(
                        mol,
                        f"fixed/{index}/{label}/{canonical}",
                        canonical,
                        True,
                        length=14.4 / 28.0,
                        coords=coordinates,
                        old=True,
                    )
        if index % 3 == 0:
            yield emit(mol, f"missing-ring-cache/{index}", kind="none")
        if count > 3:
            order = list(range(count))
            rng.shuffle(order)
            yield emit(Chem.RenumberAtoms(mol, order), f"permutation/{index}", False, True)
    # Every built-in graph, without filtering by current Rust acceptance.
    strings = [
        json.loads(line.strip().rstrip(","))
        for line in (source / "Code/GraphMol/Depictor/TemplateSmarts.h").read_text().splitlines()
        if line.lstrip().startswith('"')
    ]
    if len(strings) != 578:
        raise ValueError("Pinned builtin template count changed")
    for ordinal, text in enumerate(strings):
        query = Chem.MolFromSmarts(text)
        mol = Chem.RWMol()
        for _ in query.GetAtoms():
            atom = Chem.Atom(6)
            atom.SetNoImplicit(True)
            mol.AddAtom(atom)
        for bond in query.GetBonds():
            mol.AddBond(bond.GetBeginAtomIdx(), bond.GetEndAtomIdx(), Chem.BondType.SINGLE)
        yield emit(mol, f"builtin/{ordinal}", canonical=ordinal % 2 == 0, templates=True)
        if ordinal % 5 == 0:
            yield emit(mol, f"builtin-no-template/{ordinal}", templates=False)
    # Deterministic coverage spread over the complete bundled NCI input file.
    lines = (Path(RDConfig.RDDataDir) / "NCI/first_5K.smi").read_text().splitlines()
    for index in range(0, len(lines), 6):
        mol = Chem.MolFromSmiles(lines[index].split()[0])
        if mol is None:  # Native sanitizer rejects these before depiction.
            continue
        yield emit(mol, f"nci/{index}", index % 4 < 2, index % 4 >= 2)
        if index % 120 == 0:
            count = mol.GetNumAtoms()
            coordinates = [
                (i, i * 0.75 + 0.1, (i % 3) * 0.4 - 0.2) for i in range(count) if i % 4 == 0
            ]
            yield emit(mol, f"nci-cleanup/{index}", False, True, 14.4 / 28.0, coordinates, old=True)
    # A native exception after setting bondLength must not contaminate the
    # following default-length reference, nor the future Rust solver session.
    regression = Chem.MolFromSmiles("CCC")
    yield emit(
        regression,
        "sequence/custom-length-failure",
        False,
        True,
        0.25,
        [(0, 0.1, -0.2), (1, 0.1, -0.2)],
        old=True,
    )
    yield emit(regression, "sequence/default-after-failure")
    for count in (64, 256):
        yield emit(Chem.MolFromSmiles("C" * count), f"long-chain/{count}", templates=True)
        yield emit(
            Chem.MolFromSmiles(".".join(["CCO"] * (count // 3))),
            f"many-components/{count}",
            False,
            True,
        )

    # Drawing-supported bond styles are distinct native graph orders, not
    # approximated single bonds. Keep ordinary molecular caches on the source.
    styles = (
        ("hydrogen", Chem.BondType.HYDROGEN),
        ("quadruple", Chem.BondType.QUADRUPLE),
        ("partial", Chem.BondType.ONEANDAHALF),
    )
    graphs = (
        ("pair", "CC", 0),
        ("chain", "C[Cr][Cr]C", 1),
        ("branch", "C[Cr](C)[Cr]C", 2),
        ("ring", "[Cr]1[Cr]CCCC1", 0),
    )
    for style, bond_type in styles:
        for graph_name, smiles, bond_index in graphs:
            mol = Chem.MolFromSmiles(smiles)
            mol.GetBondWithIdx(bond_index).SetBondType(bond_type)
            mol.UpdatePropertyCache(strict=False)
            for templates in (False, True):
                for canonical in (False, True):
                    yield emit(
                        mol,
                        f"bond-style/{style}/{graph_name}/{templates}/{canonical}",
                        canonical,
                        templates,
                        old=True,
                    )
                yield emit(
                    mol,
                    f"bond-style/{style}/{graph_name}/{templates}/cleanup",
                    False,
                    templates,
                    14.4 / 28.0,
                    [(0, 0.1, -0.2), (1, 0.85, 0.2)],
                    old=True,
                )

    # The fallback property is independent of the legacy CIP cache. All these
    # strings are accepted by native unsigned conversion, including wrapping
    # negative values and trailing whitespace; invalid lazy values are covered
    # by the raw import-property contract instead of this typed-state adapter.
    for ordinal, text in enumerate(
        ("0", "+1", "-1", "4294967295", "-4294967295", "01", "1 ", "1\t")
    ):
        mol = Chem.MolFromSmiles("CC")
        for atom in mol.GetAtoms():
            if atom.HasProp("_CIPRank"):
                atom.ClearProp("_CIPRank")
        mol.GetAtomWithIdx(1).SetProp("_chiralAtomRank", text)
        for canonical in (False, True):
            yield emit(mol, f"rank-property/string/{ordinal}/{canonical}", canonical, old=True)
    for graph_name, smiles in (("branch", "CC(C)CC"), ("ring", "c1ccc(C(C)C)cc1CC")):
        for precedence in (False, True):
            for reverse in (False, True):
                mol = Chem.MolFromSmiles(smiles)
                count = mol.GetNumAtoms()
                for atom in mol.GetAtoms():
                    index = atom.GetIdx()
                    atom.SetUnsignedProp(
                        "_chiralAtomRank", (count - index if reverse else index) * 100_000
                    )
                    if precedence:
                        atom.SetUnsignedProp("_CIPRank", 3 * index + 7)
                    elif atom.HasProp("_CIPRank"):
                        atom.ClearProp("_CIPRank")
                for canonical in (False, True):
                    yield emit(
                        mol,
                        f"rank-property/{graph_name}/cip-{precedence}/reverse-{reverse}/{canonical}",
                        canonical,
                        old=True,
                    )


def provenance(source=None):
    import rdkit
    from rdkit import RDConfig, rdBase

    if rdBase.rdkitVersion != VERSION:
        raise ValueError("Pinned native version required")
    if source is not None:
        for relative, expected in SOURCE_SHA256.items():
            if digest(source / relative) != expected:
                raise ValueError(f"Pinned source hash differs: {relative}")
    package = Path(rdkit.__file__).parent
    libraries = package / ".dylibs" if sys.platform == "darwin" else package.parent / "rdkit.libs"
    paths = [
        p
        for p in libraries.iterdir()
        if any(
            name in p.name
            for name in ("RDKitDepictor", "RDKitGraphMol", "RDKitRDGeometryLib", "RDKitRDGeneral")
        )
    ]
    return {
        "provenance": {
            "version": VERSION,
            "commit": PIN,
            "source_sha256": SOURCE_SHA256,
            "platform": platform.platform(),
            "machine": platform.machine(),
            "python": sys.version,
            "library_sha256": {p.name: digest(p) for p in sorted(paths)},
            "nci_sha256": digest(Path(RDConfig.RDDataDir) / "NCI/first_5K.smi"),
            "observer_sha256": digest(Path(__file__)),
            "adapter_sha256": {
                name: digest(ROOT / "tests" / name)
                for name in (
                    "perception_reference.py",
                    "stereo_reference.py",
                    "kekulize_reference.py",
                    "ranking_reference.py",
                    "valence_reference.py",
                )
            },
        }
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--worker", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--generate-inputs", action="store_true")
    parser.add_argument("--live", action="store_true")
    parser.add_argument("--audit-seed-cache", action="store_true")
    parser.add_argument("--rdkit-source", type=Path)
    parser.add_argument("--inputs", type=Path, default=INPUTS)
    parser.add_argument("--fixture", type=Path, help="Dump a checked capture without loading RDKit")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--filter", help="Select a stable name substring for diagnosis")
    args = parser.parse_args()
    if args.worker:
        worker()
        return
    if args.fixture:
        header, cases = read_records(args.fixture)
    elif args.generate_inputs:
        if args.rdkit_source is None:
            raise ValueError("Input generation requires the pinned source path")
        header = provenance(args.rdkit_source)
        cases = list(requests(args.rdkit_source))
    elif args.live or args.audit_seed_cache:
        original, cases = read_records(args.inputs, MAX_INPUT)
        header = provenance(args.rdkit_source)
        header["provenance"]["input_provenance"] = original["provenance"]
        header["provenance"]["input_sha256"] = digest(args.inputs)
        if args.filter:
            cases = [case for case in cases if args.filter in case["name"]]
        if not cases:
            raise ValueError("No reference cases selected")
        cases = list(seed_cache_audit(cases) if args.audit_seed_cache else live(cases))
        header["provenance"]["worker_policy"] = (
            "deliberate same-process seed-cache audit"
            if args.audit_seed_cache
            else "fresh SP/TBP/OH request; other requests batched; restart immediately on native exception"
        )
        if not args.filter and not args.audit_seed_cache:
            by_name = {case["name"]: case["expected"] for case in cases}
            failed = by_name["sequence/custom-length-failure"]
            following = by_name["sequence/default-after-failure"]
            baseline = by_name["curated/5/True/False/None"]
            if failed["success"] or not following["success"] or following != baseline:
                raise ValueError("Custom-length failure contaminated the next default request")
    else:
        parser.error("Choose --generate-inputs, --live or --fixture")
    if len(cases) > MAX_CASES:
        raise ValueError("Reference case limit")
    header["provenance"]["case_count"] = len(cases)
    raw = encoded(header) + b"".join(encoded(case) for case in cases)
    if len(raw) > (MAX_INPUT if args.generate_inputs else MAX_OUTPUT):
        raise ValueError("Reference corpus limit")
    if args.output:
        args.output.write_bytes(gzip.compress(raw, mtime=0))
        print(f"{len(cases)} public layout cases; JSONL SHA256 {hashlib.sha256(raw).hexdigest()}")
    else:
        sys.stdout.buffer.write(raw)


if __name__ == "__main__":
    main()
