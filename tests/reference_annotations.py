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
    return dict(
        smiles=Chem.MolToSmiles(molecule),
        atoms=molecule.GetNumAtoms(),
        bonds=molecule.GetNumBonds(),
        abbreviations=sum(
            node.get("NodeType") in ("Fragment", "Nickname")
            for node in ET.fromstring(text).iter("n")
        ),
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
