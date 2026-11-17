"""NMR prediction: the rows ``reshiki --cli predict-nmr`` and the MCP ``predict_nmr`` tool return."""

from __future__ import annotations

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
