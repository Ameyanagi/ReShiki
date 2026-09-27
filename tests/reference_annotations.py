"""Independent chemistry for the annotated multi-attachment fixture new in 0.9.

The legacy Python importer rejects this entire drawing. RDKit's direct CDXML
reader supports it, providing chemistry without using the Rust implementation.
"""

import xml.etree.ElementTree as ET

from rdkit import Chem


def molecular_extension(name, text):
    if name != "preparation/molecular-fixture/atom-to-fragment.cdxml":
        return None
    parts = Chem.MolsFromCDXML(text)
    molecule = parts[0]
    for part in parts[1:]:
        molecule = Chem.CombineMols(molecule, part)
    wrappers = [
        node
        for node in ET.fromstring(text).iter("n")
        if node.get("NodeType") in ("Fragment", "Nickname")
    ]
    assert len(wrappers) == 1
    wrapper = wrappers[0]
    inner = wrapper.find("fragment")
    label = wrapper.find("t")
    assert inner is not None and label is not None
    members = [n for n in inner.findall("n") if n.get("NodeType") != "ExternalConnectionPoint"]
    # This source defines a single central carbon joined to both external
    # attachment points by double bonds. That environment identifies the anchor.
    assert len(members) == 1
    anchor = members[0]
    return dict(
        smiles=Chem.MolToSmiles(molecule),
        atoms=molecule.GetNumAtoms(),
        bonds=molecule.GetNumBonds(),
        abbreviations=[
            dict(
                label="".join(label.itertext()),
                members=len(members),
                element=Chem.GetPeriodicTable().GetElementSymbol(int(anchor.get("Element", "6"))),
                bond_orders=sorted(
                    int(b.get("Order", "1"))
                    for b in inner.findall("b")
                    if anchor.get("id") in (b.get("B"), b.get("E"))
                ),
            )
        ],
    )


def legacy_preparation_input(name, text):
    if name != "molecular-fixture/geometry-tetrahedral-4.cdxml":
        return text
    root = ET.fromstring(text)
    for parent in root.iter():
        for child in list(parent):
            if child.tag == "annotation":
                assert child.get("Keyword") == "Name"
                assert set(child.attrib) == {"Keyword", "Content"} and len(child) == 0
                parent.remove(child)
    return ET.tostring(root, encoding="unicode")
