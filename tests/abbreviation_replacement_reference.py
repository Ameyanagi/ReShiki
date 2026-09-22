"""Original replace() oracle, independent native geometry, and final chemistry checks."""

import argparse
import copy
import json
import platform
import random
import struct
import sys
from pathlib import Path

from rdkit import Chem, RDLogger, rdBase
from rdkit.Chem import rdDepictor

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from abbreviation_detection_reference import document

from engine import abbreviations, worker


def geometry():
    templates = []
    for label, (smiles, _) in abbreviations.PRESETS.items():
        mol = Chem.MolFromSmiles(smiles)
        rdDepictor.Compute2DCoords(mol)
        positions = [
            dict(x=p.x, y=p.y, z=p.z)
            for p in [mol.GetConformer().GetAtomPosition(i) for i in range(mol.GetNumAtoms())]
        ]
        templates.append(dict(label=label, smiles=smiles, positions=positions))
    return dict(rdkit_version=rdBase.rdkitVersion, templates=templates)


def f32(value):
    return struct.unpack("f", struct.pack("f", value))[0]


def emit(name, doc, selection, label, check_chemistry=False):
    doc = copy.deepcopy(doc)
    for atom in doc["atoms"]:
        atom["position"] = {key: f32(value) for key, value in atom["position"].items()}
    before = copy.deepcopy(doc)
    try:
        expected = abbreviations.replace(doc, selection, label, worker.to_document)
        error = None
    except (ValueError, OverflowError) as exception:
        expected, error = None, str(exception)
    assert before == doc
    chemistry_ok = None
    if expected is not None and check_chemistry:
        try:
            worker.from_document(expected)
            chemistry_ok = True
        except (ValueError, RuntimeError):
            chemistry_ok = False
    print(
        json.dumps(
            dict(
                name=name,
                document=doc,
                selection=selection,
                label=label,
                expected=expected,
                error=error,
                chemistry_ok=chemistry_ok,
            )
        )
    )


