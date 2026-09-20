"""Constrained, component-preserving 2D cleanup over a complete drawing."""

import copy
import math

from rdkit import Chem
from rdkit.Chem import rdDepictor
from rdkit.Geometry import Point2D


def components(doc):
    ids = [a["id"] for a in doc["atoms"]]
    if len(set(ids)) != len(ids):
        raise ValueError("Duplicate atom IDs")
    neighbors = {i: set() for i in ids}
    for bond in doc["bonds"]:
        a, b = bond["a"], bond["b"]
        if a not in neighbors or b not in neighbors or a == b:
            raise ValueError("Invalid bond endpoints")
        neighbors[a].add(b)
        neighbors[b].add(a)
    visited = set()
    for atom_id in ids:
        if atom_id in visited:
            continue
        found, pending = set(), [atom_id]
        visited.add(atom_id)
        while pending:
            current = pending.pop()
            found.add(current)
            for other in neighbors[current] - visited:
                visited.add(other)
                pending.append(other)
        yield found


def fragments(doc):
    members = list(components(doc))
    owners = {atom_id: i for i, part in enumerate(members) for atom_id in part}
    parts = [
        dict(
            version=doc["version"],
            atoms=[],
            bonds=[],
            abbreviations=[],
            atom_labels=doc.get("atom_labels", {}),
        )
        for _ in members
    ]
    for atom in doc["atoms"]:
        parts[owners[atom["id"]]]["atoms"].append(atom)
    for bond in doc["bonds"]:
        parts[owners[bond["a"]]]["bonds"].append(bond)
    for group in doc.get("abbreviations", []):
        owner = owners.get(group["anchor"])
        if owner is None or any(owners.get(i) != owner for i in group["members"]):
            raise ValueError("Invalid abbreviation in cleanup drawing")
        parts[owner]["abbreviations"].append(group)
    return zip(members, parts)


def orient(mol, old, fixed, keep_orientation):
    """Rigid rotation only: never reflect or change the chemical drawing scale."""
    conf = mol.GetConformer()
    new = [conf.GetAtomPosition(i) for i in range(mol.GetNumAtoms())]
    if len(fixed) >= 2:
        return
    if fixed:
        pivot = next(iter(fixed))
        before, after = old[pivot], new[pivot]
        bx, by, ax, ay = before.x, before.y, after.x, after.y
    else:
        n = len(new)
        bx, by = sum(p.x for p in old) / n, sum(p.y for p in old) / n
        ax, ay = sum(p.x for p in new) / n, sum(p.y for p in new) / n
    angle = 0.0
    if keep_orientation:
        dot = sum((p.x - ax) * (q.x - bx) + (p.y - ay) * (q.y - by) for p, q in zip(new, old))
        cross = sum((p.x - ax) * (q.y - by) - (p.y - ay) * (q.x - bx) for p, q in zip(new, old))
        if abs(dot) + abs(cross) > 1e-12:
            angle = math.atan2(cross, dot)
    c, s = math.cos(angle), math.sin(angle)
    for i, p in enumerate(new):
        x, y = p.x - ax, p.y - ay
        conf.SetAtomPosition(i, (bx + c * x - s * y, by + s * x + c * y, 0.0))


