"""Reaction membership and chemical exchange; canvas positions never determine roles."""

import copy
from typing import Any

from rdkit import Chem
from rdkit.Chem import rdChemReactions, rdDepictor

ROLES = ("reactants", "products", "agents")


def validate(doc):
    atoms = {a["id"] for a in doc["atoms"]}
    arrows = {a["id"] for a in doc.get("arrows", [])}
    notes = {a["id"] for a in doc.get("annotations", [])}
    seen = set()
    for reaction in doc.get("reactions", []):
        if set(reaction) - {"arrow", *ROLES, "annotations"}:
            raise ValueError("Unsupported reaction data")
        arrow = reaction["arrow"]
        if arrow not in arrows or arrow in seen:
            raise ValueError("Invalid or duplicate reaction arrow")
        seen.add(arrow)
        annotations = reaction.get("annotations", [])
        if len(annotations) != len(set(annotations)) or not set(annotations) <= notes:
            raise ValueError("Invalid reaction annotation")
        assigned = set()
        for role in ROLES:
            for participant in reaction.get(role, []):
                if set(participant) - {"atoms", "coefficient"}:
                    raise ValueError("Unsupported reaction participant data")
                ids = set(participant["atoms"])
                coefficient = participant.get("coefficient", 1)
                if (
                    not ids
                    or len(ids) != len(participant["atoms"])
                    or not ids <= atoms
                    or ids & assigned
                    or type(coefficient) is not int
                    or not 1 <= coefficient <= 99
                ):
                    raise ValueError("Invalid, duplicate, or empty reaction participant")
                assigned.update(ids)
                if any(
                    b["order"] != 0 and (b["a"] in ids) != (b["b"] in ids) for b in doc["bonds"]
                ):
                    raise ValueError("Assign complete molecules to reaction roles")


def export(doc, selected, format, from_document):
    validate(doc)
    reactions = doc.get("reactions", [])
    if selected:
        reactions = [r for r in reactions if r["arrow"] in selected]
    if len(reactions) != 1:
        raise ValueError("Choose one defined reaction in Reaction roles before exporting")
    source = reactions[0]
    if not source.get("reactants") or not source.get("products"):
        raise ValueError("Assign at least one reactant and one product before exporting")
    reaction = rdChemReactions.ChemicalReaction()
    count = 0
    for role, add in zip(
        ROLES,
        (reaction.AddReactantTemplate, reaction.AddProductTemplate, reaction.AddAgentTemplate),
    ):
        maps = set()
        for participant in source.get(role, []):
            ids = set(participant["atoms"])
            fragment = {
                "version": 15,
                "atoms": [a for a in doc["atoms"] if a["id"] in ids],
                "bonds": [b for b in doc["bonds"] if b["a"] in ids and b["b"] in ids],
            }
            # Do not silently discard interactions or unsupported bond orders.
            if any(
                b["order"] in (0, 6, 7) and (b["a"] in ids or b["b"] in ids) for b in doc["bonds"]
            ):
                raise ValueError(
                    "Reaction exchange cannot preserve these hydrogen, partial or quadruple bonds; save the native drawing"
                )
            molecule = from_document(fragment)
            coefficient = participant.get("coefficient", 1)
            for atom in molecule.GetAtoms():
                number = atom.GetAtomMapNum()
                if number:
                    if number in maps or coefficient > 1:
                        raise ValueError(
                            "Atom map numbers must be unique on each reaction side; repeated mapped molecules need separate mappings"
                        )
                    maps.add(number)
            count += molecule.GetNumAtoms() * coefficient
            if count > 10000:
                raise ValueError("Expanded reaction exceeds 10,000 atoms")
            for _ in range(coefficient):
                add(Chem.Mol(molecule))
    if format == "rxn":
        return rdChemReactions.ReactionToV3KRxnBlock(reaction, separateAgents=True)
    return rdChemReactions.ReactionToSmiles(reaction)


