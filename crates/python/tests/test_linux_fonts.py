from __future__ import annotations

import math
import os
import subprocess
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

import pytest
from PIL import Image


def dark_pixels(image: Image.Image) -> int:
    rgba = image.convert("RGBA")
    background = Image.new("RGBA", rgba.size, "white")
    background.alpha_composite(rgba)
    return sum(background.convert("L").histogram()[:128])


@pytest.mark.skipif(sys.platform != "linux", reason="Linux font discovery uses fontconfig")
@pytest.mark.parametrize("system_fonts", ["empty", "generic-sans"])
def test_exports_keep_labels_with_restricted_system_fonts(
    tmp_path: Path, system_fonts: str
) -> None:
    font_dir = tmp_path / "fonts"
    font_dir.mkdir()
    config = ET.Element("fontconfig")
    ET.SubElement(config, "dir").text = str(font_dir)
    if system_fonts == "generic-sans":
        fixture = (
            Path(__file__).resolve().parents[3]
            / "tests"
            / "fixtures"
            / "font-export-103"
            / "static-400.subset.ttf"
        )
        (font_dir / fixture.name).write_bytes(fixture.read_bytes())
        alias = ET.SubElement(config, "alias")
        ET.SubElement(alias, "family").text = "sans-serif"
        prefer = ET.SubElement(alias, "prefer")
        ET.SubElement(prefer, "family").text = "ReShiki Font Export Fixture"
    config_path = tmp_path / "fonts.conf"
    ET.ElementTree(config).write(config_path, encoding="utf-8", xml_declaration=True)

    # Each configuration needs a fresh interpreter: font discovery is cached
    # after the process first measures or draws text.
    completed = subprocess.run(
        [
            sys.executable,
            "-c",
            """
import sys
from pathlib import Path

import reshiki

output = Path(sys.argv[1])
for name, smiles, width in [
    ("oxygen", "O", 128),
    ("aspirin", "CC(=O)Oc1ccccc1C(=O)O", 384),
]:
    molecule = reshiki.Molecule.from_smiles(smiles)
    molecule.save(output / f"{name}.svg")
    molecule.save(output / f"{name}.png", width=width)
""",
            str(tmp_path),
        ],
        env={**os.environ, "FONTCONFIG_FILE": str(config_path)},
        capture_output=True,
        text=True,
    )
    assert completed.returncode == 0, completed.stderr

    # There are no bonds in this drawing, so ink must come from its labels.
    with Image.open(tmp_path / "oxygen.png") as oxygen:
        assert dark_pixels(oxygen) > 0

    svg = ET.parse(tmp_path / "aspirin.svg").getroot()
    labels = [node for node in svg.iter("{http://www.w3.org/2000/svg}text") if node.text == "O"]
    assert len(labels) == 4
    if system_fonts == "generic-sans":
        assert {node.attrib["font-family"] for node in labels} == {"ReShiki Font Export Fixture"}

    left, top, width, _ = map(float, svg.attrib["viewBox"].split())
    with Image.open(tmp_path / "aspirin.png") as aspirin:
        scale = aspirin.width / width
        for label in labels:
            x = float(label.attrib["x"])
            y = float(label.attrib["y"])
            size = float(label.attrib["font-size"])
            # Inspect each letter's interior area, where the adjacent bonds
            # have already stopped. A bonds-only PNG must fail this check.
            crop = aspirin.crop(
                (
                    math.floor((x + size * 0.05 - left) * scale),
                    math.floor((y + size * 0.2 - top) * scale),
                    math.ceil((x + size * 0.72 - left) * scale),
                    math.ceil((y + size * 0.9 - top) * scale),
                )
            )
            assert dark_pixels(crop) > 0, f"Missing aspirin O label at ({x}, {y})"
