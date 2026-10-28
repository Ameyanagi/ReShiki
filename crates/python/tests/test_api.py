from __future__ import annotations

import json
import math
import os
import struct
import subprocess
import xml.etree.ElementTree as ET
from dataclasses import FrozenInstanceError, asdict
from pathlib import Path
from threading import Event, Thread

import pytest
import reshiki

ASPIRIN = "CC(=O)Oc1ccccc1C(=O)O"
ETHANOL_INCHI = "InChI=1S/C2H6O/c1-2-3/h3H,2H2,1H3"


def png_size(data: bytes) -> tuple[int, int]:
    assert data.startswith(b"\x89PNG\r\n\x1a\n")
    return struct.unpack(">II", data[16:24])


@pytest.mark.parametrize(
    ("filename", "formula"),
    [
        ("ethanol.mol", "C2H6O"),
        ("reference-ethanol.cdxml", "C2H6O"),
        ("assistant-benchmark/v1/ethanol.rsk", "C2H6O"),
    ],
)
def test_open_molecular_fixtures(fixtures: Path, filename: str, formula: str) -> None:
    drawing = reshiki.Drawing.open(fixtures / filename)
    assert drawing.properties().formula == formula
    assert "<svg" in drawing.svg(width=180)


@pytest.mark.parametrize(
    "filename",
    [
        "ligand-exchange/native.cdx",
        "crystal/co2.cif",
        "crystal/urea.pdb",
        "crystal/rutile.vasp",
        "crystal/water.xyz",
        "crystal/silicon.extxyz",
    ],
)
def test_open_all_binary_and_crystal_families(fixtures: Path, filename: str) -> None:
    drawing = reshiki.Drawing.open(fixtures / filename)
    assert isinstance(drawing, reshiki.Drawing)
    assert "<svg" in drawing.svg(width=160, height=160)
    assert drawing.rsk()


@pytest.mark.parametrize("filename", ["POSCAR", "CONTCAR", "rutile.VASP"])
def test_poscar_filenames(fixtures: Path, tmp_path: Path, filename: str) -> None:
    target = tmp_path / filename
    target.write_bytes((fixtures / "crystal/rutile.vasp").read_bytes())
    assert "<svg" in reshiki.Drawing.open(target).svg(width=150)


@pytest.mark.parametrize("extension", ["ent", "pdb"])
def test_pdb_alias(fixtures: Path, tmp_path: Path, extension: str) -> None:
    target = tmp_path / f"urea.{extension}"
    target.write_bytes((fixtures / "crystal/urea.pdb").read_bytes())
    assert reshiki.Drawing.open(target).rsk()


def test_explicit_and_automatic_molecular_constructors(fixtures: Path) -> None:
    molfile = (fixtures / "ethanol.mol").read_text()
    cdxml = (fixtures / "reference-ethanol.cdxml").read_bytes()
    for molecule in (
        reshiki.Molecule.from_smiles("CCO"),
        reshiki.Molecule.from_molfile(molfile),
        reshiki.Molecule.from_inchi(ETHANOL_INCHI),
        reshiki.Molecule.from_cdxml(cdxml),
        reshiki.parse("CCO"),
        reshiki.parse(b"CCO"),
        reshiki.parse(molfile),
        reshiki.parse(molfile + "\n$$$$\n", format="sdf"),
        reshiki.parse(ETHANOL_INCHI),
        reshiki.parse(cdxml),
    ):
        assert isinstance(molecule, reshiki.Drawing)
        assert molecule.properties().formula == "C2H6O"


def test_offline_name_resolution() -> None:
    assert reshiki.Molecule.from_name("ethanol").properties().formula == "C2H6O"
    with pytest.raises(NotImplementedError, match="(?i)(pubchem|online)"):
        reshiki.Molecule.from_name("ethanol", online=True)


def test_reaction_smiles_and_rxn(fixtures: Path) -> None:
    assert isinstance(reshiki.parse("CCO>>CC=O"), reshiki.Reaction)
    reaction = reshiki.Reaction.from_smiles("CCO>>CC=O")
    assert "<svg" in reaction.clean().svg(width=240)
    assert reaction.smiles() == "CCO>>CC=O"
    assert reshiki.Reaction.from_cdxml(reaction.cdxml()).smiles() == "CCO>>CC=O"
    mol = (fixtures / "ethanol.mol").read_text()
    rxn = f"$RXN\n\nReShiki\n\n  1  1\n$MOL\n{mol}$MOL\n{mol}"
    assert "<svg" in reshiki.Reaction.from_rxn(rxn).svg(width=240)
    assert isinstance(reshiki.parse(rxn), reshiki.Reaction)


