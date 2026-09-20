"""Chemical abbreviations retain their complete molecular graph and stable IDs."""

import copy
import math

from rdkit import Chem
from rdkit.Chem import rdAbbreviations, rdDepictor

# The leading dummy is the sole outside attachment; it is not part of the label.
PRESETS = {
    "OMe": ("*OC", "MeO"),
    "OEt": ("*OCC", "EtO"),
    "Me": ("*C", ""),
    "Et": ("*CC", ""),
    "nPr": ("*CCC", ""),
    "iPr": ("*C(C)C", ""),
    "nBu": ("*CCCC", ""),
    "tBu": ("*C(C)(C)C", ""),
    "Ph": ("*c1ccccc1", ""),
    "Bn": ("*Cc1ccccc1", ""),
    "Boc": ("*C(=O)OC(C)(C)C", ""),
    "Cbz": ("*C(=O)OCc1ccccc1", ""),
    "Fmoc": ("*C(=O)OCC1c2ccccc2-c2ccccc21", ""),
    "Ac": ("*C(C)=O", ""),
    "OAc": ("*OC(C)=O", "AcO"),
    "Bz": ("*C(=O)c1ccccc1", ""),
    "OBz": ("*OC(=O)c1ccccc1", "BzO"),
    "Ts": ("*S(=O)(=O)c1ccc(C)cc1", ""),
    "OTs": ("*OS(=O)(=O)c1ccc(C)cc1", "TsO"),
    "Ms": ("*S(C)(=O)=O", ""),
    "OMs": ("*OS(C)(=O)=O", "MsO"),
    "TMS": ("*[Si](C)(C)C", ""),
    "TBS": ("*[Si](C)(C)C(C)(C)C", ""),
    "CF3": ("*C(F)(F)F", "F3C"),
    "CN": ("*C#N", "NC"),
    "NO2": ("*[N+](=O)[O-]", "O2N"),
    "CO2H": ("*C(=O)O", "HO2C"),
    "CO2Me": ("*C(=O)OC", "MeO2C"),
    "CO2Et": ("*C(=O)OCC", "EtO2C"),
}


def validate(doc):
    atoms = {a["id"]: a for a in doc["atoms"]}
    if doc.get("abbreviations") and doc.get("version", 1) < 10:
        raise ValueError("Abbreviations require document version 10")
    used = set()
    for g in doc.get("abbreviations", []):
        label = g.get("label", "")
        if not isinstance(label, str) or not label.strip():
            raise ValueError("An abbreviation needs a label")
        for text in (label, g.get("reverse_label", "")):
            if (
                not isinstance(text, str)
                or len(text) > 32
                or any(ord(c) < 32 or 127 <= ord(c) < 160 for c in text)
            ):
                raise ValueError("Invalid abbreviation label")
        members = set(g["members"])
        anchor = g["anchor"]
        if (
            not members
            or len(members) != len(g["members"])
            or anchor not in members
            or not members <= atoms.keys()
            or members & used
        ):
            raise ValueError("Invalid or overlapping abbreviation atoms")
        used.update(members)
        boundary = [b for b in doc["bonds"] if (b["a"] in members) != (b["b"] in members)]
        if len(boundary) > 1 or any(anchor not in (b["a"], b["b"]) for b in boundary):
            raise ValueError("Abbreviations need one attachment atom and at most one outside bond")
        reached = {anchor}
        while True:
            old = len(reached)
            for b in doc["bonds"]:
                if {b["a"], b["b"]} <= members and {b["a"], b["b"]} & reached:
                    reached.update((b["a"], b["b"]))
            if old == len(reached):
                break
        if reached != members:
            raise ValueError("An abbreviation must be connected")


def find(doc, mol, selection, label=None):
    available = [(name, *data) for name, data in PRESETS.items() if not label or name == label]
    if not available:
        raise ValueError("Unknown abbreviation")
    # Prefer complete protecting groups over smaller groups within them.
    available.sort(key=lambda x: Chem.MolFromSmiles(x[1]).GetNumAtoms(), reverse=True)
    definitions = rdAbbreviations.ParseAbbreviations(
        "\n".join(f"{name} {smiles}" for name, smiles, _ in available)
    )
    result = copy.deepcopy(doc)
    groups = result.setdefault("abbreviations", [])
    used = {i for g in groups for i in g["members"]}
    ids = {a.GetIdx(): int(a.GetProp("reshiki_id")) for a in mol.GetAtoms()}
    atoms = {a["id"]: a for a in doc["atoms"]}
    labeled = rdAbbreviations.LabelMolAbbreviations(mol, definitions, 1.0)
    for g in Chem.GetMolSubstanceGroups(labeled):
        members = [ids[i] for i in g.GetAtoms()]
        points = list(g.GetAttachPoints())
        if not members or len(points) != 1 or used.intersection(members):
            continue
        if selection and not set(members) <= set(selection):
            continue
        if any(
            atoms[i].get("isotope")
            or atoms[i].get("map_num")
            or atoms[i].get("stereo")
            or atoms[i].get("marks")
            or atoms[i].get("display", {}).get("number")
            for i in members
        ):
            continue
        name = g.GetProp("LABEL")
        groups.append(
            dict(
                label=name,
                reverse_label=PRESETS[name][1],
                anchor=ids[points[0].aIdx],
                members=members,
            )
        )
        used.update(members)
    result["version"] = 15
    validate(result)
    return result


