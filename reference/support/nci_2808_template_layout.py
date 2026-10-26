"""Independent, exact-source #538 layout expectation; never calls the Rust engine.

Run with the pinned development environment, for example:
  .venv/bin/python -B reference/support/nci_2808_template_layout.py

The frozen worker supplies the untouched default response. Public RDKit APIs
independently establish the template presentation, eligibility and identity.
"""

import hashlib
import itertools
import json
import math
import platform
import struct
import sys
from copy import deepcopy
from pathlib import Path

from rdkit import Chem, rdBase
from rdkit.Chem import rdDepictor, rdMolDescriptors

REFERENCE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REFERENCE))
from engine import worker  # noqa: E402

INPUT = "CC1(C)[CH]2CC[CH](CC(O)=O)[CH]1C2"
REACTION_INPUT = f"O>{INPUT}>O"
CONTROL = "C1C2CC3CC1CC(C2)C3"
RDKIT_VERSION = "2026.03.6"
CLEARANCE = 0.75


def points(molecule):
    conformer = molecule.GetConformer()
    return [tuple(conformer.GetAtomPosition(i)) for i in range(molecule.GetNumAtoms())]


def collision_pairs(molecule):
    positions = points(molecule)
    bonds = {
        tuple(sorted((bond.GetBeginAtomIdx(), bond.GetEndAtomIdx())))
        for bond in molecule.GetBonds()
    }
    return [
        [a, b]
        for a, b in itertools.combinations(range(len(positions)), 2)
        if (a, b) not in bonds and math.dist(positions[a][:2], positions[b][:2]) < CLEARANCE
    ]


def crossings(molecule):
    positions = points(molecule)
    bonds = [(b.GetBeginAtomIdx(), b.GetEndAtomIdx()) for b in molecule.GetBonds()]

    def cross(a, b, c):
        return (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])

    result = []
    for first, second in itertools.combinations(bonds, 2):
        if set(first) & set(second):
            continue
        a, b = (positions[i] for i in first)
        c, d = (positions[i] for i in second)
        if cross(a, b, c) * cross(a, b, d) < -1e-12 and cross(c, d, a) * cross(c, d, b) < -1e-12:
            result.append([list(first), list(second)])
    return result


def drawn_identity(molecule):
    # Ignore all stored tags and derive stereo from visible 2D bond directions.
    drawing = Chem.Mol(molecule)
    Chem.RemoveStereochemistry(drawing)
    Chem.AssignChiralTypesFromBondDirs(drawing, replaceExistingTags=True)
    Chem.AssignStereochemistry(drawing, cleanIt=True, force=True)
    return {
        "smiles": Chem.MolToSmiles(drawing, isomericSmiles=True),
        "inchikey": Chem.MolToInchiKey(drawing),
        "assigned_centers": Chem.FindMolChiralCenters(drawing, includeUnassigned=False),
    }


def response(text, format="smiles"):
    return worker.handle({"protocol": 1, "operation": "import", "format": format, "text": text})


def without_reaction_geometry(value):
    value = deepcopy(value)
    for atom in value["document"]["atoms"]:
        atom.pop("position")
    for arrow in value["document"]["arrows"]:
        arrow["start"].pop("x")
        arrow["end"].pop("x")
    return value


def reaction_agent_contract():
    """Keep frozen assembly; change only the exact agent's public depiction call."""
    original = response(REACTION_INPUT, "rsmi")
    canonical = Chem.MolToSmiles(Chem.MolFromSmiles(INPUT))
    compute = rdDepictor.Compute2DCoords
    calls = []

    def exact_agent_template(molecule, *args, **kwargs):
        if molecule.GetNumAtoms() == 13 and Chem.MolToSmiles(molecule) == canonical:
            assert not molecule.GetNumConformers()
            assert "useRingTemplates" not in kwargs
            kwargs["useRingTemplates"] = True
            calls.append(True)
        return compute(molecule, *args, **kwargs)

    # This process-local override does not modify the frozen worker or its row
    # assembly. The two water participants keep their ordinary depiction calls.
    try:
        rdDepictor.Compute2DCoords = exact_agent_template
        template = response(REACTION_INPUT, "rsmi")
    finally:
        rdDepictor.Compute2DCoords = compute
    assert calls == [True]
    assert without_reaction_geometry(original) == without_reaction_geometry(template)
    semantic = template["document"]["reactions"][0]
    assert semantic["agents"] == [{"atoms": list(range(2, 15)), "coefficient": 1}]
    assert semantic["reactants"] == [{"atoms": [1], "coefficient": 1}]
    assert semantic["products"] == [{"atoms": [16], "coefficient": 1}]
    assert len(template["document"]["atoms"]) == 15
    agent_y = [atom["position"]["y"] for atom in template["document"]["atoms"][1:14]]
    assert math.isclose(max(agent_y), -100, abs_tol=1e-10)
    return {
        "case_name": "molecule/NCI/2808/1",
        "input": REACTION_INPUT,
        "template_depiction_calls": len(calls),
        "assembly": "unchanged reference/engine/reactions.py::import_reaction",
        "agent_y_range": [min(agent_y), max(agent_y)],
        "original_response": original,
        "template_response": template,
    }


