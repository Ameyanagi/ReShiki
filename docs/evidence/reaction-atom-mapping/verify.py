#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Verify the portable native evidence; stdlib only, no app or file mutation."""
import copy
import hashlib
import itertools
import json
import math
from pathlib import Path


class VerificationError(Exception):
    pass


def require(condition, message):
    if not condition:
        raise VerificationError(message)


DATA = Path(__file__).resolve().parent
REPO = DATA.parents[2]
NAMES = (
    "native-before.rsk", "native-custom-label-after.rsk",
    "authored-oxidation-after.rsk", "alignment-input.rsk",
    "alignment-native-after.rsk",
)
TOLERANCE = 1e-4  # Drawing units; saved coordinates use f32 precision.
STYLE = dict(family="Arial", size_pt=7.5, bold=False, italic=False,
             underline=False, color=[0, 0, 0], script="normal", formula=False)
DEFAULT_DISPLAY = dict(
    carbons=None, hydrogens=None, hydrogen_position="auto", number=None,
    mapping=dict(show=None, offset=None, style=STYLE),
    stereo=dict(show=None, offset=None, style={**STYLE, "italic": True}),
)
# Authored expectations: not derived from the candidate output or its manifest.
# id, element, explicit attached H, no implicit H, map, cached visible H.
METHANOL_ATOMS = (
    (1, "C", 3, True, 1, 3), (2, "O", 1, True, 2, 1),
    (3, "O", 0, False, 0, 2), (5, "C", 2, True, 1, 2),
    (6, "O", 0, True, 2, 0),
)
OXIDATION_ATOMS = (
    (1, "C", 3, True, 1, 3), (2, "C", 2, True, 2, 2),
    (3, "O", 1, True, 3, 1), (5, "C", 3, True, 1, 3),
    (6, "C", 1, True, 2, 1), (7, "O", 0, False, 3, 0),
)
METHANOL_POSITIONS = {
    1: (36.0, -1.5543122e-15), 2: (78.0, 1.5543122e-15),
    3: (226.0, -100.0), 5: (374.0, -1.5543122e-15),
    6: (416.0, 1.5543122e-15),
}
OXIDATION_POSITIONS = {
    1: (36.0, 10.5), 2: (72.37307, -10.5), 3: (108.74613, 10.5),
    5: (404.74612, 10.499997), 6: (441.1192, -10.5),
    7: (477.49228, 10.500003),
}


def atoms(doc):
    result = {a["id"]: a for a in doc["atoms"]}
    require(len(result) == len(doc["atoms"]), "Duplicate atom ID")
    return result


def xy(atom):
    return atom["position"]["x"], atom["position"]["y"]


def display(atom):
    result = copy.deepcopy(DEFAULT_DISPLAY)
    result.update(copy.deepcopy(atom.get("display", {})))
    return result


def bond(a, b, order):
    return dict(z_order=0, a=a, b=b, order=order, display="plain", stereo=None,
                stereo_atoms=[], double_position="auto", color=[0, 0, 0])


def reaction(reactants, products, agents):
    participants = lambda ids: [dict(atoms=ids, coefficient=1)] if ids else []
    return [dict(arrow=4, reactants=participants(reactants),
                 products=participants(products), agents=participants(agents),
                 annotations=[])]


def check_graph(doc, oxidation):
    require(set(doc) == {"atom_labels", "version", "atoms", "bonds",
                         "annotations", "arrows", "graphics", "groups", "reactions"},
            "Unexpected document metadata")
    expected = OXIDATION_ATOMS if oxidation else METHANOL_ATOMS
    actual = atoms(doc)
    require(list(actual) == [row[0] for row in expected], "Unexpected atoms/IDs")
    for atom_id, element, hydrogen, no_implicit, map_num, label_h in expected:
        wanted = dict(element=element, explicit_h=hydrogen,
                      no_implicit=no_implicit, map_num=map_num, label_h=label_h,
                      charge=0, isotope=0, radical_electrons=0,
                      aromatic=False, stereo=None)
        require(set(actual[atom_id]) - {"display"} == set(wanted) | {"id", "position"},
                "Unexpected atom metadata")
        for key, value in wanted.items():
            require(actual[atom_id].get(key) == value,
                    "Authored chemical expectation: atom %s %s" % (atom_id, key))
    expected_bonds = ([bond(1, 2, 1), bond(2, 3, 1), bond(5, 6, 1), bond(6, 7, 2)]
                      if oxidation else [bond(1, 2, 1), bond(5, 6, 2)])
    require(doc["bonds"] == expected_bonds, "Authored bond graph/order/display")
    expected_roles = (reaction([1, 2, 3], [5, 6, 7], []) if oxidation
                      else reaction([1, 2], [5, 6], [3]))
    require(doc["reactions"] == expected_roles, "Authored roles/maps/coefficients")
    ends = ((186.74614, 326.74612) if oxidation else (156.0, 296.0))
    require(doc["arrows"] == [dict(id=4, start=dict(x=ends[0], y=0.0),
                                   end=dict(x=ends[1], y=0.0), kind="forward")],
            "Authored arrow identity/geometry")
    for key in ("annotations", "graphics", "groups"):
        require(doc[key] == [], "Unexpected " + key)
    require(doc["atom_labels"] == dict(carbons="skeletal", hydrogens=True,
                                      stereo=False), "Unexpected label settings")


