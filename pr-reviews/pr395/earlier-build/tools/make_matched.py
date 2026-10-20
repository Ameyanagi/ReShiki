"""ibuprofen 'matched view': the prototype's own matrix (from tools/proto_view.js) with its z row
negated, which is a proper rotation with exactly the prototype's screen x/y projection (depth order reversed)."""
import json, math, copy, sys, os
EV = sys.argv[1]
R = [[0.971,0.099,0.219],[-0.098,-0.669,0.737],[-0.219,0.737,0.639]]  # bun tools/proto_view.js, 3 dp
# re-orthonormalise (Gram-Schmidt) after rounding
def norm(v): n = math.sqrt(sum(x*x for x in v)); return [x/n for x in v]
x = norm(R[0]); y = R[1]; d = sum(a*b for a,b in zip(x,y)); y = norm([a-d*b for a,b in zip(y,x)])
z = [x[1]*y[2]-x[2]*y[1], x[2]*y[0]-x[0]*y[2], x[0]*y[1]-x[1]*y[0]]  # = -(prototype z row)
m = [x, y, z]; t = m[0][0]+m[1][1]+m[2][2]
if t > 0:
    s = math.sqrt(t+1)*2; q = [s/4,(m[2][1]-m[1][2])/s,(m[0][2]-m[2][0])/s,(m[1][0]-m[0][1])/s]
elif m[0][0] > m[1][1] and m[0][0] > m[2][2]:
    s = math.sqrt(1+m[0][0]-m[1][1]-m[2][2])*2; q = [(m[2][1]-m[1][2])/s, s/4, (m[0][1]+m[1][0])/s, (m[0][2]+m[2][0])/s]
elif m[1][1] > m[2][2]:
    s = math.sqrt(1+m[1][1]-m[0][0]-m[2][2])*2; q = [(m[0][2]-m[2][0])/s, (m[0][1]+m[1][0])/s, s/4, (m[1][2]+m[2][1])/s]
else:
    s = math.sqrt(1+m[2][2]-m[0][0]-m[1][1])*2; q = [(m[1][0]-m[0][1])/s, (m[0][2]+m[2][0])/s, (m[1][2]+m[2][1])/s, s/4]
os.makedirs(f"{EV}/input/proto-matched", exist_ok=True)
for f in os.listdir(f"{EV}/input/proto"):
    if f.startswith("ibuprofen"):
        d = json.load(open(f"{EV}/input/proto/{f}")); d["models"][0]["rotation"] = q
        json.dump(d, open(f"{EV}/input/proto-matched/{f}", "w"))
print(q)
