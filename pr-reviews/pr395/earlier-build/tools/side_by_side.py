"""(Prototype side trims at a 40-level threshold so its #e6e9e8 dot grid is ignored; this can shave the palest H wire tips.)
Side-by-side: prototype (browser screenshot, model canvas cropped) | ReShiki export of the
same coordinates and view. Both are trimmed to their ink and scaled to the same height."""
import sys
from PIL import Image, ImageDraw, ImageChops
EV = sys.argv[1]
SRC = sys.argv[2] if len(sys.argv) > 2 else "proto"
MOLS = sys.argv[3].split(",") if len(sys.argv) > 3 else ["caffeine", "ibuprofen"]
TAG = "" if SRC == "proto" else "-" + SRC
CROP = (1022, 1000, 1528, 1506)  # inside the prototype's selection box, 2x device pixels
def flat(p):
    im = Image.open(p).convert("RGBA"); bg = Image.new("RGBA", im.size, "white"); bg.alpha_composite(im); return bg.convert("RGB")
def trim(im, pad=12, light=40):
    g = im.convert("L").point(lambda v: 255 if v < 215 else 0)
    # also count saturated pixels (light colours) via RGB distance from white
    d = ImageChops.difference(im, Image.new("RGB", im.size, "white")).convert("L").point(lambda v: 255 if v > light else 0)
    box = ImageChops.lighter(g, d).getbbox()
    x0, y0, x1, y1 = box
    return im.crop((max(0, x0 - pad), max(0, y0 - pad), min(im.width, x1 + pad), min(im.height, y1 + pad)))
H = 600
for mol in MOLS:
    for look in ["flat", "glossy"]:
        rows = []
        for style in ["ballstick", "stick", "wire", "space"]:
            a = trim(flat(f"{EV}/compare/proto-shots/{mol}-{style}-{look}.png").crop(CROP))
            b = trim(flat(f"{EV}/renders/{SRC}/{mol}-{style}-{look}-export-1200dpi.png"), light=8)
            a = a.resize((round(a.width * H / a.height), H), Image.LANCZOS)
            b = b.resize((round(b.width * H / b.height), H), Image.LANCZOS)
            W = Image.new("RGB", (a.width + b.width + 60, H + 40), "white"); g = ImageDraw.Draw(W)
            W.paste(a, (10, 30)); W.paste(b, (a.width + 50, 30))
            g.text((10, 8), f"prototype (browser) - {mol} {style} {look}", fill="black")
            g.text((a.width + 50, 8), "ReShiki branch export, same coordinates, " + ("ReShiki opening view" if SRC == "proto" else "rotation matched to the prototype projection"), fill="black")
            g.line([(a.width + 30, 30), (a.width + 30, H + 30)], fill="#bbb")
            W.save(f"{EV}/compare/sbs{TAG}-{mol}-{style}-{look}.png"); rows.append(W)
        Wd = max(r.width for r in rows); S = Image.new("RGB", (Wd, sum(r.height for r in rows)), "white")
        y = 0
        for r in rows: S.paste(r, (0, y)); y += r.height
        S.thumbnail((1800, 4000)); S.save(f"{EV}/compare/sheet{TAG}-{mol}-{look}.png")