def without(doc, positions=False, displays=False, version=False):
    result = copy.deepcopy(doc)
    if version:
        result.pop("version", None)
    for atom in result["atoms"]:
        if positions:
            atom.pop("position", None)
        if displays:
            atom.pop("display", None)
    return result


def centroid(points):
    return tuple(sum(p[i] for p in points) / len(points) for i in (0, 1))


def oriented_area(points):
    a, b, c = points
    return (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])


def check_semantics(docs):
    before, custom, authored, rotated, aligned = (docs[name] for name in NAMES)
    for doc in (before, custom):
        check_graph(doc, False)
        require(doc["version"] == 19, "Native methanol version")
        for atom_id, atom in atoms(doc).items():
            require(xy(atom) == METHANOL_POSITIONS[atom_id],
                    "Methanol atom coordinates changed")
            expected = copy.deepcopy(DEFAULT_DISPLAY)
            if atom_id == 3:
                expected["hydrogen_position"] = "left"
            if doc is custom and atom_id == 2:
                expected["mapping"]["offset"] = dict(x=36.582657, y=-34.99306)
            require(display(atom) == expected, "Wrong custom label owner/style")
    require(without(before, displays=True) == without(custom, displays=True),
            "Custom label changed non-display document data")
    for doc in (authored, rotated, aligned):
        check_graph(doc, True)
        for atom_id, atom in atoms(doc).items():
            expected = copy.deepcopy(DEFAULT_DISPLAY)
            if atom_id == 1:
                expected["number"] = dict(text="Cα", offset=None, style=STYLE)
            require(display(atom) == expected, "Cα/custom number or map style changed")
    require(authored["version"] == rotated["version"] == 15,
            "Controlled authored input version")
    require(aligned["version"] == 19, "Native save version normalization")
    source, inp, out = (atoms(doc) for doc in (authored, rotated, aligned))
    for atom_id, position in OXIDATION_POSITIONS.items():
        require(xy(source[atom_id]) == position, "Authored oxidation coordinates")
    product_ids = [5, 6, 7]
    center = centroid([xy(source[i]) for i in product_ids])
    for atom_id in source:
        if atom_id in product_ids:
            x, y = xy(source[atom_id])
            expected = (center[0] - (y - center[1]), center[1] + (x - center[0]))
            require(math.dist(xy(inp[atom_id]), expected) < 1e-9,
                    "Controlled input is not the authored proper +90° rotation")
        else:
            require(source[atom_id] == inp[atom_id] == out[atom_id],
                    "Alignment changed a reactant or its custom number")
    require(without(authored, positions=True) == without(rotated, positions=True),
            "Controlled derivative changed chemical/display/role data")
    require(without(rotated, positions=True, version=True)
            == without(aligned, positions=True, version=True),
            "Alignment changed chemical/map/display/bond/role data")
    left = [xy(inp[i]) for i in product_ids]
    right = [xy(out[i]) for i in product_ids]
    require(oriented_area(left) * oriented_area(right) > 0,
            "Product reflection/orientation reversal")
    max_distance_delta = max(
        abs(math.dist(left[i], left[j]) - math.dist(right[i], right[j]))
        for i, j in itertools.combinations(range(3), 2)
    )
    require(max_distance_delta < TOLERANCE, "Product scale/bond distortion")
    pc, qc = centroid(left), centroid(right)
    u = [(x - pc[0], y - pc[1]) for x, y in left]
    v = [(x - qc[0], y - qc[1]) for x, y in right]
    angle = math.atan2(sum(x * Y - y * X for (x, y), (X, Y) in zip(u, v)),
                       sum(x * X + y * Y for (x, y), (X, Y) in zip(u, v)))
    c, s = math.cos(angle), math.sin(angle)
    residual = max(math.hypot(c * x - s * y - X, s * x + c * y - Y)
                   for (x, y), (X, Y) in zip(u, v))
    require(residual < TOLERANCE, "Whole product is not one rigid transform")
    require(abs(math.degrees(angle) + 90) < 1e-3, "Unexpected product rotation")
    lanes = [(out[i]["position"]["x"] - out[j]["position"]["x"],
              out[i]["position"]["y"] - out[j]["position"]["y"])
             for i, j in zip(product_ids, [1, 2, 3])]
    require(max(math.dist(lanes[0], lane) for lane in lanes) < TOLERANCE,
            "Mapped product vectors do not match reactant orientation")
    return dict(custom_label_owner="Mapping(2), reactant O",
                custom_label_offset=dict(x=36.582657, y=-34.99306),
                product_rotation_degrees=math.degrees(angle), determinant=c*c+s*s,
                max_rigid_fit_residual=residual,
                max_pair_distance_delta=max_distance_delta,
                native_save_version=[15, 19])


