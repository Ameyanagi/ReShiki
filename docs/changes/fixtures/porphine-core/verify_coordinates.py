"""Audit actual desktop native saves using only Python's standard library.

Run from any directory: python3 verify_coordinates.py
Chemical identifiers are independently recorded in identity-oracles.json.
This script imports no application implementation and does not invoke a GUI.
"""

import hashlib
import json
import math
from pathlib import Path

ROOT = Path(__file__).parent
TOLERANCE = 0.00003  # f32 coordinates and shortest-decimal JSON, world units


def load(name):
    return json.loads((ROOT / name).read_text())


def points(doc):
    return {a["id"]: (a["position"]["x"], a["position"]["y"]) for a in doc["atoms"]}


def length(doc, start, end):
    p = points(doc)
    return math.dist(p[start], p[end])


def chemical_atom(atom):
    return {k: v for k, v in atom.items() if k not in ("position", "label_h", "cip_label")}


def semantic_equal(before, after):
    assert len(before["atoms"]) == len(after["atoms"])
    assert [chemical_atom(a) for a in before["atoms"]] == [chemical_atom(a) for a in after["atoms"]]
    assert before["bonds"] == after["bonds"]
    assert {k: v for k, v in before.items() if k != "atoms"} == {
        k: v for k, v in after.items() if k != "atoms"
    }


def exact_pair(first, second):
    assert load(first) == load(second), (first, second)
    return {"before": first, "after": second, "all_native_fields_exact": True}


def nh_sites(doc):
    sites = []
    for atom in doc["atoms"]:
        if atom["element"] != "N":
            continue
        valence = sum(b["order"] for b in doc["bonds"] if atom["id"] in (b["a"], b["b"]))
        assert valence in (2, 3)
        assert atom["label_h"] == (1 if valence == 2 else 0)
        if valence == 2:
            sites.append(atom["id"])
    assert len(sites) == 2
    return sites


def core_metrics(doc):
    assert len(doc["atoms"]) == 24 and len(doc["bonds"]) == 28
    assert sum(a["element"] == "C" for a in doc["atoms"]) == 20
    assert sum(b["order"] == 2 for b in doc["bonds"]) == 11
    lengths = [length(doc, b["a"], b["b"]) for b in doc["bonds"]]
    assert max(abs(n - 42) for n in lengths) < TOLERANCE
    sites = nh_sites(doc)
    p = points(doc)
    center = tuple(sum(v[k] for v in p.values()) / len(p) for k in (0, 1))
    opposite_error = math.dist(tuple((p[sites[0]][k] + p[sites[1]][k]) / 2 for k in (0, 1)), center)
    assert opposite_error < TOLERANCE
    return {
        "atoms": 24,
        "bonds": 28,
        "double_bonds": 11,
        "nh_ids": sites,
        "bond_min_world": min(lengths),
        "bond_max_world": max(lengths),
        "nh_midpoint_to_center_world": opposite_error,
    }


