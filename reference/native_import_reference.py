"""Original complete drawing/reaction imports, without prepared Rust transport."""

import json
import math
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import support.frozen_radicals  # noqa: F401  (before check_supported is imported)
from rdkit import Chem, RDLogger, rdBase
from reference_annotations import molecular_extension
from smiles_engine_reference import sssr, unspecified_ez

from engine.worker import handle

# ReShiki keeps RDKit's legacy ring symmetrization; child processes inherit it.
Chem.SetUseLegacyRingFinding(True)


def original_import(format, text):
    return handle(
        dict(protocol=1, operation="import", format=format, text=text, local_pictures=True)
    )


def current_symbol_appearance(name, text, expected):
    fixture = name in (
        "fixture/symbols-chemdraw.cdxml",
        "preparation/fixture/symbols-chemdraw.cdxml",
    )
    if not fixture and name != "graphic/Symbol/{'SymbolType': 'Plus'}":
        return
    # These captures use free signs, presented upright by ChemDraw.
    root = ET.fromstring(text)
    symbol_type = "CirclePlus" if fixture else "Plus"
    symbol_kind = "circle_plus" if fixture else "plus"
    (source_symbol,) = root.findall(f".//graphic[@SymbolType='{symbol_type}']")
    assert source_symbol.get("GraphicType") == "Symbol"
    assert source_symbol.get("ObjectID") is None
    assert source_symbol.get("ChemicallySignificant") != "yes"
    ax, ay, bx, by = map(float, source_symbol.attrib["BoundingBox"].split())
    # Spell the upright endpoint at CDXML's normalization precision, then
    # let the original importer apply the document scale and translation.
    source_symbol.set(
        "BoundingBox", f"{ax:.4f} {ay:.4f} {ax - math.hypot(ax - bx, ay - by):.4f} {ay:.4f}"
    )
    upright = original_import("cdxml", ET.tostring(root, encoding="unicode"))
    (before,) = (
        g for g in expected["document"]["graphics"] if g["kind"] == {"symbol": symbol_kind}
    )
    (after,) = (g for g in upright["document"]["graphics"] if g["kind"] == {"symbol": symbol_kind})
    assert before["axis_x"]["y"] != 0
    assert after["axis_x"]["y"] == 0 and after["axis_y"]["x"] == 0
    before.update(axis_x=after["axis_x"], axis_y=after["axis_y"])
    assert upright == expected, "Upright projection changed unrelated response fields"
    if fixture:
        (orbital_source,) = root.findall(".//graphic[@OrbitalType='p']")
        assert orbital_source.get("GraphicType") == "Orbital"
        (orbital,) = (g for g in expected["document"]["graphics"] if g["kind"] == {"orbital": "p"})
        assert orbital["phase"] == "open"
        orbital["phase"] = "gradient"


def emit(name, text, restriction=None, *, format="cdxml"):
    if not isinstance(text, str):
        text = ET.tostring(text, encoding="unicode")
    try:
        expected = original_import(format, text)
        if format == "cdxml":
            current_symbol_appearance(name, text, expected)
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
                molecular_extension=molecular_extension(name, text),
                unspecified_ez=unspecified_ez(text) if expected and format == "smiles" else None,
                sssr=sssr(expected),
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
