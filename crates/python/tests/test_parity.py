"""One input, the same answers on every surface.

``tests/fixtures/agent-api/parity.json`` is written by ``reshiki --cli``
(``scripts/agent_parity_fixture.py``) and also checked by ``tests/cli.rs`` and
``packages/editor/test/parity.test.ts``.
"""

from __future__ import annotations

import json
from dataclasses import asdict
from pathlib import Path
from typing import Any

import pytest
import reshiki


@pytest.fixture(scope="module")
def parity(fixtures: Path) -> dict[str, Any]:
    return json.loads((fixtures / "agent-api" / "parity.json").read_text(encoding="utf-8"))


def test_the_molecule_converts_as_the_command_line_converts_it(parity: dict[str, Any]) -> None:
    molecule = reshiki.Molecule.from_smiles(parity["molecule"]["smiles"])
    outputs = parity["molecule"]["outputs"]
    assert molecule.smiles() == outputs["smiles"]
    assert molecule.inchi() == outputs["inchi"]
    assert reshiki.convert(parity["molecule"]["smiles"], to="smiles") == outputs["smiles"]


def test_properties_are_the_rounded_values_the_command_line_prints(
    parity: dict[str, Any],
) -> None:
    properties = asdict(reshiki.properties(parity["molecule"]["smiles"]))
    # Exactly equal: every surface rounds mass, exact_mass, logp and tpsa to 9 places.
    for name, expected in parity["molecule"]["properties"].items():
        assert properties[name] == expected, name


def test_the_reaction_converts_as_the_command_line_converts_it(parity: dict[str, Any]) -> None:
    reaction = reshiki.Reaction.from_smiles(parity["reaction"]["rsmi"])
    assert reaction.rsmi() == parity["reaction"]["outputs"]["rsmi"]
    assert reaction.rxn().startswith("$RXN")
    assert reaction.cdx().startswith(b"VjCD0100")


def test_svg_text_is_outlined_unless_live(parity: dict[str, Any]) -> None:
    molecule = reshiki.Molecule.from_smiles(parity["molecule"]["smiles"])
    for svg in (molecule.svg(), molecule.export("svg"), molecule.svg(svg_text="outline")):
        assert "<path" in svg and "<text" not in svg
    assert "<text" in molecule.svg(svg_text="live")
    # The older spelling of the same option.
    assert molecule.svg(text="live") == molecule.svg(svg_text="live")
    with pytest.raises(reshiki.ReShikiError) as error:
        molecule.svg(svg_text="live", text="outline")
    assert error.value.code == "invalid_arguments"


def test_errors_carry_the_operation_error_code() -> None:
    with pytest.raises(reshiki.ReShikiError) as unreadable:
        reshiki.Molecule.from_smiles("C(")
    assert unreadable.value.code == "failed"
    assert isinstance(unreadable.value, ValueError)
    # Malformed content in a supported format, under any of its names.
    for format in ("mol", "sdf", "cif", ".SMI"):
        with pytest.raises(reshiki.ReShikiError) as malformed:
            reshiki.parse("C(", format=format)
        assert malformed.value.code == "failed", format
    molecule = reshiki.Molecule.from_smiles("CCO")
    for call in (
        lambda: reshiki.parse("CCO", format="nonsense"),
        # A constructor given another kind of content.
        lambda: reshiki.Molecule.from_smiles("CCO>>CC=O"),
        lambda: reshiki.Reaction.from_smiles("CCO"),
        lambda: molecule.export("xyz"),
        lambda: molecule.export("png", width=0),
        lambda: molecule.export("svg", svg_text="bold"),
        lambda: reshiki.Style("No such style"),
    ):
        with pytest.raises(reshiki.ReShikiError) as invalid:
            call()
        assert invalid.value.code == "invalid_arguments"
        assert isinstance(invalid.value, ValueError)
    with pytest.raises(reshiki.ReShikiError, match="Unsupported input format: 'nonsense'"):
        reshiki.parse("CCO", format="nonsense")


def test_export_is_render_and_save_can_refuse_to_overwrite(tmp_path: Path) -> None:
    molecule = reshiki.Molecule.from_smiles("CCO")
    assert molecule.export("mol") == molecule.render("mol") == reshiki.export("CCO", "mol")
    path = tmp_path / "ethanol.mol"
    molecule.save(path)
    molecule.save(path)  # replaces by default
    with pytest.raises(FileExistsError):
        molecule.save(path, overwrite=False)
    assert molecule.save(tmp_path / "new.mol", overwrite=False).read_text() == molecule.mol()
