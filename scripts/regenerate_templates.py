"""Regenerate frozen templates with ReShiki's chemistry worker.

Run: uv run --locked python scripts/regenerate_templates.py
The catalog stores PubChem facts, optional drawing tautomers, and source-SMILES
atom locants. Existing drawings with the same molecular graph are retained,
including atom IDs and coordinates, except for explicit orientation changes;
drawings without one stand their six-membered rings upright.
No network access or reference-editor assets are required.
"""

import json
import math
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "reference"))
from rdkit import Chem  # noqa: E402
from rdkit.Chem import rdMolDescriptors, rdqueries  # noqa: E402

from engine.worker import from_document, handle  # noqa: E402


def identity(entry):
    return entry.get("id", f"builtin:{entry['group']}/{entry['name']}")


def atom_mapping(mol, document, locants, previous_locants):
    """Map source order to drawing IDs, including aromatic H and symmetry."""
    target = from_document(document)
    query = Chem.MolFromSmarts(Chem.MolToSmarts(mol))
    for atom in mol.GetAtoms():
        query.GetAtomWithIdx(atom.GetIdx()).ExpandQuery(
            rdqueries.HCountEqualsQueryAtom(atom.GetTotalNumHs())
        )
    matches = target.GetSubstructMatches(query, uniquify=False, maxMatches=0)
    assert matches, "Drawing differs from the catalog graph"
    atoms = document["atoms"]
    if previous_locants:
        for match in matches:
            if all(
                previous_locants.get(str(atoms[index]["id"])) == locant
                for index, locant in zip(match, locants)
                if locant is not None
            ):
                return match
    if "1" not in locants:
        return matches[0]
    # Resolve equivalent graph automorphisms using the drawing's upper-right
    # start and clockwise progression, without moving its frozen coordinates.
    center = {
        axis: sum(atom["position"][axis] for atom in atoms) / len(atoms) for axis in ("x", "y")
    }

    def preference(match):
        first = atoms[match[locants.index("1")]]["position"]
        clockwise = True
        if "2" in locants:
            second = atoms[match[locants.index("2")]]["position"]
            clockwise = (first["x"] - center["x"]) * (second["y"] - center["y"]) > (
                first["y"] - center["y"]
            ) * (second["x"] - center["x"])
        return (not clockwise, round(first["y"], 8), -round(first["x"], 8))

    return min(matches, key=preference)


def orient(document, mapping, orientation):
    """Orient zero-based source-SMILES atom centroids left-to-right."""
    atoms = document["atoms"]

    def centroid(indices):
        return tuple(
            sum(atoms[mapping[index]]["position"][axis] for index in indices) / len(indices)
            for axis in ("x", "y")
        )

    left, right = (centroid(indices) for indices in orientation["axis"])
    angle = math.atan2(right[1] - left[1], right[0] - left[0])
    # Skip roundoff at an already horizontal axis so repeated runs retain the
    # exact frozen bytes instead of accumulating floating-point transforms.
    if not math.isclose(angle, 0.0, abs_tol=1e-12):
        cosine, sine = math.cos(angle), math.sin(angle)
        for atom in atoms:
            p = atom["position"]
            p["x"], p["y"] = p["x"] * cosine + p["y"] * sine, -p["x"] * sine + p["y"] * cosine
    left_y = centroid(orientation["axis"][0])[1]
    if (centroid(orientation.get("above", orientation.get("below")))[1] > left_y) == (
        "above" in orientation
    ):
        for atom in atoms:
            atom["position"]["y"] *= -1


