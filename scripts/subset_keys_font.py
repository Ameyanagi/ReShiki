# /// script
# requires-python = ">=3.11"
# dependencies = ["fonttools==4.60.1"]
# ///
"""Make assets/fonts/ReShikiKeys.ttf, the browser build's symbol font, from
DejaVu Sans 2.37 (dejavu-fonts-ttf-2.37/ttf/DejaVuSans.ttf).

Liberation Sans, the only other face the browser has, lacks these symbols;
see licenses/dejavu-fonts/NOTICE. Run with
`uv run scripts/subset_keys_font.py path/to/DejaVuSans.ttf`.
"""

import hashlib
import sys
from pathlib import Path

from fontTools import subset  # ty: ignore[unresolved-import]  (inline script dependency)
from fontTools.ttLib import TTFont  # ty: ignore[unresolved-import]

ROOT = Path(__file__).resolve().parents[1]
SOURCE_SHA256 = "7da195a74c55bef988d0d48f9508bd5d849425c1770dba5d7bfc6ce9ed848954"
# The symbols the interface draws that Liberation Sans lacks. The src/app
# test `browser_fonts_draw_every_interface_symbol` keeps this list complete.
SYMBOLS = " ⌘⇧⌥⌃⌫↩↵↗↶↷↺⌄▴▸▾◂▶△◢★☆✓⠿"
# DejaVu Sans has no full-width plus; the ASCII plus stands in for it.
ALIASES = {"＋": "+"}
NAME = "ReShiki Keys"


def main(source):
    data = Path(source).read_bytes()
    if hashlib.sha256(data).hexdigest() != SOURCE_SHA256:
        sys.exit(f"{source} is not DejaVu Sans 2.37 from dejavu-fonts-ttf-2.37")
    font = TTFont(source)
    options = subset.Options()
    options.hinting = False
    options.layout_features = []
    options.drop_tables += ["GDEF", "GPOS", "GSUB", "MATH", "FFTM"]
    options.name_IDs = [0, 1, 2, 3, 4, 5, 6, 13, 14]
    subsetter = subset.Subsetter(options)
    subsetter.populate(text=SYMBOLS + "".join(ALIASES.values()))
    subsetter.subset(font)
    best = font.getBestCmap()
    for table in font["cmap"].tables:
        if table.isUnicode():
            for alias, original in ALIASES.items():
                table.cmap[ord(alias)] = best[ord(original)]
    # The license asks a modified font to drop the DejaVu name.
    names = font["name"]
    for record in list(names.names):
        if record.nameID in (1, 4, 16):
            names.setName(NAME, record.nameID, record.platformID, record.platEncID, record.langID)
        elif record.nameID == 3:
            names.setName(
                f"{NAME} Regular (DejaVu Sans 2.37 subset)",
                3,
                record.platformID,
                record.platEncID,
                record.langID,
            )
        elif record.nameID == 6:
            names.setName("ReShikiKeys", 6, record.platformID, record.platEncID, record.langID)
    out = ROOT / "assets/fonts/ReShikiKeys.ttf"
    font.save(out)
    print(f"{out}: {out.stat().st_size} bytes")


if __name__ == "__main__":
    main(sys.argv[1])
