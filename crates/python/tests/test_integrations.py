from __future__ import annotations

import importlib
import subprocess
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

import pytest
import reshiki


def test_explicit_constructors_infer_and_check_document_kind() -> None:
    molecule = reshiki.Drawing.from_smiles("CCO")
    reaction = reshiki.Drawing.from_smiles("CCO>>CC=O")
    assert isinstance(molecule, reshiki.Molecule)
    assert isinstance(reaction, reshiki.Reaction)
    assert isinstance(reshiki.Reaction.from_smiles("CCO>>CC=O"), reshiki.Reaction)
    assert isinstance(reshiki.Drawing.from_molfile(molecule.mol()), reshiki.Molecule)
    assert isinstance(reshiki.Drawing.from_cdxml(reaction.cdxml()), reshiki.Reaction)
    with pytest.raises(ValueError, match="Molecule.*reaction"):
        reshiki.Molecule.from_smiles("CCO>>CC=O")
    with pytest.raises(ValueError, match="Molecule.*reaction"):
        reshiki.Molecule.from_cdxml(reaction.cdxml())
    with pytest.raises(ValueError, match="Reaction.*molecule"):
        reshiki.Reaction.from_smiles("CCO")
    with pytest.raises(ValueError, match="Reaction.*molecule"):
        reshiki.Reaction.from_molfile(molecule.mol())


def test_open_checks_requested_document_kind(tmp_path: Path) -> None:
    reaction_path = tmp_path / "reaction.rsk"
    reshiki.Reaction.from_smiles("CCO>>CC=O").save(reaction_path)
    assert isinstance(reshiki.Drawing.open(reaction_path), reshiki.Reaction)
    assert isinstance(reshiki.Reaction.open(reaction_path), reshiki.Reaction)
    with pytest.raises(ValueError, match="Molecule.*reaction"):
        reshiki.Molecule.open(reaction_path)
    molecule_path = tmp_path / "molecule.rsk"
    reshiki.Molecule.from_smiles("CCO").save(molecule_path)
    assert isinstance(reshiki.Molecule.open(molecule_path), reshiki.Molecule)
    with pytest.raises(ValueError, match="Reaction.*molecule"):
        reshiki.Reaction.open(molecule_path)


def test_optional_dependencies_are_not_required_for_core() -> None:
    code = """
import importlib.abc
import sys
class NoExtras(importlib.abc.MetaPathFinder):
    def find_spec(self, fullname, path=None, target=None):
        if fullname.split('.')[0] in {'pandas', 'openpyxl', 'PIL', 'rdkit'}:
            raise ModuleNotFoundError(fullname)
sys.meta_path.insert(0, NoExtras())
import reshiki
assert reshiki.properties('CCO').formula == 'C2H6O'
assert not {'pandas', 'openpyxl', 'PIL', 'rdkit'} & sys.modules.keys()
assert 'reshiki.excel' not in sys.modules
assert 'reshiki.pandas' not in sys.modules
try:
    import reshiki.pandas
except ImportError as error:
    assert 'reshiki[pandas]' in str(error)
else:
    raise AssertionError('missing pandas should raise')
try:
    reshiki.Molecule.from_smiles('CCO').to_rdkit()
except ImportError as error:
    assert 'rdkit' in str(error).lower()
else:
    raise AssertionError('missing RDKit should raise')
try:
    reshiki.excel.add_structures('missing.xlsx')
except ImportError as error:
    assert 'reshiki[excel]' in str(error)
else:
    raise AssertionError('missing Excel extras should raise')
"""
    subprocess.run([sys.executable, "-c", code], check=True, capture_output=True, text=True)


def test_excel_roundtrip(tmp_path: Path) -> None:
    openpyxl = pytest.importorskip("openpyxl")
    pytest.importorskip("PIL.Image")
    source = tmp_path / "compounds.xlsx"
    destination = tmp_path / "compounds-drawn.xlsx"
    workbook = openpyxl.Workbook()
    sheet = workbook.active
    sheet.title = "Compounds"
    sheet.append(["SMILES", "Structure", "Keep"])
    sheet.append(["CCO", None, "ethanol"])
    sheet.append([None, None, "empty"])
    sheet.append(["C1CC", None, "invalid"])
    sheet.append(["c1ccccc1", None, "benzene"])
    sheet["D2"] = "=1+1"
    workbook.save(source)
    workbook.close()
    assert (
        reshiki.excel.add_structures(
            source, smiles="A", into="B", out=destination, width=180, sheet="Compounds"
        )
        == destination
    )
    result = openpyxl.load_workbook(destination)
    assert result.active["C2"].value == "ethanol"
    assert result.active["D2"].value == "=1+1"
    assert len(result.active._images) == 2
    assert [image.anchor._from.row for image in result.active._images] == [1, 4]
    assert result.active.row_dimensions[2].height > 0
    result.close()
    original = openpyxl.load_workbook(source)
    assert not original.active._images
    original.close()


