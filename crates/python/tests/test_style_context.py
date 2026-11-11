from __future__ import annotations

import json
import sys
import xml.etree.ElementTree as ET
from dataclasses import FrozenInstanceError
from pathlib import Path
from types import ModuleType, SimpleNamespace
from typing import Any

import pytest
import reshiki

ASPIRIN = "CC(=O)Oc1ccccc1C(=O)O"
ETHANOL_INCHI = "InChI=1S/C2H6O/c1-2-3/h3H,2H2,1H3"


def drawing_style(drawing: reshiki.Drawing) -> dict[str, Any]:
    values = json.loads(drawing.rsk()).get("drawing_style", {})
    return reshiki.Style(**values).as_dict()


def test_style_first_constructor_persists_through_clean_and_save(tmp_path: Path) -> None:
    custom = reshiki.with_style("JACS / ACS", font_size=16, bond_length=24)
    aspirin = custom.Molecule.from_smiles(ASPIRIN)

    assert isinstance(custom, reshiki.StyleContext)
    assert type(aspirin) is reshiki.Molecule
    assert aspirin.properties().formula == "C9H8O4"
    assert aspirin.rsk() == reshiki.Molecule.from_smiles(ASPIRIN).with_style(custom.style).rsk()

    cleaned = aspirin.clean()
    assert type(cleaned) is reshiki.Molecule
    assert drawing_style(cleaned) == custom.style.as_dict()
    assert float(ET.fromstring(cleaned.cdxml()).attrib["LabelSize"]) == 16
    target = cleaned.save(tmp_path / "aspirin.rsk")
    restored = reshiki.Molecule.open(target)
    assert drawing_style(restored) == custom.style.as_dict()
    assert float(ET.fromstring(restored.cdxml()).attrib["LabelSize"]) == 16


def test_contexts_are_immutable_and_do_not_change_defaults() -> None:
    original = reshiki.Molecule.from_smiles("CCO")
    original_bytes = original.rsk()
    first = reshiki.with_style(font_size=16)
    second = reshiki.with_style(font_size=12)

    assert first.Molecule.from_smiles("CCO").rsk() != second.Molecule.from_smiles("CCO").rsk()
    assert original.rsk() == original_bytes
    assert reshiki.Molecule.from_smiles("CCO").rsk() == original_bytes
    assert reshiki.Style().values["font_size_pt"] == 10
    assert first.style.values["font_size_pt"] == 16
    assert second.style.values["font_size_pt"] == 12
    with pytest.raises(FrozenInstanceError):
        first._style = second.style  # type: ignore[misc]


def test_style_and_context_overrides_compose_without_mutating_the_base() -> None:
    style = reshiki.Style("JACS / ACS", font_size=11, line_width=0.9)
    custom = reshiki.with_style(style, font_size=15, bond_length=24)
    derived = custom.with_style(line_width=1.2)

    assert style.values["font_size_pt"] == 11
    assert style.values["bond_length_pt"] == 14.4
    assert custom.style.values["font_size_pt"] == 15
    assert custom.style.values["line_width_pt"] == 0.9
    assert derived.style.values["font_size_pt"] == 15
    assert derived.style.values["line_width_pt"] == 1.2
    assert derived.style.values["bond_length_pt"] == 24
    assert derived.style.values["bond_length_world"] == pytest.approx(70)

    replaced = derived.with_style("JACS / ACS", font_size=17)
    assert replaced.style.values["font_size_pt"] == 17
    assert replaced.style.values["line_width_pt"] == 0.6
    assert replaced.style.values["bond_length_pt"] == 14.4
    replaced_again = derived.with_style(reshiki.Style(font_size=19), font_size=20)
    assert replaced_again.style.values["font_size_pt"] == 20
    assert replaced_again.style.values["line_width_pt"] == 0.6


