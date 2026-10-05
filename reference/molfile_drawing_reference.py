"""Native MOL-to-drawing passes, independent of the Rust parser and transport."""

import json
import sys
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDLogger, rdBase
from rdkit.Chem import rdCIPLabeler

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from engine.worker import to_document  # Retained original drawing converter.

if TYPE_CHECKING or __package__:
    from .molfile_import_reference import cases, read
    from .perception_reference import snapshot
else:
    from molfile_import_reference import cases, read
    from perception_reference import snapshot


def emit(name, text):
    expected, labels, document, failure = None, None, None, None
    try:
        original = read(text)
        work = Chem.Mol(original)
        Chem.Kekulize(work, clearAromaticFlags=True)
        Chem.WedgeMolBonds(work, work.GetConformer())
        for obj in list(work.GetAtoms()) + list(work.GetBonds()):
            if obj.HasProp("_CIPCode"):
                obj.ClearProp("_CIPCode")
        expected = snapshot(work, "symmetric")
        rdCIPLabeler.AssignCIPLabels(work, maxRecursiveIterations=1_250_000)
        labels = dict(
            rdkit_version=rdBase.rdkitVersion,
            atoms=[
                a.GetProp("_CIPCode") if a.HasProp("_CIPCode") else None for a in work.GetAtoms()
            ],
            bonds=[
                dict(
                    code=b.GetProp("_CIPCode") if b.HasProp("_CIPCode") else None,
                    stereo=int(b.GetStereo()),
                    stereo_atoms=list(b.GetStereoAtoms()),
                )
                for b in work.GetBonds()
            ],
        )
        document = to_document(original)
    except (ValueError, RuntimeError, KeyError, OverflowError) as error:
        expected, failure = None, str(error)
    print(
        json.dumps(
            dict(
                name=name,
                text=text,
                expected=expected,
                labels=labels,
                document=document,
                failure=failure,
            )
        )
    )


if __name__ == "__main__":
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    cases(emit)
