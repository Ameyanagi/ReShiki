"""100% and 400% images from a 1200-dpi export, and a 400% crop at a joint.
usage: derive.py <svg> <export png> <out prefix> <joint x> <joint y> <atom id>"""
import re, sys
from PIL import Image
svg, png, out, jx, jy = sys.argv[1], sys.argv[2], sys.argv[3], float(sys.argv[4]), float(sys.argv[5])
lx, ly, w, h = map(float, re.search(r'viewBox="([^"]+)"', open(svg).read()).group(1).split())
im = Image.open(png).convert("RGB")
for zoom, tag in [(1, "100pct"), (4, "400pct")]:
    im.resize((round(w * zoom), round(h * zoom)), Image.LANCZOS).save(f"{out}-{tag}.png")
big = Image.open(f"{out}-400pct.png")
half = 80  # a 160 x 160 px window, 40 drawing units across
cx, cy = (jx - lx) * 4, (jy - ly) * 4
big.crop((round(cx - half), round(cy - half), round(cx + half), round(cy + half))).save(f"{out}-400pct-joint.png")
