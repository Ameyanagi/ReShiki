"""Retained ring-display workflow with direct native preparation and identifiers."""

import copy
import json
import random
import sys
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDConfig, RDLogger, rdBase

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from engine.aromatic import toggle
from engine.worker import to_document

if TYPE_CHECKING or __package__:
    from .document_preparation_reference import drawing, molecule, prepare
else:
    from document_preparation_reference import drawing, molecule, prepare


def emit(name, doc, selection):
    expected, before, after, identity, failure = None, None, None, None, None
    try:
        expected, checked = toggle(doc, selection, molecule, to_document)
        before, after = prepare(doc), prepare(expected)
        identity = dict(
            rdkit_version=rdBase.rdkitVersion,
            before=Chem.MolToSmiles(molecule(doc)),
            after=Chem.MolToSmiles(checked),
        )
    except (ValueError, RuntimeError, KeyError, OverflowError) as error:
        expected, failure = None, str(error)
    print(
        json.dumps(
            dict(
                name=name,
                document=doc,
                selection=selection,
                expected=expected,
                before=before,
                after=after,
                identity=identity,
                failure=failure,
            )
        )
    )
    return expected


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    root = Path(__file__).resolve().parents[1]
    texts = [t["smiles"] for t in json.loads((root / "assets/templates.json").read_text())]
    texts += [
        "c1cc[nH]c1",
        "c1ncc[nH]1",
        "c1ccc2[nH]ccc2c1",
        "[13cH:90]1ccccc1.[2H]O[3H]",
        "C[C@H](c1ccccc1)O",
        "c1ccccc1/C=C/Cl",
        "c1ccc2ccccc2c1.CCO",
        "C1CCCCC1",
        "CCO",
    ]
    texts += [
        line.split()[0]
        for line in (Path(RDConfig.RDDataDir) / "NCI/first_5K.smi").read_text().splitlines()
    ]
    rng = random.Random(27183)
    for index, text in enumerate(texts):
        mol = Chem.MolFromSmiles(text)
        if mol is None:
            continue
        if index % 4 == 0:
            order = list(range(mol.GetNumAtoms()))
            rng.shuffle(order)
            mol = Chem.RenumberAtoms(mol, order)
        doc = drawing(mol)
        ids = [a["id"] for a in doc["atoms"]]
        if index % 5 == 0:
            # Presentation and stale labels must survive, apart from label H.
            for atom in doc["atoms"]:
                atom["position"]["x"] *= -1
                atom["label_h"] = 99
            for bond in doc["bonds"]:
                bond["color"] = [25, 60, 190]
                bond["double_position"] = "left"
            doc["annotations"] = [dict(id=987654, position=dict(x=0, y=100), text="試料")]
        circle = emit(f"all/{index}", doc, ids)
        if circle:
            emit(f"back/{index}", circle, ids)
        if index % 11 == 0:
            emit(f"empty/{index}", doc, [])
            emit(f"incomplete/{index}", doc, ids[:1] + [987654])
        if index % 3 == 0:
            for j, ring in enumerate(list(mol.GetRingInfo().AtomRings())[:2]):
                selection = [ids[i] for i in ring]
                circle = emit(f"ring/{index}/{j}", doc, selection)
                if circle:
                    emit(f"ring back/{index}/{j}", circle, selection)
                if index % 15 == 0 and len(selection) > 2:
                    abbreviated = copy.deepcopy(doc)
                    abbreviated["abbreviations"] = [
                        dict(
                            label="Ar",
                            anchor=selection[0],
                            members=selection,
                        )
                    ]
                    emit(f"abbreviation/{index}/{j}", abbreviated, selection)


if __name__ == "__main__":
    main()
