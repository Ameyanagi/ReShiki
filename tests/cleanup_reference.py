"""Unchanged complete cleanup responses plus independently observed native layouts."""

import copy
import json
import math
import struct
import sys
from pathlib import Path

from rdkit import RDLogger
from rdkit.Chem import rdDepictor

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from abbreviation_replacement_reference import decorated

from engine import worker


def f32(value):
    if isinstance(value, float):
        return struct.unpack("f", struct.pack("f", value))[0]
    if isinstance(value, list):
        return [f32(item) for item in value]
    if isinstance(value, dict):
        # Point fields are f32 in Request, including hand-built integer zeros.
        # serde_json::to_value promotes them to f64: Python must see 0.0, not 0,
        # because unary minus preserves different signed-zero behavior.
        return {
            key: f32(float(item)) if key in ("x", "y") else f32(item) for key, item in value.items()
        }
    return value


def imported(text):
    return worker.handle(dict(protocol=1, operation="import", format="smiles", text=text))[
        "document"
    ]


def points(mol):
    conf = mol.GetConformer()
    return [
        dict(x=p.x, y=p.y, z=p.z)
        for p in (conf.GetAtomPosition(i) for i in range(mol.GetNumAtoms()))
    ]


def run(request):
    try:
        return worker.handle(request), None
    except (ValueError, RuntimeError, KeyError, OverflowError) as error:
        return None, str(error)


def emit(name, document, scope="drawing", selected=(), keep=True):
    request = dict(
        protocol=1,
        operation="clean",
        document=f32(document),
        cleanup=dict(scope=scope, keep_orientation=keep),
        selected_ids=list(selected),
    )
    before = copy.deepcopy(request)
    # No hooks or monkey patches participate in the expected full response.
    expected, error = run(request)
    layouts = []
    analysis_positions = []
    original = rdDepictor.Compute2DCoords
    original_analyze = worker.analyze

    def observe_analysis(mol, **kwargs):
        analysis_positions.extend(points(mol))
        return original_analyze(mol, **kwargs)

    def observe(mol, **kwargs):
        def stereo():
            return (
                [int(a.GetChiralTag()) for a in mol.GetAtoms()],
                [
                    (int(b.GetBondDir()), int(b.GetStereo()), list(b.GetStereoAtoms()))
                    for b in mol.GetBonds()
                ],
            )

        before_stereo = stereo()
        record = dict(
            ids=[int(a.GetProp("reshiki_id")) for a in mol.GetAtoms()],
            old=points(mol),
            fixed={str(i): dict(x=p.x, y=p.y) for i, p in kwargs["coordMap"].items()},
            bond_length=kwargs["bondLength"],
            canonical_orientation=kwargs["canonOrient"],
            use_ring_templates=kwargs["useRingTemplates"],
            force_rdkit=kwargs["forceRDKit"],
        )
        result = original(mol, **kwargs)
        assert stereo() == before_stereo, "Native layout changed the cleanup molecule stereo state"
        record["positions"] = points(mol)
        layouts.append(record)
        return result

    try:
        rdDepictor.Compute2DCoords = observe
        worker.analyze = observe_analysis
        observed, observed_error = run(request)
    finally:
        rdDepictor.Compute2DCoords = original
        worker.analyze = original_analyze
    assert observed == expected and observed_error == error
    assert before == request
    print(
        json.dumps(
            dict(
                name=name,
                document=request["document"],
                options=request["cleanup"],
                selected=list(selected),
                layouts=layouts,
                expected=expected,
                error=error,
                analysis_positions=analysis_positions,
            )
        ),
        flush=True,
    )


def distort(doc, shift=0):
    for atom in doc["atoms"]:
        x, y = atom["position"].values()
        atom["position"] = dict(x=1.7 * x + 0.4 * y + shift, y=0.8 * y - 0.31 * shift)


def high_ids(doc):
    ids = {a["id"]: 9007199254741101 + 17 * i for i, a in enumerate(doc["atoms"])}
    for atom in doc["atoms"]:
        atom["id"] = ids[atom["id"]]
        if atom.get("stereo"):
            atom["stereo"]["neighbors"] = [ids[i] for i in atom["stereo"]["neighbors"]]
    for bond in doc["bonds"]:
        bond["a"], bond["b"] = ids[bond["a"]], ids[bond["b"]]
        bond["stereo_atoms"] = [ids[i] for i in bond.get("stereo_atoms", [])]
    return ids