def replace(doc, selection, label, to_document):
    if label not in PRESETS:
        raise ValueError("Choose a defined abbreviation")
    remove = set(selection)
    old_group = next((g for g in doc.get("abbreviations", []) if set(g["members"]) == remove), None)
    target = old_group["anchor"] if old_group else (selection[0] if len(selection) == 1 else None)
    atoms = {a["id"]: a for a in doc["atoms"]}
    if target is None or target not in atoms:
        raise ValueError("Select one terminal atom or one abbreviation to replace")
    boundary = [b for b in doc["bonds"] if (b["a"] in remove) != (b["b"] in remove)]
    if len(boundary) > 1 or any(
        b["order"] != 1 or target not in (b["a"], b["b"]) for b in boundary
    ):
        raise ValueError("Select an endpoint with a single bond, or an isolated atom")
    template = Chem.MolFromSmiles(PRESETS[label][0])
    if template is None:
        raise ValueError("Invalid abbreviation definition")
    rdDepictor.Compute2DCoords(template)
    dummy = next(a for a in template.GetAtoms() if a.GetAtomicNum() == 0)
    root = dummy.GetNeighbors()[0].GetIdx()
    conf = template.GetConformer()
    root_point = conf.GetAtomPosition(root)
    dummy_point = conf.GetAtomPosition(dummy.GetIdx())
    original_angle = math.atan2(-(dummy_point.y - root_point.y), dummy_point.x - root_point.x)
    origin = atoms[target]["position"]
    outside = None
    if boundary:
        b = boundary[0]
        outside = atoms[b["b"] if b["a"] == target else b["a"]]["position"]
    desired_angle = (
        math.atan2(outside["y"] - origin["y"], outside["x"] - origin["x"]) if outside else math.pi
    )
    angle = desired_angle - original_angle
    c, s = math.cos(angle), math.sin(angle)
    scale = (
        math.hypot(outside["x"] - origin["x"], outside["y"] - origin["y"]) / 1.5
        if outside
        else 28.0
    )
    if not math.isfinite(scale) or scale < 0.1:
        raise ValueError("The attachment bond has invalid geometry")
    all_ids = [
        a["id"]
        for key in ("atoms", "annotations", "arrows", "graphics", "groups")
        for a in doc.get(key, [])
    ]
    next_id = max(all_ids, default=0) + 1
    for a in template.GetAtoms():
        if a.GetIdx() == dummy.GetIdx():
            continue
        identifier = target if a.GetIdx() == root else next_id
        if a.GetIdx() != root:
            next_id += 1
        if identifier >= 2**64 - 1:
            raise ValueError("Object ID limit exceeded")
        a.SetProp("reshiki_id", str(identifier))
        p = conf.GetAtomPosition(a.GetIdx())
        x = (p.x - root_point.x) * scale
        y = -(p.y - root_point.y) * scale
        conf.SetAtomPosition(
            a.GetIdx(), ((origin["x"] + x * c - y * s) / 28, -(origin["y"] + x * s + y * c) / 28, 0)
        )
    rw = Chem.RWMol(template)
    rw.RemoveAtom(dummy.GetIdx())
    fragment = rw.GetMol()
    for a in fragment.GetAtoms():
        if a.HasProp("reshiki_id") and int(a.GetProp("reshiki_id")) == target:
            a.SetNoImplicit(False)
            a.SetNumRadicalElectrons(0)
    Chem.SanitizeMol(fragment)
    part = to_document(fragment)
    for a in part["atoms"]:
        if a["id"] == target and atoms[target].get("text_style"):
            a["text_style"] = copy.deepcopy(atoms[target]["text_style"])
    result = copy.deepcopy(doc)
    result["atoms"] = [a for a in result["atoms"] if a["id"] not in remove] + part["atoms"]
    result["bonds"] = [b for b in result["bonds"] if not ({b["a"], b["b"]} <= remove)] + part[
        "bonds"
    ]
    members = [a["id"] for a in part["atoms"]]
    for group in result.get("groups", []):
        if target in group["members"]:
            group["members"] = [i for i in group["members"] if i not in remove] + members
    result["abbreviations"] = [
        g for g in result.get("abbreviations", []) if not remove.intersection(g["members"])
    ]
    for reaction in result.get("reactions", []):
        for role in ("reactants", "products", "agents"):
            for participant in reaction.get(role, []):
                if target in participant["atoms"]:
                    participant["atoms"] = [
                        i for i in participant["atoms"] if i not in remove
                    ] + members
    result["abbreviations"].append(
        dict(label=label, reverse_label=PRESETS[label][1], anchor=target, members=members)
    )
    result["version"] = 15
    validate(result)
    return result
