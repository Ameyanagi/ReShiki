"""NMR prediction: the rows ``reshiki --cli predict-nmr`` and the MCP ``predict_nmr`` tool return."""

from __future__ import annotations

import json
import warnings
from pathlib import Path

import pytest
import reshiki

ETHYL_ACETATE = "CCOC(C)=O"


def test_ethyl_acetate_has_every_site_with_typed_fields() -> None:
    molecule = reshiki.Molecule.from_smiles(ETHYL_ACETATE)
    protons = molecule.predict_nmr("1H")
    assert isinstance(protons, reshiki.NmrPrediction)
    assert (protons.nucleus, protons.solvent) == ("1H", "CDCl3")
    assert "nmrshiftdb2" in protons.credit
    # SMILES atoms 1, 2 and 5 carry the H.
    assert [row.atom for row in protons] == [1, 2, 5]
    assert [row.hydrogen_count for row in protons] == [3, 2, 3]
    for row in protons:
        assert isinstance(row, reshiki.NmrShift)
        assert row.nucleus == "1H"
        assert isinstance(row.ppm, float) and isinstance(row.sd, float)
        assert isinstance(row.min, float) and isinstance(row.max, float)
        assert isinstance(row.shell, int) and isinstance(row.references, int)
        assert row.broad is False
        assert row.hydrogens == ()
        assert row.note == "Unresolved attached-H group; not a stereospecific peak assignment"
    carbons = molecule.predict_nmr("13C")
    assert [row.atom for row in carbons] == [1, 2, 4, 5]
    assert all(row.hydrogen_count is None and row.ppm is not None for row in carbons)


def test_a_site_without_data_has_no_values_and_a_note() -> None:
    hydroxyl = reshiki.Molecule.from_smiles("CCO").predict_nmr("1H").rows[2]
    assert hydroxyl.atom == 3
    assert (hydroxyl.ppm, hydroxyl.shell, hydroxyl.references, hydroxyl.sd) == (None,) * 4
    assert hydroxyl.note is not None and "Exchangeable" in hydroxyl.note


def test_cleanup_keeps_atom_ids() -> None:
    molecule = reshiki.Molecule.from_smiles(ETHYL_ACETATE)
    assert molecule.clean().predict_nmr("13C") == molecule.predict_nmr("13C")


def test_errors_carry_the_operation_error_code() -> None:
    molecule = reshiki.Molecule.from_smiles("CCO")
    for nucleus in ("H1", "13c", "15N", ""):
        with pytest.raises(reshiki.ReShikiError) as invalid:
            molecule.predict_nmr(nucleus)  # type: ignore[arg-type]
        assert invalid.value.code == "invalid_arguments", nucleus
        assert str(invalid.value) == "nucleus must be 1H or 13C"
    for smiles, nucleus, message in (
        ("CCO.CC", "13C", "NMR prediction needs exactly one molecule; this drawing has 2"),
        ("O", "13C", "This molecule has no supported ¹³C sites"),
        ("ClC(Cl)(Cl)Cl", "1H", "This molecule has no supported ¹H sites"),
        ("[Fe]", "13C", None),
        ("[2H]C", "1H", None),
    ):
        with pytest.raises(reshiki.ReShikiError) as failed:
            reshiki.Drawing.from_smiles(smiles).predict_nmr(nucleus)
        assert failed.value.code == "failed", smiles
        if message is not None:
            assert str(failed.value) == message


def test_smiles_and_mol_atom_ids_follow_rdkit_numbering() -> None:
    chem = pytest.importorskip("rdkit.Chem")
    for smiles in (ETHYL_ACETATE, "[H]C(=O)OC", "OCc1ccccc1C(=O)N", "C[C@H](N)C(=O)O"):
        source = chem.MolFromSmiles(smiles)
        for molecule in (
            reshiki.Molecule.from_smiles(smiles),
            reshiki.Molecule.from_molfile(chem.MolToMolBlock(source)),
        ):
            for row in molecule.predict_nmr("1H"):
                atom = source.GetAtomWithIdx(row.atom - 1)
                assert atom.GetTotalNumHs() == row.hydrogen_count, (smiles, row)
            for row in molecule.predict_nmr("13C"):
                assert source.GetAtomWithIdx(row.atom - 1).GetSymbol() == "C", (smiles, row)


def test_mol_atom_ids_are_atom_block_positions() -> None:
    # V3000 record numbers 10, 20 and 30 are not kept: ethanol's carbons are atoms 1 and 2.
    molfile = """
  ReShiki

  0  0  0     0  0            999 V3000
M  V30 BEGIN CTAB
M  V30 COUNTS 3 2 0 0 0
M  V30 BEGIN ATOM
M  V30 10 C 0 0 0 0
M  V30 20 C 1.5 0 0 0
M  V30 30 O 2.25 1.3 0 0
M  V30 END ATOM
M  V30 BEGIN BOND
M  V30 1 1 10 20
M  V30 2 1 20 30
M  V30 END BOND
M  V30 END CTAB
M  END
"""
    rows = reshiki.Molecule.from_molfile(molfile).predict_nmr("13C").rows
    assert [row.atom for row in rows] == [1, 2]


