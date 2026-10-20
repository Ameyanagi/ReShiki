"""Contact sheet: rows = styles, cols = looks, for one molecule and size suffix."""
import sys
from PIL import Image, ImageDraw
d, mol, suffix, out, cell = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4], int(sys.argv[5])
styles = ["ballstick", "stick", "wire", "space"]; looks = ["flat", "glossy"]
W = Image.new("RGB", (60 + cell * 2, 30 + cell * 4), "white"); g = ImageDraw.Draw(W)
for j, l in enumerate(looks): g.text((60 + j * cell + 5, 8), l, fill="black")
for i, s in enumerate(styles):
    g.text((4, 30 + i * cell + cell // 2), s, fill="black")
    for j, l in enumerate(looks):
        im = Image.open(f"{d}/{mol}-{s}-{l}-{suffix}.png").convert("RGBA")
        bg = Image.new("RGBA", im.size, "white"); bg.alpha_composite(im); im = bg.convert("RGB")
        im.thumbnail((cell - 6, cell - 6), Image.LANCZOS)
        W.paste(im, (60 + j * cell + 3, 30 + i * cell + 3))
        g.rectangle([60 + j * cell, 30 + i * cell, 60 + (j+1) * cell - 1, 30 + (i+1) * cell - 1], outline="#ccc")
W.save(out)
