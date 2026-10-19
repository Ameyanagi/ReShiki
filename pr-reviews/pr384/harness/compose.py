"""Crop the label areas of the issue #377 renders and lay them out side by side.

Usage: uv run --with pillow compose.py RAW_DIR OUT_DIR
"""

import sys
from pathlib import Path

from PIL import Image, ImageDraw

raw, out = Path(sys.argv[1]), Path(sys.argv[2])
out.mkdir(parents=True, exist_ok=True)

# Fractions of the full render (left, top, right, bottom) around each label.
CROPS = {
    "c4-OBn": (0.0, 0.4, 0.6, 0.85),
    "c1-MeO": (0.3, 0.4, 1.0, 0.85),
}
STAGES = [("main", "before: origin/main"), ("pr382", "PR #382 alone"), ("after", "after: this branch")]


def crop(path: Path, box: tuple[float, float, float, float]) -> Image.Image:
    image = Image.open(path).convert("RGB")
    w, h = image.size
    left, top, right, bottom = box
    return image.crop((int(left * w), int(top * h), int(right * w), int(bottom * h)))


for case, box in CROPS.items():
    tiles = []
    for stage, caption in STAGES:
        tile = crop(raw / f"{stage}-{case}.png", box)
        tile.save(out / f"{case}-{stage}-crop.png")
        tiles.append((caption, tile))
    pad, label = 16, 28
    width = sum(t.width for _, t in tiles) + pad * (len(tiles) + 1)
    height = max(t.height for _, t in tiles) + label + pad * 2
    sheet = Image.new("RGB", (width, height), "white")
    draw = ImageDraw.Draw(sheet)
    x = pad
    for caption, tile in tiles:
        draw.text((x, pad // 2), caption, fill="black")
        sheet.paste(tile, (x, pad + label))
        draw.rectangle((x, pad + label, x + tile.width - 1, pad + label + tile.height - 1), outline="#999999")
        x += tile.width + pad
    sheet.save(out / f"{case}-comparison.png")