def generate():
    assert rdBase.rdkitVersion == RDKIT_VERSION
    assert Path(worker.__file__).resolve() == REFERENCE / "engine/worker.py"
    assert worker.SCALE == 28.0
    molecule = Chem.MolFromSmiles(INPUT)
    assert molecule.GetNumAtoms() == 13 and molecule.GetNumBonds() == 14
    assert rdMolDescriptors.CalcMolFormula(molecule) == "C11H18O2"
    assert Chem.MolToInchiKey(molecule) == "GEVYGCFPYCDBEW-UHFFFAOYSA-N"
    assert all(a.GetChiralTag() == Chem.ChiralType.CHI_UNSPECIFIED for a in molecule.GetAtoms())
    assert all(b.GetStereo() == Chem.BondStereo.STEREONONE for b in molecule.GetBonds())
    rings = [list(ring) for ring in Chem.GetSymmSSSR(molecule)]
    shared = [len(set(a) & set(b)) for a, b in itertools.combinations(rings, 2)]
    assert any(count >= 3 for count in shared)

    default, template = Chem.Mol(molecule), Chem.Mol(molecule)
    rdDepictor.Compute2DCoords(default, useRingTemplates=False)
    rdDepictor.Compute2DCoords(template, useRingTemplates=True)
    original_response = response(INPUT)
    for atom, (x, y, _) in zip(
        original_response["document"]["atoms"], points(default), strict=True
    ):
        assert atom["position"] == {"x": x * worker.SCALE, "y": -y * worker.SCALE}
    default_pairs, template_pairs = collision_pairs(default), collision_pairs(template)
    assert default_pairs == [[0, 5], [2, 5]] and template_pairs == []
    default_crossings, template_crossings = crossings(default), crossings(template)
    assert default_crossings == [] and template_crossings == []
    identity = drawn_identity(default)
    assert identity == drawn_identity(template)
    assert identity["assigned_centers"] == []
    assert identity["inchikey"] == "GEVYGCFPYCDBEW-UHFFFAOYSA-N"

    # The editable document stores single precision. This is a representation
    # conversion, not a relaxed comparison or coordinates borrowed from Rust.
    def f32(value):
        return struct.unpack("<f", struct.pack("<f", value))[0]

    template_positions = [
        {"x": f32(x * worker.SCALE), "y": f32(-y * worker.SCALE)} for x, y, _ in points(template)
    ]
    control = Chem.MolFromSmiles(CONTROL)
    rdDepictor.Compute2DCoords(control, useRingTemplates=False)
    assert collision_pairs(control) == []
    return {
        "contract": "NCI/2808 #538 independent ring-template presentation v1",
        "case_name": "NCI/2808",
        "input": INPUT,
        "rdkit_version": rdBase.rdkitVersion,
        "source_base": "8d3493d92582d57b6ad2ee07506f331db3b5535c",
        "provenance": {
            "command": ".venv/bin/python -B reference/support/nci_2808_template_layout.py",
            "python": platform.python_version(),
            "system": platform.system(),
            "machine": platform.machine(),
            "generator_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
            "frozen_worker_sha256": hashlib.sha256(Path(worker.__file__).read_bytes()).hexdigest(),
            "frozen_reaction_assembly_sha256": hashlib.sha256(
                (REFERENCE / "engine/reactions.py").read_bytes()
            ).hexdigest(),
            "coordinate_api": "rdkit.Chem.rdDepictor.Compute2DCoords",
            "default_options": {"useRingTemplates": False},
            "template_options": {"useRingTemplates": True},
            "document_scale": worker.SCALE,
            "document_y_reversed": True,
            "document_coordinate_type": "f32",
        },
        "eligibility": {
            "atom_count": molecule.GetNumAtoms(),
            "bond_count": molecule.GetNumBonds(),
            "symmetric_rings": rings,
            "shared_ring_atom_counts": shared,
            "clearance": CLEARANCE,
            "default_collision_pairs": default_pairs,
            "template_collision_pairs": template_pairs,
            "default_proper_crossings": default_crossings,
            "template_proper_crossings": template_crossings,
        },
        "display_only_identity": identity,
        "original_response": original_response,
        "template_positions": template_positions,
        "reaction_agent": reaction_agent_contract(),
        "collision_free_control": {
            "name": "control/clear-bridged-adamantane",
            "input": CONTROL,
            "default_collision_pairs": [],
            "original_response": response(CONTROL),
        },
    }


if __name__ == "__main__":
    print(json.dumps(generate(), ensure_ascii=False, indent=2))
