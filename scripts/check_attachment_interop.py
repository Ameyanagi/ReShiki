"""Reproduce attachment behavior using the locked RDKit, without calling an AI.

Synthetic CDXML is authored from the published NodeType/Attachments format;
it contains no extracted ChemDraw implementation or proprietary templates.
"""

import argparse
import json
import math
from pathlib import Path
from xml.etree import ElementTree as ET

from rdkit import Chem, rdBase
from rdkit.Chem import rdChemDraw, rdDepictor


def attachment_xml(kind):
    root = ET.Element("CDXML", BondLength="40")
    fragment = ET.SubElement(ET.SubElement(root, "page", id="1"), "fragment", id="2")
    for index in range(6):
        angle = index * math.pi / 3
        ET.SubElement(
            fragment,
            "n",
            id=str(index + 10),
            p=f"{200 + 40 * math.cos(angle):.4f} {200 + 40 * math.sin(angle):.4f}",
        )
        ET.SubElement(
            fragment,
            "b",
            id=str(index + 20),
            B=str(index + 10),
            E=str(10 + (index + 1) % 6),
            Order="2" if index % 2 == 0 else "1",
        )
    ET.SubElement(
        fragment, "n", id="30", p="200 200", NodeType=kind, Attachments="10 11 12 13 14 15"
    )
    ET.SubElement(fragment, "n", id="31", p="200 280", Element="26")
    ET.SubElement(fragment, "b", id="32", B="30", E="31", Order="1")
    return ET.tostring(root, encoding="unicode")


def endpoints(mol):
    return [
        {
            "begin": bond.GetBeginAtomIdx(),
            "end": bond.GetEndAtomIdx(),
            "type": str(bond.GetBondType()),
            "endpoints": bond.GetProp("_MolFileBondEndPts"),
            "attach": bond.GetProp("_MolFileBondAttach")
            if bond.HasProp("_MolFileBondAttach")
            else None,
        }
        for bond in mol.GetBonds()
        if bond.HasProp("_MolFileBondEndPts")
    ]


def require_mol(mol):
    if mol is None:
        raise ValueError("RDKit rejected a reference molecule")
    return mol


def haptic_case(smiles, metal_number):
    mol = require_mol(Chem.MolFromSmiles(smiles))
    rdDepictor.Compute2DCoords(mol)
    editable = Chem.RWMol(mol)
    metal = next(a.GetIdx() for a in editable.GetAtoms() if a.GetAtomicNum() == metal_number)
    for atom in editable.GetAtoms():
        if atom.GetAtomicNum() == 6:
            editable.AddBond(atom.GetIdx(), metal, Chem.BondType.DATIVE)
    explicit = editable.GetMol()
    haptic = Chem.DativeBondsToHaptic(explicit)
    restored = Chem.HapticBondsToDative(haptic)
    block = Chem.MolToMolBlock(haptic, forceV3000=True)
    reloaded = require_mol(Chem.MolFromMolBlock(block, sanitize=True, removeHs=False))
    return {
        "attachments": endpoints(haptic),
        "dummy_count": sum(a.GetAtomicNum() == 0 for a in haptic.GetAtoms()),
        "graph_roundtrip": Chem.MolToSmiles(explicit) == Chem.MolToSmiles(restored),
        "v3000_roundtrip": Chem.MolToSmiles(explicit)
        == Chem.MolToSmiles(Chem.HapticBondsToDative(reloaded)),
        "v3000_bonds": [line for line in block.splitlines() if "ENDPTS" in line],
    }


def inspect():
    result = {"rdkit_version": rdBase.rdkitVersion, "cdxml": {}, "haptic": {}}
    for kind in ("MultiAttachment", "VariableAttachment"):
        source = attachment_xml(kind)
        result["cdxml"][kind] = {}
        for name, parse in (
            ("MolsFromCDXML", Chem.MolsFromCDXML),
            ("MolsFromChemDrawBlock", rdChemDraw.MolsFromChemDrawBlock),
        ):
            molecules = parse(source)
            result["cdxml"][kind][name] = [
                {
                    "smiles": Chem.MolToSmiles(mol),
                    "atoms": mol.GetNumAtoms(),
                    "dummy_count": sum(a.GetAtomicNum() == 0 for a in mol.GetAtoms()),
                    "attachments": endpoints(mol),
                }
                for mol in molecules
            ]
    for name, smiles, metal in (
        ("eta3_allyl", "[CH2-]C=C.[Pd+2]", 46),
        ("eta6_arene", "c1ccccc1.[Cr]", 24),
        ("ferrocene", "[cH-]1cccc1.[Fe+2].[cH-]1cccc1", 26),
    ):
        result["haptic"][name] = haptic_case(smiles, metal)
    variable = require_mol(Chem.MolFromSmiles("CO*.c1ccccc1 |m:2:3.4.5.6.7.8|"))
    block = Chem.MolToMolBlock(variable, forceV3000=True)
    reloaded = require_mol(Chem.MolFromMolBlock(block, sanitize=True, removeHs=False))
    result["variable"] = {
        "cx_input": endpoints(variable),
        "v3000_reloaded": endpoints(reloaded),
        "cx_output": Chem.MolToCXSmiles(variable),
        "v3000_bonds": [line for line in block.splitlines() if "ENDPTS" in line],
    }
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    result = inspect()
    output = json.dumps(result, indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(output)
    print(output, end="")
    if not all(
        case["graph_roundtrip"] and case["v3000_roundtrip"] for case in result["haptic"].values()
    ):
        raise ValueError("A haptic representation did not survive the round trip")


if __name__ == "__main__":
    main()