def test_auto_detection_binary_and_crystal(fixtures: Path) -> None:
    for filename in (
        "ligand-exchange/native.cdx",
        "crystal/co2.cif",
        "crystal/urea.pdb",
        "crystal/rutile.vasp",
        "crystal/water.xyz",
        "crystal/silicon.extxyz",
    ):
        assert reshiki.parse((fixtures / filename).read_bytes()).rsk()


def test_open_sdf_rxn_smiles_and_inchi(fixtures: Path, tmp_path: Path) -> None:
    mol = (fixtures / "ethanol.mol").read_text()
    rxn = f"$RXN\n\nReShiki\n\n  1  1\n$MOL\n{mol}$MOL\n{mol}"
    for extension, text in (
        ("sdf", mol + "\n$$$$\n"),
        ("smi", "CCO"),
        ("rsmi", "CCO>>CC=O"),
        ("rxn", rxn),
        ("inchi", ETHANOL_INCHI),
    ):
        path = tmp_path / f"structure.{extension}"
        path.write_text(text)
        drawing = reshiki.Drawing.open(path)
        assert "<svg" in drawing.svg(width=180)
        expected = reshiki.Reaction if extension in {"rsmi", "rxn"} else reshiki.Molecule
        assert isinstance(drawing, expected)


def test_chain_is_immutable_and_preserves_type() -> None:
    original = reshiki.Molecule.from_smiles(ASPIRIN)
    original_bytes = original.rsk()
    cleaned = original.clean()
    styled = cleaned.with_style("JACS / ACS", font_size=12)
    assert original.rsk() == original_bytes
    assert cleaned is not original and styled is not cleaned
    assert isinstance(styled, reshiki.Molecule)
    assert styled.properties().formula == "C9H8O4"
    with pytest.raises(FrozenInstanceError):
        styled._data = b"changed"  # type: ignore[misc]
    assert float(ET.fromstring(styled.cdxml()).attrib["LabelSize"]) == 12


def test_styles_share_app_presets_and_accept_overrides() -> None:
    presets = reshiki.styles()
    assert "JACS / ACS" in presets
    style = reshiki.Style("JACS / ACS", font_size=12, bond_length=18)
    assert style.values["font_size_pt"] == 12
    assert style.values["bond_length_pt"] == 18
    assert style.name == "JACS / ACS"
    with pytest.raises(TypeError):
        style.values["font_size_pt"] = 99  # type: ignore[index]
    first = style.as_dict()
    first["font_size_pt"] = 99
    assert style.values["font_size_pt"] == 12
    assert "<svg" in reshiki.render("CCO", "svg", style=style, font_size=14)
    xml = reshiki.convert("CCO", to="cdxml", font_size=14)
    assert float(ET.fromstring(xml).attrib["LabelSize"]) == 14
    with pytest.raises(ValueError):
        reshiki.Style("missing-style")
    with pytest.raises(ValueError):
        reshiki.Style("JACS / ACS", mystery_setting=3)


def test_binary_and_text_outputs_and_save(tmp_path: Path) -> None:
    molecule = reshiki.Molecule.from_smiles(ASPIRIN).clean()
    assert png_size(molecule.png(width=300))[0] <= 300
    assert molecule.pdf(width=300).startswith(b"%PDF-")
    assert "<svg" in molecule.svg(width=300)
    assert "<CDXML" in molecule.cdxml()
    assert "M  END" in molecule.mol()
    assert "$$$$" in molecule.sdf()
    assert molecule.smiles()
    assert molecule.inchi().startswith("InChI=1S/")
    assert reshiki.parse(molecule.rsk()).properties().formula == "C9H8O4"
    for extension in ("png", "svg", "pdf", "cdxml", "mol", "sdf", "smi", "inchi", "rsk"):
        target = tmp_path / f"aspirin.{extension}"
        assert molecule.save(target, width=200) == target
        assert target.stat().st_size > 0
    assert molecule._repr_svg_().startswith("<")
    with pytest.raises(ValueError, match="extension"):
        molecule.save(tmp_path / "aspirin")
    with pytest.raises(NotImplementedError, match="follow-up.*462"):
        molecule.emf()


