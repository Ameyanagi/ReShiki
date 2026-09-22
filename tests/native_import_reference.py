"""Original complete drawing/reaction imports, without prepared Rust transport."""

import json
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from rdkit import RDLogger, rdBase

from engine.worker import handle


def emit(name, text, restriction=None, *, format="cdxml"):
    if not isinstance(text, str):
        text = ET.tostring(text, encoding="unicode")
    try:
        expected = handle(
            dict(protocol=1, operation="import", format=format, text=text, local_pictures=True)
        )
        json.dumps(expected, allow_nan=False)
        failure = None
    except (ValueError, RuntimeError, KeyError, IndexError, OverflowError, ET.ParseError) as error:
        expected, failure = None, str(error)
    print(
        json.dumps(
            dict(
                name=name,
                text=text,
                format=format,
                expected=expected,
                failure=failure,
                restriction=restriction,
            )
        )
    )


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    # Reuse only independently generated input cases. Every expected response
    # is produced by the unchanged original worker's complete import operation.
    import cdxml_scene_reference as scene

    scene.emit = emit
    scene.main()
    for text in (
        "CO>O>N |(1,2,3;4,5,6;7,8,9;10,11,12)|",
        "[13CH3:2][OH:1]>>[13CH2:2]=[O:1] |(0,0,;1.5,0,;0,0,;1.5,0,)|",
        "F[C@](Cl)(Br)I>>F[C@@](Cl)(Br)I |(0,0,;1,1,;2,0,;0,2,;2,2,;4,0,;5,1,;6,0,;4,2,;6,2,)|",
        "F/C=C/F>O>F/C=C\\F |(0,0,;1,1,;2,1,;3,2,;0,3,;4,0,;5,1,;6,1,;7,2,)|",
        "C>>O |(0,0,;1,1,)|",
        "C>>O |()|",
        "C>>O",
        "invalid",
        "[CH5]>>O |(0,0,;1,1,)|",
        "C>>O |(nan,inf,-inf)|",
    ):
        emit("reaction-coordinates/" + text, text, format="rsmi")


if __name__ == "__main__":
    main()