def test_configuration_can_be_applied_before_or_after_construction() -> None:
    custom = reshiki.with_style(font_size=16).with_style(bond_length=24)
    before = custom.Molecule.from_smiles("CCO")
    after = reshiki.Molecule.from_smiles("CCO").with_style(font_size=16).with_style(bond_length=24)

    assert before.rsk() == after.rsk()
    parsed = custom.parse("CCO")
    assert type(parsed) is reshiki.Molecule
    assert parsed.rsk() == reshiki.parse("CCO").with_style(custom.style).rsk()
    reaction = custom.parse("CCO>>CC=O")
    assert type(reaction) is reshiki.Reaction
    assert drawing_style(reaction) == custom.style.as_dict()


def test_bound_molecular_constructors(fixtures: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    custom = reshiki.with_style(font_size=16)
    molfile = (fixtures / "ethanol.mol").read_text()
    cdxml = (fixtures / "reference-ethanol.cdxml").read_bytes()
    monkeypatch.setattr(reshiki, "_rdkit", lambda: SimpleNamespace(MolToMolBlock=lambda _: molfile))

    molecules = (
        custom.Molecule.open(fixtures / "ethanol.mol"),
        custom.Molecule.from_molfile(molfile),
        custom.Molecule.from_cdxml(cdxml),
        custom.Molecule.from_inchi(ETHANOL_INCHI),
        custom.Molecule.from_name("ethanol"),
        custom.Molecule.from_rdkit(object()),
        custom.Drawing.from_smiles("CCO"),
        custom.Drawing.open(fixtures / "ethanol.mol"),
    )
    for molecule in molecules:
        assert type(molecule) is reshiki.Molecule
        assert molecule.properties().formula == "C2H6O"
        assert drawing_style(molecule) == custom.style.as_dict()


def test_bound_reaction_constructors(fixtures: Path) -> None:
    custom = reshiki.with_style(font_size=16)
    molfile = (fixtures / "ethanol.mol").read_text()
    rxn = f"$RXN\n\nReShiki\n\n  1  1\n$MOL\n{molfile}$MOL\n{molfile}"
    reaction = custom.Reaction.from_smiles("CCO>>CC=O")
    for drawing in (
        reaction,
        custom.Reaction.from_cdxml(reaction.cdxml()),
        custom.Reaction.from_rxn(rxn),
        custom.Drawing.from_smiles("CCO>>CC=O"),
    ):
        assert type(drawing) is reshiki.Reaction
        assert drawing_style(drawing) == custom.style.as_dict()


def test_functional_exports_use_context_style_with_per_call_overrides(tmp_path: Path) -> None:
    custom = reshiki.with_style(font_size=16)
    source = tmp_path / "ethanol.smi"
    source.write_text("CCO")

    for output in (
        custom.render("CCO", "cdxml"),
        custom.convert(source, to="cdxml"),
        custom.render(reshiki.Molecule.from_smiles("CCO"), "cdxml"),
    ):
        assert isinstance(output, str)
        assert float(ET.fromstring(output).attrib["LabelSize"]) == 16
    overridden = custom.render("CCO", "cdxml", font_size=18)
    assert isinstance(overridden, str)
    assert float(ET.fromstring(overridden).attrib["LabelSize"]) == 18
    replaced = custom.convert("CCO", to="cdxml", style=reshiki.Style(font_size=12))
    assert isinstance(replaced, str)
    assert float(ET.fromstring(replaced).attrib["LabelSize"]) == 12
    preset = custom.render("CCO", "cdxml", style="JACS / ACS", font_size=14)
    assert isinstance(preset, str)
    assert float(ET.fromstring(preset).attrib["LabelSize"]) == 14
    assert isinstance(custom.render("CCO", width=80), bytes)
    assert custom.style.values["font_size_pt"] == 16


def test_context_batch_preserves_results_and_error_options() -> None:
    custom = reshiki.with_style(font_size=16)
    results = custom.render_many(["CCO", "C1CC", "O"], "cdxml", font_size=18)
    assert isinstance(results, reshiki.RenderResults)
    assert results[1] is None
    assert [index for index, _ in results.errors] == [1]
    for index in (0, 2):
        output = results[index]
        assert isinstance(output, str)
        assert float(ET.fromstring(output).attrib["LabelSize"]) == 18
    with pytest.raises(ValueError, match="Row 1"):
        custom.render_many(["CCO", "C1CC"], errors="raise")
    replaced = custom.render_many(["CCO"], "cdxml", style=reshiki.Style(font_size=12))
    output = replaced[0]
    assert isinstance(output, str)
    assert float(ET.fromstring(output).attrib["LabelSize"]) == 12


def test_context_configuration_is_validated_before_use() -> None:
    for options in ({"font_size": -1}, {"mystery_setting": 3}):
        with pytest.raises(ValueError):
            reshiki.with_style(**options)
        with pytest.raises(ValueError):
            reshiki.with_style(reshiki.Style(), **options)
    with pytest.raises(ValueError, match="(?i)unknown drawing style"):
        reshiki.with_style("missing-style")
    with pytest.raises(TypeError, match="style must be"):
        reshiki.with_style(42)  # type: ignore[arg-type]
    custom = reshiki.with_style(font_size=16)
    with pytest.raises(ValueError):
        custom.with_style(mystery_setting=3)
    assert custom.style.values["font_size_pt"] == 16


def test_bound_constructors_preserve_type_errors() -> None:
    custom = reshiki.with_style(font_size=16)
    with pytest.raises(ValueError, match="Molecule requires molecule content"):
        custom.Molecule.from_smiles("CCO>>CC=O")
    with pytest.raises(ValueError, match="Reaction requires reaction content"):
        custom.Reaction.from_smiles("CCO")
    with pytest.raises(ValueError):
        custom.parse("C1CC", format="smiles")


def test_online_name_resolution_is_forwarded_by_bound_constructors(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    calls: list[str] = []
    data = reshiki.Molecule.from_smiles(ASPIRIN)._data
    online = ModuleType("reshiki._online")

    def from_name(name: str) -> tuple[bytes, str]:
        calls.append(name)
        return data, "molecule"

    monkeypatch.setattr(online, "from_name", from_name, raising=False)
    monkeypatch.setitem(sys.modules, "reshiki._online", online)
    custom = reshiki.with_style(font_size=16)
    aspirin = custom.Molecule.from_name("aspirin", online=True)

    assert calls == ["aspirin"]
    assert type(aspirin) is reshiki.Molecule
    assert aspirin.properties().formula == "C9H8O4"
    assert drawing_style(aspirin) == custom.style.as_dict()


@pytest.mark.parametrize(
    ("option", "enabled", "default"),
    [
        ("center_chain_double_bonds", True, False),
        ("two_wedge_stereocentres", True, False),
        ("ring_fusion_hydrogens", False, True),
        ("wavy_unknown_alkenes", True, False),
        ("charge_before_radical", True, False),
        ("hydrogen_isotopes", "letters", "auto"),
        ("crossing_clearance_pt", 0.3, 1.1),
    ],
)
def test_context_defaults_reset_existing_drawings(
    option: str, enabled: Any, default: Any, tmp_path: Path
) -> None:
    custom = reshiki.with_style(**{option: enabled})
    molecule = custom.Molecule.from_smiles("CC=CC")
    assert drawing_style(molecule)[option] == enabled
    reset = custom.with_style(**{option: default})
    filename = molecule.save(tmp_path / "styled.rsk")

    rendered = reset.render(molecule, "rsk")
    batch = reset.render_many([molecule], "rsk")[0]
    assert isinstance(rendered, bytes) and isinstance(batch, bytes)
    for result in (
        molecule.with_style(reset.style),
        reset.Drawing.open(filename),
        reset.parse(molecule.rsk(), format="rsk"),
        reshiki.parse(rendered, format="rsk"),
        reshiki.parse(batch, format="rsk"),
    ):
        assert drawing_style(result)[option] == default
    assert drawing_style(molecule)[option] == enabled

    partial = molecule.with_style(font_size=14)
    assert drawing_style(partial)[option] == enabled
    assert drawing_style(partial)["font_size_pt"] == 14