def import_reaction(text, format, to_document, check_supported):
    if len(text) > 16 * 1024 * 1024:
        raise ValueError("Reaction exceeds 16 MB")
    reaction = (
        rdChemReactions.ReactionFromRxnBlock(text, sanitize=True, removeHs=False)
        if format == "rxn"
        else rdChemReactions.ReactionFromSmarts(text.strip(), useSmiles=True)
    )
    if (
        reaction is None
        or not reaction.GetNumReactantTemplates()
        or not reaction.GetNumProductTemplates()
    ):
        raise ValueError("A reaction needs at least one reactant and one product")
    participants = (reaction.GetReactants(), reaction.GetProducts(), reaction.GetAgents())
    if sum(m.GetNumAtoms() for parts in participants for m in parts) > 10000:
        raise ValueError("Reaction exceeds 10,000 atoms")
    doc = to_document(Chem.Mol())
    semantic: dict[str, Any] = dict(arrow=0, reactants=[], products=[], agents=[], annotations=[])
    next_id = 1

    def caption(text, x, y):
        nonlocal next_id
        doc["annotations"].append(dict(id=next_id, text=text, position=dict(x=x, y=y)))
        semantic["annotations"].append(next_id)
        next_id += 1

    def row(parts, role, x, y):
        nonlocal next_id
        for index, original in enumerate(parts):
            molecule = Chem.RWMol(original)
            # RXN parsing wraps even ordinary elements in atomic-number queries.
            # Strip only this exact wrapper; never flatten real query chemistry.
            for atom in molecule.GetAtoms():
                if atom.HasQuery():
                    if atom.DescribeQuery().strip() != f"AtomAtomicNum {atom.GetAtomicNum()} = val":
                        raise ValueError("Query reaction atoms are not supported yet")
                    molecule.ReplaceAtom(atom.GetIdx(), Chem.Atom(atom), preserveProps=True)
            Chem.SanitizeMol(molecule)
            check_supported(molecule)
            if not molecule.GetNumAtoms():
                raise ValueError("Empty reaction participant")
            if not molecule.GetNumConformers():
                rdDepictor.Compute2DCoords(molecule)
            for atom in molecule.GetAtoms():
                atom.SetProp("reshiki_id", str(next_id))
                next_id += 1
            part = to_document(molecule)
            # Reserve label room for heteroatoms and implicit hydrogens.
            lo = min(a["position"]["x"] - 36 for a in part["atoms"])
            hi = max(a["position"]["x"] + 36 for a in part["atoms"])
            cy = (
                min(a["position"]["y"] for a in part["atoms"])
                + max(a["position"]["y"] for a in part["atoms"])
            ) / 2
            if index:
                caption("+", x + 12, y - 15)
                x += 70
            for atom in part["atoms"]:
                if atom["element"] == "O" and atom["label_h"] == 2 and len(part["atoms"]) == 1:
                    atom["display"] = {"hydrogen_position": "left"}
                atom["position"]["x"] += x - lo
                atom["position"]["y"] += y - cy
            doc["atoms"].extend(part["atoms"])
            doc["bonds"].extend(part["bonds"])
            semantic[role].append(dict(atoms=[a["id"] for a in part["atoms"]], coefficient=1))
            x += hi - lo
        return x

    x = row(participants[0], "reactants", 0, 0) + 42
    # Agents occupy their own row with enough vertical clearance for all molecules.
    height = max((abs(a["position"]["y"]) for a in doc["atoms"]), default=0)
    agents_start = len(doc["atoms"])
    agent_width = row(participants[2], "agents", 0, 0) if participants[2] else 0
    agent_atoms = doc["atoms"][agents_start:]
    agent_height = max((abs(a["position"]["y"]) for a in agent_atoms), default=0)
    width = max(140, agent_width + 42)
    for atom in agent_atoms:
        atom["position"]["x"] += x + (width - agent_width) / 2
        atom["position"]["y"] -= height + agent_height + 100
    # Reposition agent separators along with their molecules.
    agent_notes = max(0, len(participants[2]) - 1)
    if agent_notes:
        for note in doc["annotations"][-agent_notes:]:
            note["position"]["x"] += x + (width - agent_width) / 2
            note["position"]["y"] -= height + agent_height + 100
    semantic["arrow"] = next_id
    doc["arrows"].append(
        dict(id=next_id, start=dict(x=x, y=0), end=dict(x=x + width, y=0), kind="forward")
    )
    next_id += 1
    row(participants[1], "products", x + width + 42, 0)
    doc["reactions"] = [semantic]
    validate(doc)
    return copy.deepcopy(doc)
