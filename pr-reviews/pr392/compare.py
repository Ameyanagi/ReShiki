"""Measure circled-charge geometry in the exported SVGs and build side-by-side PNGs.

Run: uv run --with pillow python -I compare.py <ev388 dir>
"""
import re
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

E = Path(sys.argv[1])
NAMES = ["methoxide", "methylpyridinium", "ammonium-chemdraw", "app-drawn"]


def svg_geometry(path: Path):
    s = path.read_text()
    # Label font is 10 pt in every input, so units-per-pt = font-size / 10.
    unit = float(re.search(r'font-size="([\d.]+)"', s).group(1)) / 10
    out = {}
    for d in re.findall(r'<path d="([^"]+)"', s):
        nums = [float(v) for v in re.findall(r"-?[\d.]+", d)]
        xs, ys = nums[0::2], nums[1::2]
        w, h = max(xs) - min(xs), max(ys) - min(ys)
        if "C" in d:
            out["circle_diameter_pt"] = round(w / unit, 3)
        elif len(xs) <= 4 and max(w, h) / unit < 10:
            out["sign_bar_pt"] = round(max(w, h) / unit, 3)
    return out


def font(size):
    for p in ["/System/Library/Fonts/Helvetica.ttc", "/System/Library/Fonts/Supplemental/Arial.ttf"]:
        try:
            return ImageFont.truetype(p, size)
        except OSError:
            pass
    return ImageFont.load_default()


def on_white(img):
    bg = Image.new("RGB", img.size, "white")
    bg.paste(img, mask=img.convert("RGBA").split()[3])
    return bg


def pair(left, right, title, scale_note, out):
    pad, f = 24, font(16)
    tb, ta = "BEFORE  origin/main 036f0c82", "AFTER  fix/smaller-circled-charge a723b234"
    probe = ImageDraw.Draw(Image.new("RGB", (1, 1)))
    tw = lambda t: probe.textbbox((0, 0), t, font=f)[2]
    col = max(left.width, right.width, tw(tb), tw(ta))
    head = 64
    w = col * 2 + pad * 3
    w = max(w, tw(f"{title} - {scale_note}") + pad * 2)
    h = max(left.height, right.height) + head + pad
    canvas = Image.new("RGB", (w, h), "white")
    d = ImageDraw.Draw(canvas)
    d.text((pad, 8), f"{title} - {scale_note}", fill="black", font=f)
    d.text((pad, 36), tb, fill=(160, 0, 0), font=f)
    d.text((col + pad * 2, 36), ta, fill=(0, 110, 0), font=f)
    # Align both renders on their bottom-left corner so the molecules line up.
    base = head + max(left.height, right.height)
    canvas.paste(left, (pad, base - left.height))
    canvas.paste(right, (col + pad * 2, base - right.height))
    d.line([(col + pad * 1.5, head), (col + pad * 1.5, h - 8)], fill=(200, 200, 200), width=2)
    canvas.save(out)


rows = []
sbs = E / "side-by-side"
sbs.mkdir(exist_ok=True)
for n in NAMES:
    b, a = svg_geometry(E / "before" / f"{n}.svg"), svg_geometry(E / "after" / f"{n}.svg")
    rows.append((n, b, a))
    big_b = on_white(Image.open(E / "before" / f"{n}-1200dpi.png"))
    big_a = on_white(Image.open(E / "after" / f"{n}-1200dpi.png"))
    # 100%: 1 pt = 1 px (72 dpi), resampled from the 1200 dpi export.
    k = 72 / 1200
    s_b = big_b.resize((round(big_b.width * k), round(big_b.height * k)), Image.LANCZOS)
    s_a = big_a.resize((round(big_a.width * k), round(big_a.height * k)), Image.LANCZOS)
    s_b.save(E / "before" / f"{n}-100pct.png")
    s_a.save(E / "after" / f"{n}-100pct.png")
    pair(s_b, s_a, n, "100% (72 dpi, 1 pt = 1 px)", sbs / f"{n}-100pct.png")
    # 4x view of the 100% image (nearest neighbour) so the 100% pixels are visible.
    s4b = s_b.resize((s_b.width * 4, s_b.height * 4), Image.NEAREST)
    s4a = s_a.resize((s_a.width * 4, s_a.height * 4), Image.NEAREST)
    pair(s4b, s4a, n, "100% pixels shown at 4x", sbs / f"{n}-100pct-x4.png")
    # Enlarged: the 1200 dpi export scaled to 400% of 72 dpi (288 dpi).
    k = 288 / 1200
    e_b = big_b.resize((round(big_b.width * k), round(big_b.height * k)), Image.LANCZOS)
    e_a = big_a.resize((round(big_a.width * k), round(big_a.height * k)), Image.LANCZOS)
    pair(e_b, e_a, n, "enlarged 400% (288 dpi from the 1200 dpi export)", sbs / f"{n}-400pct.png")

# One combined sheet of the 400% pairs.
imgs = [Image.open(sbs / f"{n}-400pct.png") for n in NAMES]
sheet = Image.new("RGB", (max(i.width for i in imgs), sum(i.height for i in imgs)), "white")
y = 0
for i in imgs:
    sheet.paste(i, (0, y))
    y += i.height
sheet.save(sbs / "all-400pct.png")

with open(E / "measurements.txt", "w") as f:
    f.write("Circled-charge geometry measured from the exported SVG paths (stored size_pt = 7.5 in the CDXML inputs; app-drawn stores none and uses 0.75 x 10 pt = 7.5)\n")
    f.write(f"{'drawing':20} {'circle diameter pt (before -> after)':38} {'sign bar pt (before -> after)'}\n")
    for n, b, a in rows:
        f.write(
            f"{n:20} {b.get('circle_diameter_pt')} -> {a.get('circle_diameter_pt')}{'':22}"
            f"{b.get('sign_bar_pt')} -> {a.get('sign_bar_pt')}\n"
        )
    f.write("Expected: before 7.8 (0.52*2*7.5) / 4.5 (0.3*2*7.5); after 6.0 (0.40*2*7.5) / 3.6 (0.24*2*7.5)\n")
print((E / "measurements.txt").read_text())
