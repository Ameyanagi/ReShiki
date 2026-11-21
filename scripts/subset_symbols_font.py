# /// script
# requires-python = ">=3.11"
# dependencies = ["fonttools==4.60.1"]
# ///
"""Make crates/model/fonts/ReShikiSymbols.ttf, the bundled fallback for the
symbol picker's symbols that Liberation Sans lacks, from DejaVu Sans 2.37 as
Ubuntu's fonts-dejavu-core 2.37-8 installs it
(/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf).

See licenses/dejavu-fonts/NOTICE. Run with
`uv run scripts/subset_symbols_font.py path/to/DejaVuSans.ttf`.
"""

import hashlib
import sys
from pathlib import Path

from fontTools import subset  # ty: ignore[unresolved-import]  (inline script dependency)
from fontTools.ttLib import TTFont  # ty: ignore[unresolved-import]

ROOT = Path(__file__).resolve().parents[1]
SOURCE_SHA256 = "ae7b7855e115a5966d8b1b3f80f254ccc117ec86f9965e202ee2940453837280"
# The picker's symbols that Liberation Sans lacks. The model test
# `bundled_fonts_draw_every_picker_symbol` keeps this list complete.
SYMBOLS = "⁺⁻↺↻⇄⇌⇒∝⊕⊖✓✗⟶"
NAME = "ReShiki Symbols"


def main(source):
    data = Path(source).read_bytes()
    if hashlib.sha256(data).hexdigest() != SOURCE_SHA256:
        sys.exit(f"{source} is not DejaVu Sans 2.37 from fonts-dejavu-core 2.37-8")
    font = TTFont(source)
    options = subset.Options()
    options.hinting = False
    options.layout_features = []
    options.drop_tables += ["GDEF", "GPOS", "GSUB", "MATH", "FFTM"]
    options.name_IDs = [0, 1, 2, 3, 4, 5, 6, 13, 14]
    subsetter = subset.Subsetter(options)
    subsetter.populate(text=" " + SYMBOLS)
    subsetter.subset(font)
    # The license asks a modified font to drop the DejaVu name.
    names = font["name"]
    for record in list(names.names):
        ids = (record.platformID, record.platEncID, record.langID)
        if record.nameID in (1, 4, 16):
            names.setName(NAME, record.nameID, *ids)
        elif record.nameID == 3:
            names.setName(f"{NAME} Regular (DejaVu Sans 2.37 subset)", 3, *ids)
        elif record.nameID == 6:
            names.setName("ReShikiSymbols", 6, *ids)
    out = ROOT / "crates/model/fonts/ReShikiSymbols.ttf"
    font.save(out)
    print(f"{out}: {out.stat().st_size} bytes")


if __name__ == "__main__":
    main(sys.argv[1])