def _numbers(drawing: reshiki.Drawing) -> dict[int, str]:
    """Each numbered atom's number, from the native document."""
    atoms = json.loads(drawing.rsk())["atoms"]
    return {
        int(atom["id"]): atom["display"]["number"]["text"]
        for atom in atoms
        if atom.get("display", {}).get("number")
    }


def test_the_issue_example_writes_labels_that_every_figure_keeps(tmp_path: Path) -> None:
    molecule = reshiki.Molecule.from_smiles(ETHYL_ACETATE).clean()
    shifts = molecule.predict_nmr("13C")
    labelled = molecule.with_labels(shifts, format="both")
    assert isinstance(labelled, reshiki.Molecule)
    # 13C labels get one decimal place by default.
    expected = {row.atom: f"#{row.atom} · {row.ppm:.1f} ppm" for row in shifts}
    assert expected[4] == "#4 · 170.7 ppm"
    assert _numbers(labelled) == expected
    assert _numbers(molecule) == {}
    path = labelled.save(tmp_path / "assigned.svg", style="JACS / ACS", svg_text="live")
    svg = path.read_text(encoding="utf-8")
    assert all(f">{text}</text>" in svg for text in expected.values())
    # The labels change every figure, and the native file keeps them.
    for format in ("svg", "png", "pdf"):
        assert labelled.render(format) != molecule.render(format), format
    assert _numbers(reshiki.parse(labelled.rsk())) == expected
    assert 'AtomNumber="#4 · 170.7 ppm"' in labelled.cdxml()


def test_the_default_is_the_bare_value_as_journals_print_it() -> None:
    molecule = reshiki.Molecule.from_smiles(ETHYL_ACETATE)
    carbons = molecule.predict_nmr("13C")
    protons = molecule.predict_nmr("1H")
    # One decimal place for 13C and two for 1H; no "#N" and no unit.
    assert _numbers(molecule.with_labels(carbons)) == {
        row.atom: f"{row.ppm:.1f}" for row in carbons if row.ppm is not None
    }
    assert _numbers(molecule.with_labels(carbons))[4] == "170.7"
    assert _numbers(molecule.with_labels(protons))[1] == "1.28"
    assert _numbers(molecule.with_labels(carbons, format="value", decimals=3))[4] == "170.700"
    assert _numbers(molecule.with_labels([(4, 170.66)], decimals=0)) == {4: "171"}


def test_rows_can_be_shifts_mappings_or_pairs() -> None:
    molecule = reshiki.Molecule.from_smiles(ETHYL_ACETATE)
    protons = molecule.predict_nmr("1H")
    # 1H rows and rows without a nucleus both get two decimal places.
    expected = {row.atom: f"{row.ppm:.2f} ppm" for row in protons}
    assert expected[1] == "1.28 ppm"
    mappings = [{"atom": row.atom, "ppm": row.ppm} for row in protons]
    pairs = [(row.atom, row.ppm) for row in protons]
    for rows in (protons, list(protons), mappings, pairs):
        assert _numbers(molecule.with_labels(rows, format="ppm")) == expected
    assert _numbers(molecule.with_labels(protons, format="numbers")) == {1: "#1", 2: "#2", 5: "#5"}
    # A row without a ppm is skipped; a second nucleus replaces only its own atoms.
    carbons = molecule.with_labels([(3, None), (4, 170.7)], format="numbers")
    assert _numbers(carbons.with_labels(protons, format="ppm")) == {**expected, 4: "#4"}


def test_a_bad_row_writes_nothing() -> None:
    molecule = reshiki.Molecule.from_smiles(ETHYL_ACETATE)
    for rows, message in (
        ([(1, 14.2), (99, 1.0)], "Atom 99 is not in the drawing"),
        ([(1, float("nan"))], "The ppm of atom 1 is not a finite number"),
        ([(1, float("inf"))], "The ppm of atom 1 is not a finite number"),
        ([(1, 1e20)], "The label of atom 1 is longer than 32 characters"),
    ):
        with pytest.raises(reshiki.ReShikiError) as error:
            molecule.with_labels(rows, format="both")
        assert error.value.code == "invalid_arguments"
        assert str(error.value) == message
    with pytest.raises(reshiki.ReShikiError) as error:
        molecule.with_labels([], format="atom")  # type: ignore[arg-type]
    assert str(error.value) == "format must be value, numbers, ppm or both"