@pytest.mark.parametrize(
    ("options", "expected_rows"),
    [({"header": False}, [0, 1]), ({"header": False, "start_row": 2}, [1])],
)
def test_excel_without_header(
    tmp_path: Path, options: dict[str, bool | int], expected_rows: list[int]
) -> None:
    openpyxl = pytest.importorskip("openpyxl")
    pytest.importorskip("PIL.Image")
    source = tmp_path / "compounds.xlsx"
    workbook = openpyxl.Workbook()
    workbook.active.append(["CCO"])
    workbook.active.append(["O"])
    workbook.save(source)
    workbook.close()
    reshiki.excel.add_structures(source, width=120, **options)
    result = openpyxl.load_workbook(source)
    assert [image.anchor._from.row for image in result.active._images] == expected_rows
    result.close()


def test_pandas_accessor() -> None:
    pd = pytest.importorskip("pandas")
    importlib.import_module("reshiki.pandas")
    series = pd.Series(["CCO", "C1CC", "O"], index=["a", "bad", "water"], name="SMILES")
    pngs = series.reshiki.png(width=140)
    assert list(pngs.index) == list(series.index)
    assert pngs.name == "SMILES"
    assert isinstance(pngs.iloc[0], bytes) and pngs.iloc[1] is None
    assert pngs.attrs["reshiki_errors"][0][0] == 1
    assert "<svg" in series.reshiki.svg(width=140).iloc[0]
    values = series.reshiki.properties()
    assert list(values.index) == list(series.index)
    assert values.loc["a", "formula"] == "C2H6O"
    assert pd.isna(values.loc["bad", "mass"])
    assert values.loc["water", "formula"] == "H2O"
    with pytest.raises(ValueError, match="Row 1"):
        series.reshiki.properties(errors="raise")


def test_pandas_properties_report_missing_files(tmp_path: Path) -> None:
    pd = pytest.importorskip("pandas")
    importlib.import_module("reshiki.pandas")
    series = pd.Series(["CCO", tmp_path / "missing.mol"])
    result = series.reshiki.properties()
    assert result.loc[0, "formula"] == "C2H6O"
    assert pd.isna(result.loc[1, "formula"])
    assert result.attrs["reshiki_errors"][0][0] == 1
    with pytest.raises(ValueError, match="Row 1"):
        series.reshiki.properties(errors="raise")


def test_style_object_overrides_use_new_values() -> None:
    molecule = reshiki.Molecule.from_smiles("CCO")
    style = reshiki.Style("JACS / ACS", font_size=12, bond_length=18)
    rendered = molecule.cdxml(style=style, font_size=14, bond_length=24)
    styled = molecule.with_style(style, font_size=14, bond_length=24)
    for output in (rendered, styled.cdxml()):
        attributes = ET.fromstring(output).attrib
        assert float(attributes["LabelSize"]) == 14
        assert float(attributes["BondLength"]) == 24
    assert style.values["font_size_pt"] == 12
    assert style.values["bond_length_pt"] == 18


def test_rdkit_roundtrip_uses_mol_blocks(monkeypatch: pytest.MonkeyPatch) -> None:
    chem = pytest.importorskip("rdkit.Chem")
    molecule = chem.MolFromSmiles("C[C@H](O)C(=O)O")
    calls = []
    to_block = chem.MolToMolBlock
    from_block = chem.MolFromMolBlock

    def encode(*args, **kwargs):
        calls.append("encode")
        return to_block(*args, **kwargs)

    def decode(*args, **kwargs):
        calls.append("decode")
        return from_block(*args, **kwargs)

    monkeypatch.setattr(chem, "MolToMolBlock", encode)
    monkeypatch.setattr(chem, "MolFromMolBlock", decode)
    drawing = reshiki.Molecule.from_rdkit(molecule)
    restored = drawing.to_rdkit()
    assert drawing.properties().formula == "C3H6O3"
    assert chem.MolToSmiles(restored) == chem.MolToSmiles(molecule)
    assert calls == ["encode", "decode"]
    with pytest.raises(ValueError, match="None"):
        reshiki.Molecule.from_rdkit(None)


def test_excel_module_is_loaded_on_demand() -> None:
    assert importlib.import_module("reshiki.excel") is reshiki.excel