def upright(document):
    """Stand six-membered rings upright, as new depictions and Clean up do.

    Mirrors layout_refinement::upright_rings: each component turns at most 30
    degrees about its centroid until its six-membered ring sides are vertical.
    A flat ring ties; the tie goes to the turn leaving more ring-fusion bonds
    vertical. Cages and components whose six-membered rings disagree keep
    their turn.
    """
    atoms = document["atoms"]
    mol = from_document(document)
    rings = mol.GetRingInfo().AtomRings()
    step = math.pi / 3

    def sides(ring):
        return [tuple(sorted((a, b))) for a, b in zip(ring, ring[1:] + ring[:1])]

    # Molfile-style y-up angles, like the Rust refinement.
    def angle(side):
        a, b = (atoms[i]["position"] for i in side)
        return math.atan2(a["y"] - b["y"], b["x"] - a["x"])

    seen, fused = set(), set()
    for ring in rings:
        for side in sides(ring):
            (fused if side in seen else seen).add(side)
    for members in Chem.GetMolFrags(mol):
        members = set(members)
        own = [ring for ring in rings if ring[0] in members]
        # Rings sharing three atoms make a cage.
        if any(len(set(a) & set(b)) >= 3 for i, a in enumerate(own) for b in own[i + 1 :]):
            continue
        six = [angle(side) for ring in own if len(ring) == 6 for side in sides(ring)]
        c = sum(math.cos(6 * t) for t in six)
        s = sum(math.sin(6 * t) for t in six)
        if not six or math.hypot(c, s) < 0.5 * len(six):
            continue
        turn = ((math.pi - math.atan2(s, c)) / 6 + step / 2) % step - step / 2
        if abs(abs(turn) - step / 2) < 1e-6:
            fusions = [angle(side) for side in fused if side[0] in members]

            def vertical(turn):
                return sum(abs(math.cos(t + turn)) < 0.05 for t in fusions)

            turn = -step / 2 if vertical(-step / 2) > vertical(step / 2) else step / 2
        if abs(turn) < 1e-9:
            continue
        cx = sum(atoms[i]["position"]["x"] for i in members) / len(members)
        cy = sum(atoms[i]["position"]["y"] for i in members) / len(members)
        cosine, sine = math.cos(turn), math.sin(turn)
        for i in members:
            p = atoms[i]["position"]
            x, y = p["x"] - cx, cy - p["y"]
            p["x"], p["y"] = cx + cosine * x - sine * y, cy - (sine * x + cosine * y)


path = ROOT / "assets/templates.json"
catalog = json.loads((ROOT / "assets/template-catalog.json").read_text(encoding="utf-8"))
previous = {identity(entry): entry for entry in json.loads(path.read_text(encoding="utf-8"))}
library = []
for entry in catalog:
    mol = (
        Chem.MolFromSequence(entry["sequence"], flavor=0)
        if entry.get("sequence")
        else Chem.MolFromSmiles(entry["smiles"])
    )
    assert mol is not None, entry["name"]
    assert rdMolDescriptors.CalcMolFormula(mol) == entry["formula"], entry["name"]
    assert Chem.MolToInchiKey(mol) == entry["inchikey"], entry["name"]
    if "tautomer_smiles" in entry:
        mol = Chem.MolFromSmiles(entry["tautomer_smiles"])
        assert mol is not None, entry["name"]
        assert rdMolDescriptors.CalcMolFormula(mol) == entry["formula"], entry["name"]
        assert Chem.MolToInchiKey(mol) == entry["inchikey"], entry["name"]
    if entry.get("geometry") == "model:porphine":
        # The catalog's graph identity is separate from the planar depiction.
        # The model owns porphine's regular-pentagon geometry.
        continue
    template = {k: entry[k] for k in ("name", "group")}
    if "id" in entry:
        template["id"] = entry["id"]
    smiles = Chem.MolToSmiles(mol)
    old = previous.get(identity(entry))
    reuse = old and Chem.MolToSmiles(Chem.MolFromSmiles(old["smiles"])) == smiles
    template["smiles"] = old["smiles"] if reuse else smiles
    template["keywords"] = entry.get("keywords", [])
    template["note"] = entry.get("note", "")
    if entry["name"] == "Decalin":
        template["note"] = "Ring-junction stereochemistry is unspecified."
    template["document"] = (
        old["document"]
        if reuse
        else handle({"protocol": 1, "operation": "import", "format": "smiles", "text": smiles})[
            "document"
        ]
    )
    if not reuse:
        # A neutral Kekule NH carries an ordinary replaceable valence H in
        # templates. Fixed SMILES [nH] metadata would block N attachment.
        for atom in template["document"]["atoms"]:
            if atom["element"] == "N" and atom["charge"] == 0 and atom["explicit_h"] == 1:
                atom["explicit_h"] = 0
                atom["no_implicit"] = False
        assert Chem.MolToSmiles(from_document(template["document"])) == smiles, entry["name"]
    if "locants" in entry:
        locants = entry["locants"]
        assert len(locants) == mol.GetNumAtoms(), entry["name"]
        mapping = atom_mapping(
            mol, template["document"], locants, old.get("locants", {}) if reuse else {}
        )
        template["locants"] = {
            str(template["document"]["atoms"][index]["id"]): locant
            for index, locant in zip(mapping, locants)
            if locant is not None
        }
        if "orientation" in entry:
            orient(template["document"], mapping, entry["orientation"])
    if "orientation" not in entry:
        upright(template["document"])
    library.append(template)
path.write_text(json.dumps(library, indent=2) + "\n")
print(f"Regenerated {len(library)} templates")