def test_size_fit_and_dpi() -> None:
    molecule = reshiki.Molecule.from_smiles(ASPIRIN)
    low = png_size(molecule.png(dpi=72))
    high = png_size(molecule.png(dpi=144))
    assert high[0] == pytest.approx(low[0] * 2, abs=2)
    assert high[1] == pytest.approx(low[1] * 2, abs=2)
    fit = png_size(molecule.png(width=180, height=90))
    assert 0 < fit[0] <= 180 and 0 < fit[1] <= 90
    for options in (
        {"width": 0},
        {"height": -1},
        {"width": 1.5},
        {"dpi": 0},
        {"dpi": math.nan},
        {"dpi": math.inf},
        {"width": True},
    ):
        with pytest.raises(ValueError):
            molecule.png(**options)


def test_aspirin_properties() -> None:
    value = reshiki.properties(ASPIRIN)
    assert isinstance(value, reshiki.Properties)
    assert value.formula == "C9H8O4"
    assert value.mass == pytest.approx(180.16, abs=0.02)
    assert value.exact_mass == pytest.approx(180.04226, abs=0.0001)
    assert value.donors == 1 and value.acceptors == 3 and value.rings == 1
    assert value.tpsa == pytest.approx(63.6, abs=0.1)
    assert value.unpaired_electrons == 0
    assert value.inchi.startswith("InChI=1S/C9H8O4/")
    assert value.inchikey == "BSYNRYMUTXBXSQ-UHFFFAOYSA-N"
    with pytest.raises(FrozenInstanceError):
        value.mass = 1.0  # type: ignore[misc]


def test_properties_match_cli() -> None:
    # Captured from this source with:
    # reshiki --cli analyze --smiles 'CC(=O)Oc1ccccc1C(=O)O'
    result = json.loads((Path(__file__).parent / "fixtures/aspirin-cli.json").read_text())
    analysis = result["value"]["analysis"]
    actual = asdict(reshiki.properties(ASPIRIN))
    assert actual.keys() == analysis.keys()
    for field, value in actual.items():
        assert analysis[field] == (pytest.approx(value) if isinstance(value, float) else value)


def test_properties_match_live_cli() -> None:
    candidate = os.environ.get("RESHIKI_CLI")
    if not candidate:
        default = Path(__file__).resolve().parents[3] / "target/debug/reshiki"
        if not default.is_file():
            pytest.skip("Set RESHIKI_CLI to compare descriptors with a built CLI")
        candidate = str(default)
    completed = subprocess.run(
        [candidate, "--cli", "analyze", "--smiles", ASPIRIN],
        check=True,
        capture_output=True,
        text=True,
    )
    result = json.loads(completed.stdout)
    analysis = result["value"]["analysis"]
    for field, value in asdict(reshiki.properties(ASPIRIN)).items():
        assert analysis[field] == (pytest.approx(value) if isinstance(value, float) else value)


def test_batch_errors_and_order() -> None:
    results = reshiki.render_many(["CCO", "C1CC", reshiki.Molecule.from_smiles("O"), None])
    assert isinstance(results, list)
    assert isinstance(results[0], bytes) and results[1] is None
    assert isinstance(results[2], bytes) and results[3] is None
    assert [index for index, _ in results.errors] == [1, 3]
    assert all(message for _, message in results.errors)
    with pytest.raises(ValueError, match="Row 1"):
        reshiki.render_many(["CCO", "C1CC"], errors="raise")
    with pytest.raises(ValueError, match="errors"):
        reshiki.render_many(["CCO"], errors="ignore")
    assert reshiki.render_many([]) == []
    assert reshiki.render_many([], format="svg").errors == []
    assert all(isinstance(item, str) for item in reshiki.render_many(["CCO", "O"], "svg"))


def test_batch_allows_other_python_threads_to_run() -> None:
    stop = Event()
    started = Event()
    ticks: list[None] = []

    def heartbeat() -> None:
        started.set()
        while not stop.wait(0.001):
            ticks.append(None)

    thread = Thread(target=heartbeat)
    thread.start()
    started.wait()
    before = len(ticks)
    try:
        results = reshiki.render_many([ASPIRIN] * 256, width=120)
        progress = len(ticks) - before
    finally:
        stop.set()
        thread.join()
    assert len(results) == 256 and not results.errors
    assert progress > 5


def test_invalid_single_calls_raise() -> None:
    for action in (
        lambda: reshiki.parse("C1CC"),
        lambda: reshiki.parse(b"bad", format="unknown"),
        lambda: reshiki.render("C1CC"),
        lambda: reshiki.properties("C1CC"),
        lambda: reshiki.convert("CCO", to="unknown"),
    ):
        with pytest.raises(ValueError):
            action()
    with pytest.raises(TypeError):
        reshiki.parse(None)
