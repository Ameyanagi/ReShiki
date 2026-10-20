"""Make the .rsk inputs for the renders.

own/   : the model ReShiki generated (MMFF94s, from the p2 evidence test), display 3D,
         one file per style x look.
proto/ : the same drawing, with the model's coordinates replaced by the prototype's
         (mols.json, RDKit MMFF94s seed 7) and the rotation set to ReShiki's
         initial_rotation of those coordinates, so both renderers show the same view.
"""
import json, math, sys, copy, os
SP, EV = sys.argv[1], sys.argv[2]
STYLES = {"ballstick": "ball_stick", "stick": "stick", "wire": "wireframe", "space": "space_filling"}
LOOKS = ["flat", "glossy"]
P = json.load(open(f"{SP}/model3d/mols.json"))

def initial_rotation(pts):
    dot = lambda a, b: sum(x * y for x, y in zip(a, b))
    c = [[sum(p[i] * p[j] for p in pts) for j in range(3)] for i in range(3)]
    def principal(v, prev):
        for _ in range(60):
            v = [dot(r, v) for r in c]
            if prev:
                d = dot(v, prev); v = [x - d * a for x, a in zip(v, prev)]
            n = math.sqrt(dot(v, v)); v = [x / n for x in v]
        return v
    x = principal([1, .3, .2], None)
    y = [-v for v in principal([.2, 1, .3], x)]
    z = [x[1]*y[2]-x[2]*y[1], x[2]*y[0]-x[0]*y[2], x[0]*y[1]-x[1]*y[0]]
    B = [x, y, z]
    sx, cx = math.sin(-.35), math.cos(-.35); sy, cy = math.sin(.3), math.cos(.3)
    rx = [[1,0,0],[0,cx,-sx],[0,sx,cx]]; ry = [[cy,0,sy],[0,1,0],[-sy,0,cy]]
    mul = lambda A, Bm: [[sum(A[i][k]*Bm[k][j] for k in range(3)) for j in range(3)] for i in range(3)]
    m = mul(mul(rx, ry), B)
    t = m[0][0] + m[1][1] + m[2][2]
    if t > 0:
        s = math.sqrt(t + 1) * 2; q = [s/4, (m[2][1]-m[1][2])/s, (m[0][2]-m[2][0])/s, (m[1][0]-m[0][1])/s]
    elif m[0][0] > m[1][1] and m[0][0] > m[2][2]:
        s = math.sqrt(1+m[0][0]-m[1][1]-m[2][2])*2; q = [(m[2][1]-m[1][2])/s, s/4, (m[0][1]+m[1][0])/s, (m[0][2]+m[2][0])/s]
    elif m[1][1] > m[2][2]:
        s = math.sqrt(1+m[1][1]-m[0][0]-m[2][2])*2; q = [(m[0][2]-m[2][0])/s, (m[0][1]+m[1][0])/s, s/4, (m[1][2]+m[2][1])/s]
    else:
        s = math.sqrt(1+m[2][2]-m[0][0]-m[1][1])*2; q = [(m[1][0]-m[0][1])/s, (m[0][2]+m[2][0])/s, (m[1][2]+m[2][1])/s, s/4]
    n = math.sqrt(sum(v*v for v in q)); return [v/n for v in q]

for mol in ["caffeine", "ibuprofen"]:
    base = json.load(open(f"{SP}/p2/ev/{mol}-ballstick.rsk"))
    json.dump(base, open(f"{EV}/input/{mol}-base.rsk", "w"))
    # prototype coordinates
    pa = P[mol]["atoms"]; heavy = [i for i, a in enumerate(pa) if a["el"] != "H"]
    parent = {}
    for a, b, _ in P[mol]["bonds"]:
        if pa[a]["el"] == "H": parent[a] = b
        if pa[b]["el"] == "H": parent[b] = a
    proto = copy.deepcopy(base); m = proto["models"][0]
    ids = [a["id"] for a in m["atoms"]]
    for k, a in enumerate(m["atoms"]): a["xyz"] = pa[heavy[k]]["xyz"]
    hs = [i for i, a in enumerate(pa) if a["el"] == "H"]
    m["hydrogens"] = [{"parent": ids[parent[i]], "xyz": pa[i]["xyz"]} for i in hs]
    hx = [pa[i]["xyz"] for i in heavy]; c = [sum(p[k] for p in hx)/len(hx) for k in range(3)]
    m["rotation"] = initial_rotation([[p[k]-c[k] for k in range(3)] for p in hx])
    for tag, doc in [("own", base), ("proto", proto)]:
        os.makedirs(f"{EV}/input/{tag}", exist_ok=True)
        for s, sv in STYLES.items():
            for look in LOOKS:
                d = copy.deepcopy(doc); mm = d["models"][0]
                mm.update(display="3d" if False else "three_d", style=sv, look=look, offset={"x": 0.0, "y": 0.0})
                json.dump(d, open(f"{EV}/input/{tag}/{mol}-{s}-{look}.rsk", "w"))
print("ok")
