"""Contact sheet for one molecule: a row per style x canvas x theme; columns are
before and after at 100%, then before and after 400% joint crops (shown 2x,
nearest neighbour). Missing 'before' cells are left blank.
usage: sheet.py <ev dir> <mol> <out.png> [after-only]"""
import sys, os
from PIL import Image, ImageDraw
ev, mol, out = sys.argv[1], sys.argv[2], sys.argv[3]
after_only = len(sys.argv) > 4
rows = [(s, c, t) for s in ["stick", "ballstick"] for c in ["light", "dark"] for t in ["publication", "jmol"]]
cols = ([] if after_only else [("before", "100pct")]) + [("after", "100pct")] + \
       ([] if after_only else [("before", "400pct-joint")]) + [("after", "400pct-joint")]
CW, CH, LW = 340, 330, 150
W = Image.new("RGB", (LW + CW * len(cols), 26 + CH * len(rows)), "white"); g = ImageDraw.Draw(W)
for j, (who, kind) in enumerate(cols):
    g.text((LW + j * CW + 6, 7), f"{who} {kind.replace('pct', '%').replace('-joint', ' joint x2')}", fill="black")
for i, (s, c, t) in enumerate(rows):
    y = 26 + i * CH
    g.text((6, y + CH // 2 - 10), f"{s}\n{c} canvas\n{t}", fill="black")
    for j, (who, kind) in enumerate(cols):
        x = LW + j * CW
        g.rectangle([x, y, x + CW - 1, y + CH - 1], outline="#bbb")
        p = f"{ev}/{who}/{mol}-{s}-{c}-{t}-{kind}.png"
        if not os.path.exists(p): continue
        im = Image.open(p).convert("RGB")
        if kind.endswith("joint"): im = im.resize((im.width * 2, im.height * 2), Image.NEAREST)
        im.thumbnail((CW - 8, CH - 8), Image.LANCZOS)
        W.paste(im, (x + 4, y + 4))
W.save(out)