def test_wider_labels_make_room_in_a_reaction_as_the_app_does() -> None:
    reaction = reshiki.Reaction.from_smiles("CC(=O)O.OCC>>CCOC(C)=O.O")

    def layout(drawing: reshiki.Drawing) -> list[object]:
        native = json.loads(drawing.rsk())
        return [a["position"] for a in native["atoms"]] + [
            a["position"] for a in native.get("annotations", [])
        ]

    # Acetic acid, the first reactant, is atoms 1-4, beside the tracked "+".
    labelled = reaction.with_labels([(atom, 100.0) for atom in range(1, 5)], format="both")
    assert len(_numbers(labelled)) == 4
    assert layout(labelled) != layout(reaction)
    # Rows that write nothing leave the drawing as it was.
    assert layout(reaction.with_labels([(1, None)])) == layout(reaction)


def test_decimals_default_per_nucleus_and_can_be_chosen() -> None:
    molecule = reshiki.Molecule.from_smiles(ETHYL_ACETATE)
    carbons = molecule.predict_nmr("13C")
    assert _numbers(molecule.with_labels(carbons, format="ppm"))[4] == "170.7 ppm"
    pairs = [(row.atom, row.ppm) for row in carbons]
    assert _numbers(molecule.with_labels(pairs, format="ppm"))[4] == "170.70 ppm"
    # A row without a nucleus makes the whole call use 2.
    mixed = [*carbons.rows[:-1], (carbons.rows[-1].atom, carbons.rows[-1].ppm)]
    assert _numbers(molecule.with_labels(mixed, format="ppm"))[4] == "170.70 ppm"
    for decimals, text in ((0, "171 ppm"), (3, "170.700 ppm"), (4, "170.7000 ppm")):
        labelled = molecule.with_labels(carbons, format="ppm", decimals=decimals)
        assert _numbers(labelled)[4] == text
    for decimals in (5, -1, 300):
        with pytest.raises(reshiki.ReShikiError) as error:
            molecule.with_labels(carbons, decimals=decimals)
        assert error.value.code == "invalid_arguments"
        assert str(error.value) == "decimals must be 0 to 4"


def test_atom_numbers_round_trip_through_rdkit_atom_notes() -> None:
    pytest.importorskip("rdkit.Chem")
    molecule = reshiki.Molecule.from_smiles(ETHYL_ACETATE)
    labelled = molecule.with_labels(molecule.predict_nmr("13C"), format="both")
    rdkit = labelled.to_rdkit()
    notes = {
        a.GetIdx() + 1: a.GetProp("atomNote") for a in rdkit.GetAtoms() if a.HasProp("atomNote")
    }
    assert notes == _numbers(labelled)
    with warnings.catch_warnings():
        warnings.simplefilter("error", reshiki.StructureWarning)
        assert _numbers(reshiki.Molecule.from_rdkit(rdkit)) == _numbers(labelled)


def test_rdkit_notes_land_on_the_atoms_import_keeps() -> None:
    chem = pytest.importorskip("rdkit.Chem")
    params = chem.SmilesParserParams()
    params.removeHs = False
    # [H]CO keeps its H in RDKit: indices H 0, C 1, O 2. Import folds the H,
    # so the drawing keeps C as atom 2 and O as atom 3.
    rdkit = chem.MolFromSmiles("[H]CO", params)
    rdkit.GetAtomWithIdx(1).SetProp("atomNote", "C note")
    rdkit.GetAtomWithIdx(2).SetProp("atomNote", "O note")
    with warnings.catch_warnings():
        warnings.simplefilter("error", reshiki.StructureWarning)
        drawing = reshiki.Molecule.from_rdkit(rdkit)
    elements = {a["id"]: a["element"] for a in json.loads(drawing.rsk())["atoms"]}
    assert {elements[atom]: text for atom, text in _numbers(drawing).items()} == {
        "C": "C note",
        "O": "O note",
    }
    rdkit.GetAtomWithIdx(0).SetProp("atomNote", "H note")
    with pytest.warns(reshiki.StructureWarning, match="1 of 3 RDKit atom notes were skipped"):
        drawing = reshiki.Molecule.from_rdkit(rdkit)
    assert sorted(_numbers(drawing).values()) == ["C note", "O note"]


def test_invalid_rdkit_notes_are_skipped_with_a_warning() -> None:
    chem = pytest.importorskip("rdkit.Chem")
    rdkit = chem.MolFromSmiles("CCO")
    for index, text in enumerate(("", "x" * 33, "a\tb")):
        rdkit.GetAtomWithIdx(index).SetProp("atomNote", text)
    with pytest.warns(reshiki.StructureWarning, match="3 of 3 RDKit atom notes were skipped"):
        drawing = reshiki.Molecule.from_rdkit(rdkit)
    assert _numbers(drawing) == {}
    rdkit.GetAtomWithIdx(0).SetProp("atomNote", "x" * 32)
    with pytest.warns(reshiki.StructureWarning, match="2 of 3"):
        assert _numbers(reshiki.Molecule.from_rdkit(rdkit)) == {1: "x" * 32}