def main():
    RDLogger.DisableLog("rdApp.*")
    for text in [
        "C",
        "CCO",
        "CCO.CN",
        "c1ccccc1CCC",
        "N[C@@H](C)C(=O)O",
        "F[C@](Cl)(Br)I",
        "F/C=C/C[C@H](Cl)Br",
        "C[S@](=O)CC",
        "[13CH3:41][NH3+]",
        "[2H]O[3H]",
        "C[C@H]1CC[C@@H](C)CC1",
        "CCC=CCC",
        "N->[Cu]<-N",
        "[Mo]$[Mo]",
    ]:
        for angle in [0.0, 0.81]:
            doc = imported(text)
            distort(doc, 117)
            for atom in doc["atoms"]:
                x, y = atom["position"].values()
                atom["position"] = dict(
                    x=x * math.cos(angle) - y * math.sin(angle),
                    y=x * math.sin(angle) + y * math.cos(angle),
                )
            ids = [a["id"] for a in doc["atoms"]]
            for scope, selected in [
                ("drawing", []),
                ("selected_molecules", ids[:1]),
                ("selected_atoms", ids[:1]),
                ("selected_atoms", ids[1:] or ids),
                ("selected_atoms", ids[::2]),
            ]:
                for keep in [True, False]:
                    emit(f"{text}/{angle}/{scope}/{selected}/{keep}", doc, scope, selected, keep)
    doc = imported("CCC=CCC")
    for i, atom in enumerate(doc["atoms"]):
        atom["position"] = dict(x=50 * i, y=0)
    emit("unspecified-collinear", doc)
    doc = imported("CC")
    for atom in doc["atoms"]:
        atom["element"] = "*"
    doc["bonds"][0]["order"] = 7
    emit("partial-bond", doc)
    for reverse in [False, True]:
        doc = decorated()
        if reverse:
            doc["atoms"].reverse()
            doc["bonds"].reverse()
        doc["drawing_style"] = copy.deepcopy(worker.drawing_styles.DEFAULT)
        doc["drawing_style"].update(bond_length_pt=28.8, bond_length_world=84.0)
        for scope in ["drawing", "selected_atoms", "selected_molecules"]:
            emit(f"decorated/{reverse}/{scope}", doc, scope, [doc["atoms"][0]["id"]])
    for display in ["wedge", "hash", "hollow_wedge", "hashed", "bold"]:
        doc = imported("F[C@](Cl)(Br)I")
        for bond in doc["bonds"]:
            if bond["display"] in ["wedge", "hash"]:
                bond["display"] = display
                bond.update(color=[23, 114, 56], z_order=-7, double_position="right")
        for scope in ["drawing", "selected_atoms"]:
            emit(f"wedged/{display}/{scope}", doc, scope, [a["id"] for a in doc["atoms"][1:]])
    grouped = worker.handle(
        dict(protocol=1, operation="abbreviate", document=imported("COc1ccc(NC(=O)OC(C)(C)C)cc1"))
    )["document"]
    for group in grouped["abbreviations"]:
        emit("abbreviation/" + group["label"], grouped, "selected_atoms", [group["anchor"]])
    for variant in ["valence", "aromatic", "radical", "charge", "element"]:
        for reverse in [False, True]:
            doc = imported("CCO.CC.[H]")
            bad = doc["atoms"][3 if variant != "charge" else 5]
            if variant == "valence":
                bad["explicit_h"] = 5
            if variant == "aromatic":
                bad["aromatic"] = True
            if variant == "radical":
                bad["radical_electrons"] = 1
            if variant == "charge":
                bad["charge"] = 2
            if variant == "element":
                bad["element"] = "Xx"
            if reverse:
                doc["atoms"].reverse()
                doc["bonds"].reverse()
            selected = high_ids(doc)[1] if reverse else 1
            emit(f"partial-warning/{variant}/{reverse}", doc, "selected_molecules", [selected])
            emit(f"whole-error/{variant}/{reverse}", doc)
    for reverse in [False, True]:
        doc = imported("CCO.C1CCCC1.CCCC")
        for atom in doc["atoms"][3:8]:
            atom["aromatic"] = True
        for bond in doc["bonds"]:
            if 4 <= bond["a"] <= 8:
                bond["order"] = 4
        if reverse:
            doc["atoms"].reverse()
            doc["bonds"].reverse()
        selected = high_ids(doc)[1] if reverse else 1
        emit(f"partial-warning/kekulize/{reverse}", doc, "selected_molecules", [selected])
    for reverse in [False, True]:
        doc = imported("CCO.CN")
        doc["bonds"][-1].update(order=0, display="dotted")
        if reverse:
            doc["atoms"].reverse()
            doc["bonds"].reverse()
        selected = high_ids(doc)[1] if reverse else 1
        emit(f"partial-warning/hydrogen/{reverse}", doc, "selected_molecules", [selected])
        doc = imported("CCO.CC=CC")
        next(b for b in doc["bonds"] if b["order"] == 2).update(stereo="z", stereo_atoms=[1, 3])
        if reverse:
            doc["atoms"].reverse()
            doc["bonds"].reverse()
        selected = high_ids(doc)[1] if reverse else 1
        emit(f"partial-warning/stereo-references/{reverse}", doc, "selected_molecules", [selected])
    doc = imported("CCO")
    for scope, selection in [
        ("selected_atoms", []),
        ("selected_molecules", [999]),
        ("drawing", [999]),
    ]:
        emit("selection-error", doc, scope, selection)
    emit("empty", worker.to_document(worker.Chem.MolFromSmiles("")))


if __name__ == "__main__":
    main()
