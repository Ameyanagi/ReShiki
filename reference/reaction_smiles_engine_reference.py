"""Complete original reaction SMILES imports, without the prepared transport."""

import json
import sys
from itertools import product
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDLogger, rdBase
from rdkit.Chem import rdChemReactions

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from engine.worker import handle

if TYPE_CHECKING or __package__:
    from .reaction_smiles_reference import cases as bare_cases
    from .reaction_smiles_reference import cx_cases
else:
    from reaction_smiles_reference import cases as bare_cases
    from reaction_smiles_reference import cx_cases


def cases():
    for index, (name, text) in enumerate(bare_cases()):
        if name.startswith("limit/"):
            continue  # Parser atom and component limits have their own suite.
        if name.startswith(("group/", "framing/", "molecule/template/")) or index % 31 == 0:
            yield name, text
    for index, (name, text) in enumerate(cx_cases()):
        if name.startswith("cx/interactions/") or index % 17 == 0:
            yield name, text
    for atom, key, value in product(
        range(8),
        (
            "dummyLabel",
            "atomLabel",
            "molAttchpt",
            "_CIPRank",
            "_UnknownStereo",
            "_ringStereoAtoms",
            "_ringStereochemCand",
            "_ChiralityPossible",
            "_CIPCode",
            "_fromAttachPoint",
        ),
        ("0", "1", "-1", "bad", "_AP1", "_AP2", "Pol_p", "R1"),
    ):
        text = f"*[C@](F)(Cl)Br>O>F/C=C/F |atomProp:{atom}.{key}.{value}|"
        yield f"property/{atom}/{key}/{value}", text
    for graph, atom, key, value in product(
        ("C[C@H]1CC[C@@H](C)CC1", "c1ccccc1", "CC"),
        range(8),
        ("_ringStereoAtoms", "_ringStereochemCand", "_CIPRank", "_CIPCode"),
        ("0", "1", "bad"),
    ):
        yield f"cached/{graph}/{atom}/{key}/{value}", f"{graph}>>O |atomProp:{atom}.{key}.{value}|"
    for coords in (
        "",
        "1,2,3",
        "nan,inf,-inf",
        "1e308,1e308,1e308",
        "0,0,;1,0,;0,1,;0,-1,;-1,-1,",
        "1,2,3;4,5,6;7,8,9",
    ):
        for section in ("", ",wU:1.0", ",wD:1.1", ",$ _AP1$", ",$_AP1$"):
            yield f"coordinates/{coords}/{section}", f"*[C@](F)(Cl)Br>O>N |({coords}){section}|"


def participants(text):
    reaction = rdChemReactions.ReactionFromSmarts(text.strip(), useSmiles=True)
    # The worker draws reactants, then agents, then products.
    mols = [*reaction.GetReactants(), *reaction.GetAgents(), *reaction.GetProducts()]
    for mol in mols:
        Chem.SanitizeMol(mol)
        Chem.AssignStereochemistry(mol, cleanIt=True, force=True)
    return mols


def unspecified_ez(text):
    """#909/#923: drawing-order indices of the double bonds whose configuration
    the input leaves open (and every cumulated double bond), with RDKit's
    identifiers when the new layout does not define them. The original worker
    reads E/Z from that layout."""
    # Only inputs that state no E/Z at all: stated E/Z that reaction import
    # drops is a regression (#1043), never an intended difference.
    if any(c in text.strip().split(maxsplit=1)[0] for c in "/\\"):
        return None
    try:
        mols = participants(text)
    except (ValueError, RuntimeError):
        return None
    bonds, offset, combined = [], 0, Chem.Mol()
    for mol in mols:
        cumulated = {
            a.GetIdx()
            for a in mol.GetAtoms()
            if sum(b.GetBondType() == Chem.BondType.DOUBLE for b in a.GetBonds()) == 2
        }
        for bond in mol.GetBonds():
            if bond.GetBondType() == Chem.BondType.DOUBLE and (
                bond.GetStereo() == Chem.BondStereo.STEREONONE
                or {bond.GetBeginAtomIdx(), bond.GetEndAtomIdx()} & cumulated
            ):
                bond.SetStereo(Chem.BondStereo.STEREONONE)
                bonds.append(offset + bond.GetIdx())
        offset += mol.GetNumBonds()
        # The original worker analyzes the finished drawing, whose atoms
        # store only a nonzero map number: an explicit map 0 is not kept.
        for atom in mol.GetAtoms():
            if atom.HasProp("molAtomMapNumber") and atom.GetAtomMapNum() == 0:
                atom.ClearProp("molAtomMapNumber")
        mol.RemoveAllConformers()
        combined = Chem.CombineMols(combined, mol)
    return dict(
        bonds=bonds,
        inchi=Chem.MolToInchi(combined),
        inchikey=Chem.MolToInchiKey(combined),
        smiles=Chem.MolToSmiles(combined),
    )


if __name__ == "__main__":
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    for name, text in cases():
        try:
            expected = handle(dict(protocol=1, operation="import", format="rsmi", text=text))
            failure = None
        except (ValueError, RuntimeError, KeyError, AttributeError, OverflowError) as error:
            expected, failure = None, str(error)
        print(
            json.dumps(
                dict(
                    name=name,
                    text=text,
                    expected=expected,
                    failure=failure,
                    unspecified_ez=unspecified_ez(text) if expected else None,
                )
            )
        )
