"""Native RXN output using the original reaction rules and independent molecule builder."""

import copy
import json
import sys
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDLogger, rdBase

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from engine import reactions

if TYPE_CHECKING or __package__:
    from .document_preparation_reference import cases, drawing, f32, molecule
else:
    from document_preparation_reference import cases, drawing, f32, molecule


def scheme(part):
    doc = copy.deepcopy(part)
    doc["version"] = 15
    last = max((a["id"] for a in doc["atoms"]), default=0)
    ids = [a["id"] for a in doc["atoms"]]
    doc["atoms"].append(dict(id=last + 1, element="C", position=dict(x=420, y=0)))
    doc["arrows"] = [dict(id=last + 2, start=dict(x=200, y=0), end=dict(x=350, y=0))]
    doc["reactions"] = [
        dict(
            arrow=last + 2,
            reactants=[dict(atoms=ids, coefficient=1)],
            products=[dict(atoms=[last + 1], coefficient=1)],
            agents=[],
        )
    ]
    return doc


def emit(name, doc, selected=None):
    doc = copy.deepcopy(doc)
    # Requests promote stored f32 positions to JSON floats before Python reads
    # them. Preserve that boundary, including the sign of zero under Y reversal.
    for atom in doc["atoms"]:
        atom["position"] = {axis: f32(atom["position"][axis]) for axis in ("x", "y")}
    expected, failure = None, None
    try:
        expected = reactions.export(doc, selected, "rxn", molecule)
    except (ValueError, RuntimeError, KeyError, OverflowError) as error:
        failure = str(error)
    print(
        json.dumps(
            dict(name=name, document=doc, selected=selected, expected=expected, failure=failure)
        )
    )


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    cases(lambda name, part: emit(name, scheme(part)))
    for smiles in (
        "c1ccccc1",
        "c1ccncc1",
        "c1cc[nH]c1",
        "[nH]1nncc1",
        "c1ccc2ccccc2c1",
        "N->[Cu+2]",
        "[13CH3:1][OH:2]",
        "C[C@H](O)Cl",
        "C[C@@H](O)Cl",
        "F/C=C/Cl",
        "F/C=C\\Cl",
        "CC=C(C)C",
        "[CH2]C",
        "C[O]",
        "[Na+].[Cl-]",
    ):
        base = scheme(drawing(Chem.MolFromSmiles(smiles)))
        for coefficient in (1, 2, 99):
            for role in ("reactants", "products", "agents"):
                doc = copy.deepcopy(base)
                source = doc["reactions"][0]
                if role == "products":
                    source["reactants"], source["products"] = (
                        source["products"],
                        source["reactants"],
                    )
                elif role == "agents":
                    source["agents"] = source["reactants"]
                    next_id = source["arrow"] + 1
                    doc["atoms"].append(dict(id=next_id, element="O", position=dict(x=500, y=0)))
                    source["reactants"] = [dict(atoms=[next_id], coefficient=1)]
                source[role][0]["coefficient"] = coefficient
                emit(f"role/coefficient/{smiles}/{role}/{coefficient}", doc)
        for permute in ("atoms", "bonds", "endpoints", "members"):
            doc = copy.deepcopy(base)
            if permute in ("atoms", "bonds"):
                doc[permute].reverse()
            elif permute == "members":
                doc["reactions"][0]["reactants"][0]["atoms"].reverse()
            else:
                for bond in doc["bonds"]:
                    bond["a"], bond["b"] = bond["b"], bond["a"]
                    bond["stereo_atoms"].reverse()
            emit(f"ordering/{smiles}/{permute}", doc)
    base = scheme(drawing(Chem.MolFromSmiles("CCO")))
    arrow = base["reactions"][0]["arrow"]
    for selected in (None, [], [arrow], [arrow, arrow], [1], [arrow, 1]):
        emit(f"arrow selection/{selected}", base, selected)
    second = copy.deepcopy(base)
    second["arrows"].append(dict(id=arrow + 1, start=dict(x=0, y=200), end=dict(x=300, y=200)))
    second["reactions"].append(dict(second["reactions"][0], arrow=arrow + 1))
    for selected in (None, [], [arrow], [arrow + 1], [arrow, arrow + 1]):
        emit(f"multiple reactions/{selected}", second, selected)
    for kind in (
        "no_reactions",
        "no_reactants",
        "no_products",
        "missing_arrow",
        "duplicate_arrow",
        "empty_participant",
        "duplicate_member",
        "unknown_member",
        "incomplete_molecule",
        "overlap",
        "coefficient_zero",
        "coefficient_100",
        "duplicate_maps",
        "repeated_mapped",
        "annotation_missing",
        "annotation_duplicate",
        "bad_element",
        "bad_valence",
    ):
        doc = copy.deepcopy(base)
        source = doc["reactions"][0]
        participant = source["reactants"][0]
        if kind == "no_reactions":
            doc["reactions"] = []
        elif kind == "no_reactants":
            source["reactants"] = []
        elif kind == "no_products":
            source["products"] = []
        elif kind == "missing_arrow":
            source["arrow"] = 999
        elif kind == "duplicate_arrow":
            doc["reactions"].append(copy.deepcopy(source))
        elif kind == "empty_participant":
            participant["atoms"] = []
        elif kind == "duplicate_member":
            participant["atoms"].append(participant["atoms"][0])
        elif kind == "unknown_member":
            participant["atoms"].append(999)
        elif kind == "incomplete_molecule":
            participant["atoms"].pop()
        elif kind == "overlap":
            source["agents"] = [copy.deepcopy(participant)]
        elif kind.startswith("coefficient_"):
            participant["coefficient"] = 0 if kind.endswith("zero") else 100
        elif kind == "duplicate_maps":
            doc["atoms"][0]["map_num"] = doc["atoms"][1]["map_num"] = 1
        elif kind == "repeated_mapped":
            doc["atoms"][0]["map_num"] = 1
            participant["coefficient"] = 2
        elif kind.startswith("annotation_"):
            source["annotations"] = [99, 99] if kind.endswith("duplicate") else [99]
        elif kind == "bad_element":
            doc["atoms"][0]["element"] = "NotAnElement"
        else:
            doc["atoms"][0]["explicit_h"] = 9
        emit(f"invalid/{kind}", doc)
    for size, products in ((100, 2), (101, 1), (101, 2)):
        # Both sides count toward the atom limit, including repeated molecules.
        part = dict(
            version=15,
            atoms=[dict(id=i + 1, element="C", position=dict(x=i, y=0)) for i in range(size)],
            bonds=[],
        )
        doc = scheme(part)
        doc["reactions"][0]["reactants"][0]["coefficient"] = 99
        doc["reactions"][0]["products"][0]["coefficient"] = products
        emit(f"expanded atom boundary/{size * 99 + products}", doc)


if __name__ == "__main__":
    main()
