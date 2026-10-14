"""Source-only pinned RDKit evidence for #300's twelve carbon End-paint cases.

The Rust expectation adapter sends only source XML, never a current observation.
Unsupported shapes return no evidence and retain all existing exact comparisons.
"""

import copy
import json
import sys
import xml.etree.ElementTree as ET

from cdxml_preparation_reference import PREFIX_HASH, PREPARE, capture
from cdxml_scene_reference import CAPTURE
from perception_reference import snapshot
from rdkit import Chem, RDLogger, rdBase

from engine import worker

RDKIT_VERSION = "2026.03.6"
IMPORT_SHA256 = "8e29ae0e691ee01c3cdf3e9121ff72a8249b89accfab1ef1604b288be875196d"


def bounded_source(text):
    """A subset of valence-evidence.py's predicate, excluding richer drawings."""
    root = ET.fromstring(text)
    if root.tag != "CDXML" or root.attrib != {"BondLength": "14.4"} or len(root) != 1:
        return None
    page = root[0]
    if page.tag != "page" or page.attrib or len(page) != 1:
        return None
    fragment = page[0]
    if fragment.tag != "fragment" or set(fragment.attrib) != {"id"}:
        return None
    atoms, bonds = fragment.findall("n"), fragment.findall("b")
    if len(atoms) != 2 or len(bonds) != 1 or list(fragment) != [*atoms, *bonds]:
        return None
    if any(set(atom.attrib) != {"id", "p"} or len(atom) for atom in atoms):
        return None
    bond = bonds[0]
    if (
        bond.get("Order") != "dative"
        or bond.get("Display") not in {"WedgeEnd", "WedgedHashEnd", "HollowWedgeEnd"}
        or bond.get("Display2") not in {None, "Solid", "Dash", "Bold"}
        or bond.get("B") == bond.get("E")
        or {bond.get("B"), bond.get("E")} != {atom.get("id") for atom in atoms}
        or set(bond.attrib) - {"id", "B", "E", "Order", "Display", "Display2"}
        or len(bond)
        or any(node.text and node.text.strip() for node in root.iter())
        or any(node.tail and node.tail.strip() for node in root.iter())
    ):
        return None
    return root, atoms, bond


def evidence(text):
    assert rdBase.rdkitVersion == RDKIT_VERSION
    assert PREFIX_HASH == IMPORT_SHA256
    source = bounded_source(text)
    if source is None:
        return None
    root, atoms, bond = source
    begin, end = [
        next(i for i, atom in enumerate(atoms) if atom.get("id") == bond.get(key))
        for key in ("B", "E")
    ]
    old_scope = PREPARE(text)
    original_state = capture(old_scope)["molecule"]["state"]
    old_scene = CAPTURE(text, local_pictures=True)
    assert snapshot(old_scene["mol"], "symmetric") == original_state
    original_atoms = worker.handle(
        dict(protocol=1, operation="import", format="cdxml", text=text, local_pictures=True)
    )["document"]["atoms"]

    # Dative paint has no chemical stereo. Native RDKit reads unchanged source
    # atom semantics and donor B -> acceptor E with plain paint. This reproduces
    # the saved independent evidence, not the current Rust importer.
    bond.set("Display", "Solid")
    parts = Chem.MolsFromCDXML(
        ET.tostring(root, encoding="unicode"), sanitize=False, removeHs=False
    )
    assert len(parts) == 1
    mol = parts[0]
    assert mol.GetNumAtoms() == 2 and mol.GetNumBonds() == 1
    native_bond = mol.GetBondWithIdx(0)
    assert (native_bond.GetBeginAtomIdx(), native_bond.GetEndAtomIdx()) == (begin, end)
    assert original_state["graph"]["bonds"][0]["a"] == end
    assert original_state["graph"]["bonds"][0]["b"] == begin
    for conf in mol.GetConformers():
        for index in range(mol.GetNumAtoms()):
            point = conf.GetAtomPosition(index)
            conf.SetAtomPosition(
                index, tuple(v * old_scope["factor"] for v in (point.x, point.y, point.z))
            )
    Chem.SanitizeMol(mol)
    Chem.AssignChiralTypesFromBondDirs(mol, replaceExistingTags=False)
    Chem.DetectBondStereochemistry(mol, confId=0)
    Chem.AssignStereochemistry(mol, cleanIt=False, force=True)
    intended_state = snapshot(mol, "symmetric")
    intended_atoms = worker.to_document(mol, copy.deepcopy(old_scene["base"]))["atoms"]

    # Check the exact independent old/new evidence before exporting it. Only
    # the endpoint orientation, direction, and four cached values may differ.
    expected_state = copy.deepcopy(original_state)
    expected_state["graph"]["bonds"][0].update(a=begin, b=end)
    expected_state["directions"] = ["none"]
    for index, old_valence, new_valence, old_h, new_h in ((begin, 1, 0, 3, 4), (end, 0, 1, 4, 3)):
        assert original_state["valences"][index] == dict(
            explicit_valence=old_valence, implicit_hydrogens=old_h
        )
        assert intended_state["valences"][index] == dict(
            explicit_valence=new_valence, implicit_hydrogens=new_h
        )
        expected_state["valences"][index] = intended_state["valences"][index]
        assert original_atoms[index]["label_h"] == old_h
        assert intended_atoms[index]["label_h"] == new_h
    assert expected_state == intended_state
    expected_atoms = copy.deepcopy(original_atoms)
    for index in (begin, end):
        expected_atoms[index]["label_h"] = intended_atoms[index]["label_h"]
    assert expected_atoms == intended_atoms
    return dict(
        rdkit_version=rdBase.rdkitVersion,
        import_sha256=PREFIX_HASH,
        endpoints=[begin, end],
        original_state=original_state,
        intended_state=intended_state,
        original_atoms=original_atoms,
        intended_atoms=intended_atoms,
        labels=bond.get("Display2") is None,
    )


if __name__ == "__main__":
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(evidence(sys.stdin.read()), allow_nan=False))