def audit():
    report = {
        "method": "Independent native JSON arithmetic; no implementation imports",
        "coordinate_tolerance_world": TOLERANCE,
        "exact_restoration": [],
    }
    for before, after in [
        ("porphine-placed-desktop.rsk", "porphine-placed-redo.rsk"),
        ("porphine-placed-desktop.rsk", "porphine-placed-fresh-reopened-desktop.rsk"),
        ("source-phenyl.rsk", "porphine-phenyl-stretch-undo.rsk"),
        ("porphine-phenyl-stretched-desktop.rsk", "porphine-phenyl-stretch-redo.rsk"),
        (
            "porphine-phenyl-stretched-desktop.rsk",
            "porphine-phenyl-stretch-fresh-reopened-desktop.rsk",
        ),
        ("porphine-phenyl-stretched-desktop.rsk", "porphine-ring-stretch-refused-desktop.rsk"),
        (
            "porphine-manually-constructed-inward-labels-desktop.rsk",
            "porphine-manual-inward-fresh-reopened-desktop.rsk",
        ),
    ]:
        report["exact_restoration"].append(exact_pair(before, after))
    empty = load("porphine-placed-undo.rsk")
    assert not empty["atoms"] and not empty["bonds"]
    report["placement_undo_empty"] = True
    report["direct_core"] = core_metrics(load("porphine-placed-desktop.rsk"))
    report["manual_core"] = core_metrics(load("porphine-manually-constructed-desktop.rsk"))

    seed = load("source-seed.rsk")
    arm = load("porphine-seed-arm-length-desktop.rsk")
    aligned = load("porphine-seed-aligned-desktop.rsk")
    expected = load("source-aligned-seed.rsk")
    semantic_equal(seed, arm)
    semantic_equal(arm, aligned)
    assert points(aligned) == points(expected)
    p, q = points(seed), points(arm)
    assert {i for i in p if p[i] != q[i]} == {5}
    assert abs(length(arm, 4, 5) - 42) < TOLERANCE
    dx, dy = p[5][0] - p[4][0], p[5][1] - p[4][1]
    ex, ey = q[5][0] - q[4][0], q[5][1] - q[4][1]
    axis_error = abs(dx * ey - dy * ex) / math.hypot(dx, dy)
    assert axis_error < TOLERANCE
    angle = math.degrees(math.atan2(p[3][1] - p[2][1], p[3][0] - p[2][0]))
    assert abs(points(aligned)[3][1] - points(aligned)[2][1]) < TOLERANCE
    report["seed_alignment"] = {
        "original_edge_degrees": angle,
        "arm_before_pt": length(seed, 4, 5) * 14.4 / 42,
        "arm_after_pt": length(arm, 4, 5) * 14.4 / 42,
        "arm_only_moved_id": 5,
        "arm_axis_error_world": axis_error,
        "aligned_coordinates_exactly_match_renderer_fixture": True,
        "pinned_point_world": [0, 0],
        "label_caches_hydrated": [a["label_h"] for a in aligned["atoms"]],
    }
    seed_atoms = aligned["atoms"]
    copy_errors = []
    for name, count in [
        ("porphine-seed-copy-90-desktop.rsk", 2),
        ("porphine-four-seeds-desktop.rsk", 4),
    ]:
        doc = load(name)
        assert len(doc["atoms"]) == len(doc["bonds"]) == 6 * count
        for quarter in range(count):
            for original, copied in zip(seed_atoms, doc["atoms"][quarter * 6 : (quarter + 1) * 6]):
                x, y = original["position"]["x"], original["position"]["y"]
                for _ in range(quarter):
                    x, y = -y, x
                error = math.dist((x, y), (copied["position"]["x"], copied["position"]["y"]))
                assert error < TOLERANCE
                copy_errors.append(error)
    report["quarter_copies"] = {
        "successive_turns_degrees": [90, 90, 90],
        "shared_pin_world": [0, 0],
        "maximum_coordinate_residual_world": max(copy_errors),
    }

    four = load("porphine-four-seeds-desktop.rsk")
    closed = load("porphine-closed-carbon-scaffold-desktop.rsk")
    assert points(four) == points(closed)
    links = [b for b in closed["bonds"] if b not in four["bonds"]]
    assert {(b["a"], b["b"]) for b in links} == {(5, 22), (26, 28), (32, 34), (38, 1)}
    assert len(links) == 4 and all(b["order"] == 1 for b in links)
    report["closing_links"] = [[b["a"], b["b"]] for b in links]
    manual = load("porphine-manually-constructed-desktop.rsk")
    inward = load("porphine-manually-constructed-inward-labels-desktop.rsk")
    assert points(closed) == points(manual) == points(inward)
    assert manual["bonds"] == inward["bonds"]
    changed = [(a, b) for a, b in zip(manual["atoms"], inward["atoms"]) if a != b]
    assert len(changed) == 1 and changed[0][0]["id"] == 33
    before, after = changed[0]
    assert (
        before["display"]["hydrogen_position"] == "below"
        and after["display"]["hydrogen_position"] == "above"
    )
    normalized = json.loads(json.dumps(before))
    normalized["display"]["hydrogen_position"] = "above"
    assert normalized == after
    assert {k: v for k, v in manual.items() if k != "atoms"} == {
        k: v for k, v in inward.items() if k != "atoms"
    }
    report["manual_label_adjustment"] = {
        "atom_id": 33,
        "only_change": "display.hydrogen_position: below → above",
    }

    before = load("source-phenyl.rsk")
    after = load("porphine-phenyl-stretched-desktop.rsk")
    semantic_equal(before, after)
    a, b = points(before), points(after)
    assert [x for x in before["atoms"] if x["id"] <= 24] == [
        x for x in after["atoms"] if x["id"] <= 24
    ]
    moving = [i for i in a if a[i] != b[i]]
    assert moving == [25, 26, 27, 28, 29, 30]
    translation = tuple(b[25][k] - a[25][k] for k in (0, 1))
    residual = max(math.dist(tuple(b[i][k] - a[i][k] for k in (0, 1)), translation) for i in moving)
    assert residual < TOLERANCE
    v = tuple(a[25][k] - a[5][k] for k in (0, 1))
    axis_error = abs(v[0] * translation[1] - v[1] * translation[0]) / math.hypot(*v)
    assert axis_error < TOLERANCE
    report["phenyl_drag"] = {
        "fixed_atom_id": 5,
        "moving_atom_ids": moving,
        "translation_world": translation,
        "rigid_residual_world": residual,
        "axis_error_world": axis_error,
        "before_pt": length(before, 5, 25) * 14.4 / 42,
        "after_pt": length(after, 5, 25) * 14.4 / 42,
        "core_atoms_and_all_bonds_exact": True,
    }
    report["file_sha256"] = {
        p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(ROOT.glob("*.rsk"))
    }
    return report


if __name__ == "__main__":
    print(json.dumps(audit(), indent=2, ensure_ascii=False))
