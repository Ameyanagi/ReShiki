"""Regenerate the development-only #103 font fixture with pinned FontTools.

Use Python with fonttools==4.61.1. No font tooling is needed by ReShiki at runtime.
The source font and its license must match the immutable upstream checksums.
"""

import argparse
import hashlib
import importlib.metadata
import json
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont

REVISION = "f8d157532fbfaeda587e826d4cd5b21a49186f7c"
BASE_URL = f"https://raw.githubusercontent.com/notofonts/noto-cjk/{REVISION}/Sans"
SOURCE_URL = f"{BASE_URL}/Variable/TTF/Subset/NotoSansJP-VF.ttf"
SOURCE_SHA256 = "f4b373b226668ee33a6e54b02823dcd2d1209f17159f777421ae8c2275160369"
LICENSE_URL = f"{BASE_URL}/LICENSE"
LICENSE_SHA256 = "6a73f9541c2de74158c0e7cf6b0a58ef774f5a780bf191f2d7ec9cc53efe2bf2"
FAMILY = "ReShiki Font Export Fixture"
FONTTOOLS_VERSION = "4.61.1"
DESTINATION = Path(__file__).resolve().parents[2] / "tests/fixtures/font-export-103"


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def rename(font, style):
    """Give every modified face a distinct family; preserve copyright/license IDs."""
    names = {
        1: FAMILY,
        2: style,
        3: f"ReShikiFontExportFixture-103-{style}",
        4: f"{FAMILY} {style}",
        6: f"ReShikiFontExportFixture-{style}",
        16: FAMILY,
        17: style,
    }
    table = font["name"]
    for record in list(table.names):
        if record.nameID in names:
            table.setName(
                names[record.nameID],
                record.nameID,
                record.platformID,
                record.platEncID,
                record.langID,
            )
    for name_id, text in names.items():
        table.setName(text, name_id, 3, 1, 0x409)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--license", required=True, type=Path)
    parser.add_argument("--output", type=Path, default=DESTINATION)
    args = parser.parse_args()
    if importlib.metadata.version("fonttools") != FONTTOOLS_VERSION:
        parser.error(f"Use fonttools=={FONTTOOLS_VERSION} for reproducible fixtures")
    source = args.source.read_bytes()
    if sha256(source) != SOURCE_SHA256:
        parser.error("Source font checksum does not match the pinned upstream font")
    license_bytes = args.license.read_bytes()
    if sha256(license_bytes) != LICENSE_SHA256:
        parser.error("License checksum does not match the complete upstream SIL OFL 1.1")

    font = TTFont(args.source, recalcTimestamp=False)
    copyright_notice = font["name"].getDebugName(0)
    axes = [
        {
            "tag": axis.axisTag,
            "min": axis.minValue,
            "default": axis.defaultValue,
            "max": axis.maxValue,
        }
        for axis in font["fvar"].axes
    ]
    if axes != [{"tag": "wght", "min": 100.0, "default": 100.0, "max": 900.0}]:
        parser.error(f"Unexpected source axes: {axes}")

    options = subset.Options()
    options.name_IDs = ["*"]
    options.name_legacy = True
    options.name_languages = ["*"]
    options.recalc_timestamp = False
    subsetter = subset.Subsetter(options=options)
    subsetter.populate(text="HNO")
    subsetter.subset(font)
    args.output.mkdir(parents=True, exist_ok=True)

    files = {}
    # FontTools supplies independent static controls, not the renderer under test.
    for weight, style in [(100, "Thin"), (400, "Regular"), (700, "Bold")]:
        static = instantiateVariableFont(font, {"wght": weight}, inplace=False)
        rename(static, style)
        name = f"static-{weight}.subset.ttf"
        static.save(args.output / name)
        files[name] = sha256((args.output / name).read_bytes())
    rename(font, "Thin")
    name = "variable-default-100.subset.ttf"
    font.save(args.output / name)
    files[name] = sha256((args.output / name).read_bytes())
    (args.output / "OFL.txt").write_bytes(license_bytes)
    files["OFL.txt"] = sha256(license_bytes)
    manifest = {
        "issue": "https://github.com/Ameyanagi/ReShiki/issues/103",
        "source": {
            "repository": "https://github.com/notofonts/noto-cjk",
            "revision": REVISION,
            "url": SOURCE_URL,
            "sha256": SOURCE_SHA256,
            "license_url": LICENSE_URL,
            "copyright": copyright_notice,
            "axes": axes,
            "original_family": "Noto Sans JP",
            "face_index": 0,
        },
        "generation": {
            "script": "tools/font-export-probe/prepare_fixture.py",
            "fonttools": FONTTOOLS_VERSION,
            "characters": "HNO",
            "family": FAMILY,
            "static_weights": [100, 400, 700],
            "note": "Subset and rename; retain the original 100 default in the variable fixture.",
        },
        "files": files,
    }
    (args.output / "source.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps(manifest, indent=2))


if __name__ == "__main__":
    main()