def clean(doc, options, selection, read, write, analyze, scale, bond_length):
    options = options or {}
    scope = options.get("scope", "drawing")
    keep_orientation = options.get("keep_orientation", True)
    if scope not in ("selected_atoms", "selected_molecules", "drawing") or not isinstance(
        keep_orientation, bool
    ):
        raise ValueError("Invalid cleanup options")
    atom_ids = {a["id"] for a in doc["atoms"]}
    selected = set(selection or [])
    if not selected <= atom_ids:
        raise ValueError("Cleanup selection contains unavailable atoms")
    if scope != "drawing" and not selected:
        raise ValueError("Select atoms to clean up")
    # A collapsed abbreviation is a single selection unit, even for API callers.
    for group in doc.get("abbreviations", []):
        if selected.intersection(group["members"]):
            selected.update(group["members"])
    result = copy.deepcopy(doc)
    updated_atoms, updated_bonds = {}, {}
    count = 0
    crossed = 0
    for members, base in fragments(doc):
        if scope != "drawing" and not members.intersection(selected):
            continue
        moving = members.intersection(selected) if scope == "selected_atoms" else members
        mol = read(base)
        identity = Chem.MolToSmiles(mol, isomericSmiles=True)
        old = [mol.GetConformer().GetAtomPosition(i) for i in range(mol.GetNumAtoms())]
        fixed = {
            a.GetIdx(): Point2D(old[a.GetIdx()].x, old[a.GetIdx()].y)
            for a in mol.GetAtoms()
            if int(a.GetProp("moruno_id")) not in moving
        }
        rdDepictor.Compute2DCoords(
            mol,
            canonOrient=False,
            coordMap=fixed,
            bondLength=bond_length / scale,
            forceRDKit=True,
            useRingTemplates=True,
        )
        orient(mol, old, fixed, keep_orientation)
        for i, p in fixed.items():
            q = mol.GetConformer().GetAtomPosition(i)
            if math.hypot(q.x - p.x, q.y - p.y) > 1e-6:
                raise ValueError(
                    "Cleanup could not preserve the fixed atoms; choose Selected molecules"
                )
            mol.GetConformer().SetAtomPosition(i, (p.x, p.y, 0.0))
        generated = write(mol, base, rewedge=True)
        # Preserve fixed atoms and remote bonds exactly, including their saved
        # presentation and stereo metadata. Only the selected geometry is adopted.
        by_id = {a["id"]: a for a in generated["atoms"]}
        by_edge = {frozenset((b["a"], b["b"])): b for b in generated["bonds"]}
        merged = copy.deepcopy(base)
        merged["atoms"] = [by_id[a["id"]] if a["id"] in moving else a for a in base["atoms"]]
        merged["bonds"] = [
            by_edge[frozenset((b["a"], b["b"]))] if {b["a"], b["b"]}.intersection(moving) else b
            for b in base["bonds"]
        ]
        checked = read(merged)
        # An originally collinear/unspecified alkene must not acquire E/Z merely
        # because cleanup gives it a conventional zigzag. Cross only those bonds
        # whose new geometry would otherwise introduce a configuration.
        by_edge = {frozenset((b["a"], b["b"])): b for b in merged["bonds"]}
        for old_bond in mol.GetBonds():
            new_bond = checked.GetBondWithIdx(old_bond.GetIdx())
            if old_bond.GetStereo() in (
                Chem.BondStereo.STEREONONE,
                Chem.BondStereo.STEREOANY,
            ) and new_bond.GetStereo() not in (
                Chem.BondStereo.STEREONONE,
                Chem.BondStereo.STEREOANY,
            ):
                a = int(old_bond.GetBeginAtom().GetProp("moruno_id"))
                b = int(old_bond.GetEndAtom().GetProp("moruno_id"))
                if not {a, b}.intersection(moving):
                    continue
                by_edge[frozenset((a, b))].update(display="wavy", stereo=None, stereo_atoms=[])
                crossed += 1
        if Chem.MolToSmiles(read(merged), isomericSmiles=True) != identity:
            raise ValueError(
                "Cleanup could not preserve stereochemistry; try including the whole molecule"
            )
        # Check visible wedges and E/Z independently from stored absolute labels.
        # Only require stereo that was already conveyed by the original drawing.
        drawn_before, drawn_after = copy.deepcopy(base), copy.deepcopy(merged)
        for drawing in (drawn_before, drawn_after):
            for a in drawing["atoms"]:
                a["stereo"] = None
            for b in drawing["bonds"]:
                b["stereo"] = None
                b["stereo_atoms"] = []
        if Chem.MolToSmiles(read(drawn_before), isomericSmiles=True) != Chem.MolToSmiles(
            read(drawn_after), isomericSmiles=True
        ):
            raise ValueError(
                "Cleanup could not preserve visible stereochemistry; include the whole molecule"
            )
        updated_atoms.update((a["id"], a) for a in merged["atoms"] if a["id"] in moving)
        updated_bonds.update(
            (frozenset((b["a"], b["b"])), b)
            for b in merged["bonds"]
            if {b["a"], b["b"]}.intersection(moving)
        )
        count += 1
    if not count:
        raise ValueError("Draw or select a molecule first")
    result["atoms"] = [updated_atoms.get(a["id"], a) for a in result["atoms"]]
    result["bonds"] = [updated_bonds.get(frozenset((b["a"], b["b"])), b) for b in result["bonds"]]
    result["version"] = 15
    warnings = (
        [f"{crossed} double bond(s) use a crossed depiction to keep unspecified stereochemistry."]
        if crossed
        else []
    )
    try:
        analysis = analyze(read(result))
    except (ValueError, RuntimeError) as error:
        if scope == "drawing":
            raise
        analysis = None
        warnings.append(
            "Selected geometry cleaned. Another part of the drawing needs checking: " + str(error)
        )
    return dict(document=result, analysis=analysis, warnings=warnings)