def decorated():
    doc = document(Chem.MolFromSmiles("CCO.CN"), high_ids=True)
    # Keep high stable atom IDs, and make each figure category participate in
    # max-ID allocation in independent cases below.
    doc["annotations"] = [dict(id=100, position=dict(x=10, y=20), text="Caption α")]
    doc["arrows"] = [dict(id=101, start=dict(x=0, y=0), end=dict(x=100, y=0), kind="equilibrium")]
    doc["graphics"] = [
        dict(
            id=102,
            kind="ellipse",
            origin=dict(x=0, y=0),
            axis_x=dict(x=10, y=0),
            axis_y=dict(x=0, y=20),
        )
    ]
    ids = [a["id"] for a in doc["atoms"]]
    doc["groups"] = [
        dict(id=103, members=ids[:3] + [100], integral=True),
        dict(id=104, members=ids + [100, 101, 102]),
    ]
    for atom in doc["atoms"]:
        atom["display"] = dict(number=dict(text="n"))
        atom["text_style"] = dict(family="Arial", size_pt=12, bold=True, color=[100, 20, 30])
        atom["marks"] = [dict(kind="lone_pair", offset=dict(x=1, y=2))]
        atom["cip_label"] = "R"
    for bond in doc["bonds"]:
        bond.update(display="bold", color=[20, 40, 60], z_order=7, double_position="left")
    doc["reactions"] = [
        dict(
            arrow=101,
            reactants=[dict(atoms=ids[:3], coefficient=2)],
            products=[dict(atoms=ids[3:])],
            annotations=[100],
        )
    ]
    return doc


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument("--geometry", action="store_true")
    modes.add_argument("--wire", action="store_true")
    modes.add_argument("--write-geometry", action="store_true")
    parser.add_argument("--target-arch", choices=("x86_64", "aarch64"))
    args = parser.parse_args()
    if args.target_arch and not args.write_geometry:
        parser.error("--target-arch requires --write-geometry")
    if args.geometry:
        print(json.dumps(geometry()))
        return
    RDLogger.DisableLog("rdApp.*")
    if args.wire:
        for line in sys.stdin:
            request = json.loads(line)
            before = copy.deepcopy(request)
            edit = abbreviations.replace(
                request["document"], request["selected_ids"], request["text"], worker.to_document
            )
            response = worker.handle(request)
            assert before == request
            print(json.dumps(dict(request=request, edit=edit, response=response)), flush=True)
        return
    if args.write_geometry:
        system = platform.system().lower().replace("darwin", "macos")
        if system == "windows" and not args.target_arch:
            parser.error(
                "Choose --target-arch for the Windows host, not the emulated Python process"
            )
        arch = args.target_arch or (
            platform.machine().lower().replace("amd64", "x86_64").replace("arm64", "aarch64")
        )
        path = (
            Path(__file__).resolve().parents[1]
            / "src/chemistry/abbreviations"
            / f"geometry-{system}-{arch}.json"
        )
        path.write_text(json.dumps(geometry(), indent=2) + "\n")
        return
    print(json.dumps(geometry()))
    for label in abbreviations.PRESETS:
        # Native float64 calculations receive exact drawing float32 inputs.
        for origin in [(0, 0), (21.25, -17.5), (-234567.125, 456789.25), (f32(0.1), f32(-0.2))]:
            for direction in [
                (1, 0),
                (-1, 0),
                (0, 1),
                (0, -1),
                (0.6, 0.8),
                (-0.6, 0.8),
                (-0.6, -0.8),
                (0.6, -0.8),
            ]:
                for length in [0.15, 1.0, 42.0, 4200.0]:
                    doc = document(Chem.MolFromSmiles("CC"))
                    doc["atoms"][0]["position"] = dict(x=origin[0], y=origin[1])
                    doc["atoms"][1]["position"] = dict(
                        x=f32(origin[0] + direction[0] * length),
                        y=f32(origin[1] + direction[1] * length),
                    )
                    emit(f"geometry/{label}/{origin}/{direction}/{length}", doc, [1], label)
        for origin in [(0, 0), (f32(0.1), f32(-0.2)), (250.5, -350.25)]:
            doc = document(Chem.MolFromSmiles("[13CH4:6]"))
            doc["atoms"][0]["position"] = dict(x=origin[0], y=origin[1])
            emit(f"isolated/{label}/{origin}", doc, [1], label, True)
        doc = decorated()
        target = doc["atoms"][0]["id"]
        emit(f"decorated/{label}", doc, [target], label, True)
        for role in ["reactants", "products", "agents"]:
            doc = decorated()
            reaction = doc["reactions"][0]
            participants = reaction.pop("reactants")
            reaction.pop("products")
            reaction[role] = participants
            emit(f"reaction-role/{label}/{role}", doc, [target], label, True)
        # The original graph can carry retained stereochemistry elsewhere.
        for smiles in [
            "C[C@H](F)CO",
            "C/C=C/CC",
            "[13CH3:17]CO",
            "CC(=O)[O-]",
            "C[NH3+]",
            "C[SiH3]",
            "[CH2]C",
        ]:
            doc = document(Chem.MolFromSmiles(smiles))
            emit(f"chemical-context/{label}/{smiles}", doc, [1], label, True)
        for category in ["atoms", "annotations", "arrows", "graphics", "groups"]:
            doc = document(Chem.MolFromSmiles("CC.O"))
            large = 2**64 - 40
            if category == "atoms":
                doc["atoms"][-1]["id"] = large
            elif category == "annotations":
                doc[category] = [dict(id=large, position=dict(x=0, y=0), text="A")]
            elif category == "arrows":
                doc[category] = [dict(id=large, start=dict(x=0, y=0), end=dict(x=100, y=0))]
            elif category == "graphics":
                doc[category] = [
                    dict(
                        id=large,
                        kind="ellipse",
                        origin=dict(x=0, y=0),
                        axis_x=dict(x=10, y=0),
                        axis_y=dict(x=0, y=20),
                    )
                ]
            else:
                doc[category] = [dict(id=large, members=[1, 4])]
            emit(f"allocation/{label}/{category}", doc, [1], label)
        for maximum in [2**64 - 20, 2**64 - 2, 2**64 - 1]:
            doc = document(Chem.MolFromSmiles("C"))
            doc["annotations"] = [dict(id=maximum, position=dict(x=0, y=0), text="X")]
            emit(f"id-limit/{label}/{maximum}", doc, [1], label)
        for length in [0, 0.001, 0.149, 0.15, 0.151]:
            doc = document(Chem.MolFromSmiles("CC"))
            doc["atoms"][1]["position"]["x"] = f32(length)
            emit(f"geometry-boundary/{label}/{length}", doc, [1], label)
    # Every existing defined group can be replaced by every new definition.
    for old in abbreviations.PRESETS:
        initial = document(Chem.MolFromSmiles("NCC"))
        doc = abbreviations.replace(initial, [7], old, worker.to_document)
        selected = doc["abbreviations"][0]["members"]
        for new in abbreviations.PRESETS:
            emit(f"old-to-new/{old}/{new}", doc, selected, new, True)
        # Set semantics for existing group selection includes duplicates and order.
        emit(f"old-group-reversed/{old}", doc, list(reversed(selected)), "Me")
        emit(f"old-group-duplicates/{old}", doc, selected + selected, "OMe")
    for label in ["", "bad", "OMe", "Me"]:
        doc = document(Chem.MolFromSmiles("CCC.C"))
        for selection in [[], [999], [4], [1, 7], [1, 1], [1, 999], [10]]:
            emit(f"invalid-selection/{label}/{selection}", doc, selection, label)
    for order in [0, 2, 3, 4, 5, 6, 7]:
        doc = document(Chem.MolFromSmiles("CC"))
        doc["bonds"][0]["order"] = order
        emit(f"invalid-attachment/{order}", doc, [1], "OMe")
    for selection, anchor, members in [
        ([1, 4], 1, [1, 4]),
        ([1, 7], 1, [1, 7]),
        ([1, 4, 7], 1, [1, 4, 7]),
        ([1, 4], 4, [1, 4]),
    ]:
        doc = document(Chem.MolFromSmiles("CCCC"))
        doc["abbreviations"] = [dict(label="Custom", anchor=anchor, members=members)]
        emit(f"custom-group/{selection}/{anchor}", doc, selection, "Ph")
    # Retained invalid chemistry remains the caller's responsibility. Check the
    # same post-edit sanitization outcome without hiding a direct-edit success.
    for element, hydrogens in [("C", 5), ("O", 3), ("N", 5)]:
        doc = document(Chem.MolFromSmiles("CC.O"))
        doc["atoms"][-1].update(element=element, explicit_h=hydrogens, no_implicit=True)
        emit(f"invalid-chemistry/{element}/{hydrogens}", doc, [1], "OMe", True)
    rng = random.Random(310)
    for index in range(1500):
        doc = document(Chem.MolFromSmiles("CC"))
        for atom in doc["atoms"]:
            atom["position"] = {
                axis: f32(rng.uniform(-1, 1) * 10 ** rng.randint(-2, 5)) for axis in ["x", "y"]
            }
        label = rng.choice(list(abbreviations.PRESETS))
        emit(f"numeric-transform/{index}/{label}", doc, [1], label)
    for index in range(100):
        doc = decorated()
        rng.shuffle(doc["atoms"])
        rng.shuffle(doc["bonds"])
        target = min(a["id"] for a in doc["atoms"])
        label = rng.choice(list(abbreviations.PRESETS))
        emit(f"permuted/{index}/{label}", doc, [target], label)


if __name__ == "__main__":
    main()