def negative_controls(docs):
    controls = []

    def reject(name, change):
        modified = copy.deepcopy(docs)
        change(modified)
        try:
            check_semantics(modified)
        except VerificationError as error:
            controls.append(dict(name=name, rejected=True, reason=str(error)))
        else:
            raise VerificationError("Negative control was accepted: " + name)

    def correlated_element(d):
        for name in NAMES[:2]:
            d[name]["atoms"][0]["element"] = "N"

    def correlated_bond(d):
        for name in NAMES[:2]:
            d[name]["bonds"][0]["order"] = 2

    def correlated_roles(d):
        for name in NAMES[:2]:
            d[name]["reactions"][0]["agents"] = []

    def wrong_owner(d):
        doc = d[NAMES[1]]
        doc["atoms"][0]["display"] = doc["atoms"][1].pop("display")

    def altered_number(d):
        for name in NAMES[2:]:
            d[name]["atoms"][0]["display"]["number"]["text"] = "Cβ"

    def reflection(d):
        doc = d[NAMES[4]]
        product = [a for a in doc["atoms"] if a["id"] in (5, 6, 7)]
        center = centroid([xy(a) for a in product])
        for atom in product:
            atom["position"]["x"] = 2 * center[0] - atom["position"]["x"]

    def scaling(d):
        product = [a for a in d[NAMES[4]]["atoms"] if a["id"] in (5, 6, 7)]
        center = centroid([xy(a) for a in product])
        for atom in product:
            for key, origin in zip(("x", "y"), center):
                atom["position"][key] = origin + 1.1 * (atom["position"][key] - origin)

    reject("correlated element corruption in both native files", correlated_element)
    reject("correlated bond-order corruption in both native files", correlated_bond)
    reject("correlated agent-role deletion in both native files", correlated_roles)
    reject("duplicate map2 assigned to carbon", lambda d: d[NAMES[1]]["atoms"][0].update(map_num=2))
    reject("custom map offset attached to wrong atom", wrong_owner)
    reject("Cα overwritten consistently in all alignment files", altered_number)
    reject("distance-preserving product reflection", reflection)
    reject("product uniform scaling", scaling)
    reject("one product atom distorted", lambda d: d[NAMES[4]]["atoms"][3]["position"].update(x=410.0))
    reject("reactant moved by alignment", lambda d: d[NAMES[4]]["atoms"][0]["position"].update(y=11.5))
    reject("wrong native-save version", lambda d: d[NAMES[4]].update(version=20))
    return controls


def main():
    provenance = json.loads((DATA / "provenance.json").read_text(encoding="utf-8"))
    checked = []
    for row in provenance["files"]:
        path = (REPO / row["path"]).resolve()
        require(REPO in path.parents, "Evidence path escapes checkout")
        raw = path.read_bytes()
        require(len(raw) == row["bytes"], "Changed evidence length: " + row["path"])
        require(hashlib.sha256(raw).hexdigest() == row["sha256"],
                "Changed original bytes: " + row["path"])
        checked.append(row["path"])
    require((DATA / "mapped-methanol-oxidation.rsmi").read_text(encoding="utf-8")
            == "[CH3:1][OH:2]>O>[CH2:1]=[O:2]\n", "Controlled concrete reaction SMILES")
    docs = {name: json.loads((DATA / name).read_text(encoding="utf-8")) for name in NAMES}
    result = dict(status="PASS", artifact_hashes_checked=len(checked),
                  semantic_results=check_semantics(docs),
                  negative_controls=negative_controls(docs),
                  verifier_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                  provenance_sha256=hashlib.sha256((DATA / "provenance.json").read_bytes()).hexdigest(),
                  scope="Artifact integrity/semantics only; does not launch or retest the mapper/GUI")
    print(json.dumps(result, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
