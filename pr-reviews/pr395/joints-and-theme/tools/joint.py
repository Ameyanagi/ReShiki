"""Print the drawing-unit position of a 3-bond ring atom joint of the model in an
.rsk file, by the projection of crates/model/src/models3d.rs (View::new/project).

The atom: a ring atom with three heavy neighbours, nearest the model centre.
usage: joint.py <file.rsk>  ->  "x y atom_id"
"""
import json, sys, math
doc = json.load(open(sys.argv[1]))
m = doc["models"][0]
atoms = {a["id"]: a for a in doc["atoms"]}
sym = lambda i: atoms[i]["element"]
STICK = 0.16
BALL = {"H": .23, "C": .33, "N": .32, "O": .31, "S": .40, "F": .30, "Cl": .40, "Br": .44, "P": .40}
rad = (lambda s: STICK) if m["style"] == "stick" else (lambda s: BALL.get(s, .35))
pts = [(sym(a["id"]), a["xyz"], a["id"]) for a in m["atoms"]] + [("H", h["xyz"], None) for h in m.get("hydrogens", [])]
heavy = [p for s, p, _ in pts if s != "H"] or [p for _, p, _ in pts]
c = [sum(p[k] for p in heavy) / len(heavy) for k in range(3)]
# All hydrogens shown in these inputs.
assert m.get("show_hydrogens", "all") == "all"
reach = max(math.dist(p, c) + rad(s) for s, p, _ in pts)
def rot(q, v):
    w, x, y, z = q
    d = x*v[0] + y*v[1] + z*v[2]
    cr = [y*v[2]-z*v[1], z*v[0]-x*v[2], x*v[1]-y*v[0]]
    uu = x*x + y*y + z*z
    return [2*d*u + (w*w - uu)*vv + 2*w*cc for u, vv, cc in zip([x, y, z], v, cr)]
ids = [a["id"] for a in m["atoms"]]
pos = [atoms[i]["position"] for i in ids]
xs = [p["x"] for p in pos]; ys = [p["y"] for p in pos]
origin = ((min(xs)+max(xs))/2 + m["offset"]["x"], (min(ys)+max(ys))/2 + m["offset"]["y"])
scale = 0.44 * m["size"] / reach
persp = m.get("perspective", 0.2)
camera = reach * (1.2 + 9 * (1 - persp))
def project(p):
    r = rot(m["rotation"], [p[k] - c[k] for k in range(3)])
    f = camera / (camera - r[2]) if persp > 0 else 1
    return origin[0] + r[0]*scale*f, origin[1] - r[1]*scale*f
idset = set(ids)
bonds = [(b["a"], b["b"]) for b in doc["bonds"] if b["a"] in idset and b["b"] in idset]
nb = {i: set() for i in ids}
for a, b in bonds: nb[a].add(b); nb[b].add(a)
def in_ring(a, b):
    seen, todo = {a}, [a]
    while todo:
        x = todo.pop()
        for y in nb[x]:
            if (x, y) in ((a, b), (b, a)) or y in seen: continue
            if y == b: return True
            seen.add(y); todo.append(y)
    return False
ring = {i for a, b in bonds if in_ring(a, b) for i in (a, b)}
cands = [i for i in ids if i in ring and len([n for n in nb[i] if sym(n) != "H"]) == 3]
xyz = {a["id"]: a["xyz"] for a in m["atoms"]}
best = min(cands, key=lambda i: math.dist(project(xyz[i]), origin))
x, y = project(xyz[best])
print(f"{x} {y} {best}")
