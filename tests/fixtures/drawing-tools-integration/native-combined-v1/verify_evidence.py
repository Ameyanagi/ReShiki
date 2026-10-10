#!/usr/bin/env python3
"""Portable byte/reference audit; no app, renderer, prediction or GUI execution."""

import argparse
import base64
import copy
import hashlib
import json
import math
import re
import struct
import sys
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parent
REFERENCE_SHA256 = "117c0323fc947cecf76e0311f7a4e65eb2e56ce62b6445f8c8a2f6f084b5409e"
COLLECTIONS = ("atoms", "bonds", "annotations", "arrows", "graphics", "groups", "depth_appearance")


def inside(relative):
    """All evidence reads are confined to this published packet."""
    assert isinstance(relative, str) and relative
    parts = relative.split("/")
    assert all(part not in ("", ".", "..") for part in parts), relative
    assert "\\" not in relative and ":" not in relative and "\x00" not in relative
    path = PurePosixPath(relative)
    assert not path.is_absolute(), relative
    resolved = (ROOT / path).resolve(strict=True)
    assert resolved.is_relative_to(ROOT), relative
    assert resolved.is_file(), relative
    return resolved


def strict_json(path):
    def pairs(items):
        result = {}
        for key, value in items:
            assert key not in result, (path.name, "duplicate JSON key", key)
            result[key] = value
        return result

    def invalid_constant(value):
        raise ValueError(f"Non-finite JSON value: {value}")

    return json.loads(path.read_bytes(), object_pairs_hook=pairs, parse_constant=invalid_constant)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def f32(value):
    return struct.pack("<f", value)


def equivalent(actual, expected, native=False):
    if isinstance(expected, dict):
        return (
            isinstance(actual, dict)
            and actual.keys() == expected.keys()
            and all(equivalent(actual[k], v, native) for k, v in expected.items())
        )
    if isinstance(expected, list):
        return (
            isinstance(actual, list)
            and len(actual) == len(expected)
            and all(equivalent(a, b, native) for a, b in zip(actual, expected))
        )
    if isinstance(expected, float) and native:
        return isinstance(actual, (float, int)) and f32(actual) == f32(expected)
    return actual == expected


def point(actual, original, delta, native=False):
    for axis in ("x", "y"):
        expected = original[axis] + delta[axis]
        assert math.isfinite(actual[axis])
        tolerance = 0.0004 if native else 1e-10
        assert abs(actual[axis] - expected) <= tolerance, (actual, original, delta)


def rounded(value):
    return struct.unpack("<f", f32(value))[0]


def loaded_arrow_normalization(actual, original, source):
    """Exact native load rebasing for this pinned Atom/LP-linked cubic only.

    No gap/clearance correction is admitted: these pinned anchors already
    satisfy clearance. Reconstruct source f32 center+offset and the matching
    adjacent-control transport; never derive expectations from actual edits.
    """
    reference = strict_json(inside(source["preserved_copy"]))
    delta = source["translation"]
    assert source["name"] == "methanol" and original["kind"] == "curved"
    assert len(actual["cubic"]) == len(original["cubic"]) == 2
    for index, end in enumerate(("start", "end")):
        field = end + "_anchor"
        target = original[field]["target"]
        assert target["kind"] in ("atom", "lone_pair")
        owner = next(a for a in reference["atoms"] if a["id"] == target["atom"])
        center = {axis: rounded(owner["position"][axis] + delta[axis]) for axis in ("x", "y")}
        if target["kind"] == "lone_pair":
            mark = next(m for m in owner["marks"] if m["id"] == target["mark"])
            assert mark["kind"] == "lone_pair"
            center = {
                axis: rounded(center[axis] + rounded(mark["offset"][axis])) for axis in center
            }
        loaded = {axis: rounded(original[end][axis] + delta[axis]) for axis in center}
        resolved = {
            axis: rounded(center[axis] + rounded(original[field]["offset"][axis]))
            for axis in center
        }
        movement = {axis: rounded(resolved[axis] - loaded[axis]) for axis in center}
        control = {
            axis: rounded(rounded(original["cubic"][index][axis] + delta[axis]) + movement[axis])
            for axis in center
        }
        for candidate, expected in ((actual[end], resolved), (actual["cubic"][index], control)):
            assert candidate.keys() == expected.keys()
            assert all(f32(candidate[axis]) == f32(expected[axis]) for axis in expected), (
                end,
                candidate,
                expected,
            )
        # Only the stored offset changes expectation. Targets, local mark IDs,
        # direction, gap and every other field retain the original strict audit.
        original[field]["offset"] = {
            axis: rounded(resolved[axis] - center[axis]) for axis in center
        }


def audit_object(kind, actual, original, source, native):
    actual = copy.deepcopy(actual)
    original = copy.deepcopy(original)
    if native and kind == "arrows":
        loaded_arrow_normalization(actual, original, source)
    mapping = {int(k): v for k, v in source["id_map"].items()}
    delta = source["translation"]
    if "id" in original:
        if kind == "bonds" and native:
            # Bond has endpoints, no model object ID. These legacy extras drop.
            original.pop("id")
            assert "id" not in actual
        else:
            assert actual.pop("id") == mapping[original.pop("id")]
    for field in ("position", "origin", "start", "end", "control"):
        if original.get(field) is not None:
            point(actual.pop(field), original.pop(field), delta, native)
    if original.get("cubic"):
        for a, b in zip(actual["cubic"], original["cubic"]):
            point(a, b, delta, native)
        assert len(actual.pop("cubic")) == len(original.pop("cubic")) == 2
    for field in ("a", "b"):
        if field in original:
            assert actual.pop(field) == mapping[original.pop(field)]
    for field in ("centroid", "members", "atoms", "stereo_atoms"):
        if field in original:
            assert actual.pop(field) == [mapping[v] if v else 0 for v in original.pop(field)]
    if original.get("stereo") and isinstance(original["stereo"], dict):
        original["stereo"]["neighbors"] = [
            mapping[v] if v else 0 for v in original["stereo"]["neighbors"]
        ]
    for field in ("weights", "overrides", "rear_weights", "rear_overrides"):
        if field in original:
            original[field] = {str(mapping[int(k)]): v for k, v in original[field].items()}
    for field in ("start_anchor", "end_anchor"):
        if original.get(field):
            target = original[field]["target"]
            for key in ("atom", "a", "b"):
                if key in target:
                    target[key] = mapping[target[key]]
            # The local mark ID and vector offset/direction are unchanged.
    if native:
        if kind == "atoms":
            defaults = {
                "depth": 0.0,
                "charge": 0,
                "radical_electrons": 0,
                "isotope": 0,
                "explicit_h": 0,
                "no_implicit": False,
                "aromatic": False,
                "stereo": None,
                "map_num": 0,
                "centroid": [],
                "attachment": None,
                "marks": [],
                "mark_serial": 0,
                "display": {},
                "text_style": None,
                "cip_label": None,
            }
            # H counts can be derived during normal load of the minimal NMR input.
            if "label_h" not in original:
                # The caller checks zero or the graph-derived H count first.
                defaults["label_h"] = actual.get("label_h", 0)
        elif kind == "bonds":
            defaults = {
                "z_order": 0,
                "projection": False,
                "ring_arc": False,
                "highlight": None,
                "indicator": {},
                "cip_label": None,
                "stereo": None,
                "stereo_atoms": [],
                "stereo_authoritative": False,
                "double_position": "auto",
                "secondary_display": None,
                "color": [0, 0, 0],
            }
        elif kind == "graphics":
            defaults = {"depth": [0.0, 0.0, 0.0], "arc": None, "path": []}
        elif kind == "arrows":
            defaults = {"control": None}
        elif kind == "depth_appearance":
            defaults = {"rear_overrides": {}, "rear_weights": {}, "overrides": {}}
        else:
            defaults = {}
        for field, value in defaults.items():
            if field not in original and field in actual:
                assert equivalent(actual.pop(field), value, True), (kind, field)
    assert equivalent(actual, original, native), (
        source["name"],
        kind,
        actual.keys(),
        original.keys(),
    )


def bbox(points):
    return [
        min(p["x"] for p in points),
        min(p["y"] for p in points),
        max(p["x"] for p in points),
        max(p["y"] for p in points),
    ]


def audit_reference(native_path=None):
    reference_bytes = inside("reference.json").read_bytes()
    assert digest(reference_bytes) == REFERENCE_SHA256, "portable reference metadata changed"
    metadata = strict_json(inside("reference.json"))
    authored_path = inside(metadata["fixture"]["path"])
    authored_bytes = authored_path.read_bytes()
    assert len(authored_bytes) == metadata["fixture"]["bytes"]
    assert digest(authored_bytes) == metadata["fixture"]["sha256"]
    path = inside(native_path) if native_path else authored_path
    document = strict_json(path)
    native = native_path is not None
    assert document["version"] == metadata["native_target_version"] == 24
    assert {k: len(document.get(k, [])) for k in COLLECTIONS} == metadata["expected_counts"]
    assert not any("nmr" in k.lower() or "camera" in k.lower() for k in document)
    authored = strict_json(authored_path)
    assert document["page_layout"] == authored["page_layout"]
    assert document["atom_labels"] == authored["atom_labels"]
    allowed_extras = {
        "drawing_style": json.loads(inside("origins/drawing_style.json").read_bytes()),
        "canvas_theme": "light",
        "color_theme": "publication",
        "custom_theme": None,
        "abbreviations": [],
        "ring_fills": [],
        "reactions": [],
        "recent_colors": [],
    }
    for key in set(document) - set(authored):
        assert key in allowed_extras and document[key] == allowed_extras[key], key
    ids = [
        o["id"]
        for k in ("atoms", "annotations", "arrows", "graphics", "groups")
        for o in document.get(k, [])
    ]
    assert len(ids) == len(set(ids)) == 70
    atoms = {a["id"]: a for a in document["atoms"]}
    assert all(b["a"] in atoms and b["b"] in atoms and b["a"] != b["b"] for b in document["bonds"])
    assert len({tuple(sorted((b["a"], b["b"]))) for b in document["bonds"]}) == 96
    cursor = {k: 0 for k in COLLECTIONS}
    component_bounds = {}
    components = []
    for source in metadata["sources"]:
        raw = inside(source["preserved_copy"]).read_bytes()
        assert len(raw) == source["bytes"] and digest(raw) == source["sha256"]
        reference = strict_json(inside(source["preserved_copy"]))
        assert reference["version"] == source["native_version"]
        mapping = {int(k): v for k, v in source["id_map"].items()}
        for collection in COLLECTIONS:
            originals = reference.get(collection, [])
            objects = document.get(collection, [])[
                cursor[collection] : cursor[collection] + len(originals)
            ]
            for obj, original in zip(objects, originals):
                if native and collection == "atoms" and "label_h" not in original:
                    valence = sum(
                        b["order"] for b in document["bonds"] if obj["id"] in (b["a"], b["b"])
                    )
                    expected_h = {"C": 4, "O": 2}[original["element"]] - valence
                    assert obj.get("label_h", 0) in (0, expected_h), (obj["id"], "computed H count")
                audit_object(collection, obj, original, source, native)
            cursor[collection] += len(originals)
        # Verify graph components/degree and every original internal XY distance.
        atom_ids = [mapping[a["id"]] for a in reference.get("atoms", [])]
        edges = [b for b in document["bonds"] if b["a"] in atom_ids]
        assert all(b["b"] in atom_ids for b in edges), "cross-component chemical edge"
        for edge in reference.get("bonds", []):
            old_a = next(a for a in reference["atoms"] if a["id"] == edge["a"])["position"]
            old_b = next(a for a in reference["atoms"] if a["id"] == edge["b"])["position"]
            new_a, new_b = (
                atoms[mapping[edge["a"]]]["position"],
                atoms[mapping[edge["b"]]]["position"],
            )
            old_length = math.hypot(old_a["x"] - old_b["x"], old_a["y"] - old_b["y"])
            new_length = math.hypot(new_a["x"] - new_b["x"], new_a["y"] - new_b["y"])
            assert abs(new_length - old_length) < (0.0005 if native else 1e-9)
        components.append(
            {
                "name": source["name"],
                "atoms": len(atom_ids),
                "bonds": len(edges),
                "source_version": reference["version"],
            }
        )
        if atom_ids:
            points = [atoms[x]["position"] for x in atom_ids]
            if source["name"] == "methanol":
                arrow = next(a for a in document["arrows"] if a["id"] == mapping[3])
                points += [arrow["start"], arrow["end"]] + arrow["cubic"]
                assert arrow["start_anchor"]["target"] == {
                    "kind": "lone_pair",
                    "atom": 3002,
                    "mark": 1,
                }
                assert arrow["end_anchor"]["target"] == {"kind": "atom", "atom": 3001}
                assert atoms[3002]["marks"][0]["id"] == 1
            component_bounds[source["name"]] = bbox(points)
    assert components[:3] == [
        {"name": "c60", "atoms": 60, "bonds": 90, "source_version": 22},
        {"name": "nmr", "atoms": 6, "bonds": 5, "source_version": 17},
        {"name": "methanol", "atoms": 2, "bonds": 1, "source_version": 21},
    ]
    assert all(
        sum(b["a"] == x or b["b"] == x for b in document["bonds"]) == 3 for x in range(1001, 1061)
    )
    scope = document["depth_appearance"][0]
    assert scope["atoms"] == list(range(1001, 1061)) and scope["rear_opacity"] == 0.25
    assert set(scope["weights"]) == {str(x) for x in range(1001, 1061)}
    picture = document["graphics"][0]
    assert picture["id"] == 4001 and picture["kind"] == "picture"
    frame = [
        picture["origin"],
        {
            "x": picture["origin"]["x"] + picture["axis_x"]["x"],
            "y": picture["origin"]["y"] + picture["axis_y"]["y"],
        },
    ]
    component_bounds["emf"] = bbox(frame)
    png = base64.b64decode(picture["picture"]["png"], validate=True)
    emf = base64.b64decode(picture["picture"]["emf"], validate=True)
    assert png[:8] == b"\x89PNG\r\n\x1a\n" and struct.unpack(">II", png[16:24]) == (4724, 2834)
    assert emf == inside("origins/source.emf").read_bytes()
    assert struct.unpack("<I", emf[:4])[0] == 1 and emf[40:44] == b" EMF"
    for extra in metadata["origin_extras"]:
        data = inside(extra["preserved_copy"]).read_bytes()
        assert len(data) == extra["bytes"] and digest(data) == extra["sha256"]
    # Static layout padding is conservative, not renderer ink-bound measurement.
    padding = {"c60": 20.0, "nmr": 40.0, "methanol": 40.0, "emf": 4.0}
    padded = {
        k: [v[0] - padding[k], v[1] - padding[k], v[2] + padding[k], v[3] + padding[k]]
        for k, v in component_bounds.items()
    }
    world_per_pt = 42.0 / 14.4
    content = [
        36.0 * world_per_pt,
        36.0 * world_per_pt,
        (500.0 - 36.0) * world_per_pt,
        (375.0 - 36.0) * world_per_pt,
    ]
    for box in padded.values():
        assert (
            box[0] >= content[0]
            and box[1] >= content[1]
            and box[2] <= content[2]
            and box[3] <= content[3]
        )
    for i, (name, box) in enumerate(padded.items()):
        for other, b in list(padded.items())[i + 1 :]:
            assert box[2] < b[0] or b[2] < box[0] or box[3] < b[1] or b[3] < box[1], (
                name,
                other,
                "layout overlap",
            )
    return {
        "status": "REFERENCE_STATIC_CHECK_COMPLETED",
        "mode": "native-file-reference-audit" if native else "authored-composition",
        "runtime_or_gui_validation": False,
        "path": path.relative_to(ROOT).as_posix(),
        "bytes": path.stat().st_size,
        "sha256": digest(path.read_bytes()),
        "counts": metadata["expected_counts"],
        "components": components,
        "component_point_frame_bounds": component_bounds,
        "conservative_padded_bounds": padded,
        "paper_content_bounds": content,
        "retained_png": {
            "bytes": len(png),
            "sha256": digest(png),
            "pixel_dimensions": [4724, 2834],
        },
        "retained_emf": {
            "bytes": len(emf),
            "sha256": digest(emf),
            "equals_preserved_raw_source": True,
        },
        "nmr_persistence": "no report/labels/plot/camera session fields authored or present",
        "limitations": "Static serialization/reference comparison only; no Rust validation, rendering, prediction or GUI executed by this checker.",
    }


def sha256_string(value):
    assert isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value), value
    return value


def hash_file(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def normalized(value):
    """Compare native float fields at their declared binary32 representation."""
    if isinstance(value, float):
        return rounded(value)
    if isinstance(value, dict):
        return {key: normalized(item) for key, item in value.items()}
    if isinstance(value, list):
        return [normalized(item) for item in value]
    return value


def pinned_atom(document, identifier):
    return next(atom for atom in document["atoms"] if atom["id"] == identifier)


def rebase_pinned_arrow(document):
    """Source-defined center+offset rebasing, only for this one pinned arrow."""
    assert len(document["arrows"]) == 1
    arrow = document["arrows"][0]
    assert arrow["id"] == 3003 and arrow["kind"] == "curved" and len(arrow["cubic"]) == 2
    assert arrow["start_anchor"]["target"] == {"kind": "lone_pair", "atom": 3002, "mark": 1}
    assert arrow["end_anchor"]["target"] == {"kind": "atom", "atom": 3001}
    for index, end in enumerate(("start", "end")):
        anchor = arrow[end + "_anchor"]
        owner = pinned_atom(document, anchor["target"]["atom"])
        center = copy.deepcopy(owner["position"])
        if end == "start":
            mark = next(mark for mark in owner["marks"] if mark["id"] == 1)
            assert mark["kind"] == "lone_pair"
            center = {axis: rounded(center[axis] + mark["offset"][axis]) for axis in center}
        point = {axis: rounded(center[axis] + anchor["offset"][axis]) for axis in center}
        movement = {axis: rounded(point[axis] - arrow[end][axis]) for axis in center}
        arrow["cubic"][index] = {
            axis: rounded(arrow["cubic"][index][axis] + movement[axis]) for axis in center
        }
        anchor["offset"] = {axis: rounded(point[axis] - center[axis]) for axis in center}
        arrow[end] = point


def audit_native_changes(restored_path, stretch_path, reopened_path):
    """Whole-document comparison against independent pinned inputs/command."""
    metadata = strict_json(inside("reference.json"))
    expected = normalized(strict_json(inside(metadata["fixture"]["path"])))
    # Exact ordinary defaults from the minimal historical v17 NMR graph.
    for atom in expected["atoms"]:
        if atom["id"] not in range(1, 7):
            continue
        valence = sum(b["order"] for b in expected["bonds"] if atom["id"] in (b["a"], b["b"]))
        defaults = {
            "aromatic": False,
            "charge": 0,
            "explicit_h": 0,
            "isotope": 0,
            "label_h": {"C": 4, "O": 2}[atom["element"]] - valence,
            "map_num": 0,
            "no_implicit": False,
            "radical_electrons": 0,
            "stereo": None,
        }
        for key, value in defaults.items():
            assert key not in atom
            atom[key] = value
    for bond in expected["bonds"]:
        if "id" in bond:
            assert bond.pop("id") in range(7, 12)
            for key, value in {
                "color": [0, 0, 0],
                "double_position": "auto",
                "stereo": None,
                "stereo_atoms": [],
                "z_order": 0,
            }.items():
                assert key not in bond
                bond[key] = value
    rebase_pinned_arrow(expected)
    restored = normalized(strict_json(inside(restored_path)))
    assert expected == restored, "Restored whole JSON changed beyond pinned load normalization"
    assert inside(restored_path).read_bytes() == inside(reopened_path).read_bytes(), (
        "Fresh-process reopened save is not byte-identical to restored"
    )
    stretched_expected = copy.deepcopy(expected)
    carbon = pinned_atom(stretched_expected, 3001)["position"]
    oxygen = pinned_atom(stretched_expected, 3002)["position"]
    x, y = oxygen["x"] - carbon["x"], oxygen["y"] - carbon["y"]
    length = math.hypot(x, y)  # Reference axis is computed in f64 by the source.
    ratio = rounded(rounded(14.4) / rounded(42.0))
    target = rounded(rounded(18.0) / ratio)
    difference = target - rounded(length)
    movement = {"x": rounded(difference * x / length), "y": rounded(difference * y / length)}
    pinned_atom(stretched_expected, 3002)["position"] = {
        axis: rounded(oxygen[axis] + movement[axis]) for axis in oxygen
    }
    rebase_pinned_arrow(stretched_expected)
    stretched = normalized(strict_json(inside(stretch_path)))
    assert stretched_expected == stretched, (
        "Stretch whole JSON changed beyond the source-derived 18pt branch/adjacent-control move"
    )
    moved = pinned_atom(stretched, 3002)["position"]
    return {
        "status": "PINNED_WHOLE_NATIVE_AND_STRETCH_STATIC_CHECK_COMPLETED",
        "restored_expected_binary32_equal": True,
        "fresh_reopened_byte_equal": True,
        "fixed_carbon_atom": 3001,
        "moving_oxygen_atom": 3002,
        "stretch_command_delta_f32": movement,
        "measured_stretch_length_pt": math.hypot(moved["x"] - carbon["x"], moved["y"] - carbon["y"])
        * ratio,
        "limits": "Finite reference/load/command arithmetic only; no Rust loader, renderer or GUI run.",
    }


def audit_packet(manifest_path):
    """Audit ROOT's frozen actual files; no template or authored-save relabel."""
    manifest = strict_json(inside(manifest_path))
    assert manifest["schema"] == "drawing-evidence-v2"
    assert manifest["status"] == "ACTUAL_FILES_FROZEN"
    entries = {}
    roles = (
        "native-restored",
        "native-stretch",
        "native-reopened",
        "raw-jpeg",
        "raw-ax",
        "raw-console",
        "raw-json",
        "qualified-metadata",
    )
    for entry in manifest["files"]:
        name = entry["path"]
        assert name not in entries, (name, "duplicate manifest path")
        path = inside(name)
        assert type(entry["bytes"]) is int and entry["bytes"] > 0
        assert path.stat().st_size == entry["bytes"], (name, "length changed")
        assert hash_file(path) == sha256_string(entry["sha256"]), (name, "bytes changed")
        role = entry["role"]
        assert role in roles, role
        if role == "raw-jpeg":
            with path.open("rb") as stream:
                assert stream.read(3) == b"\xff\xd8\xff", (name, "not JPEG magic")
        elif role in ("qualified-metadata", "raw-json"):
            strict_json(path)
        entries[name] = entry
    saved_paths = {
        label: manifest["native_" + key]
        for label, key in (
            ("restored", "restore"),
            ("stretch", "stretch"),
            ("reopened", "reopened"),
        )
    }
    assert len(set(saved_paths.values())) == 3
    for label, path in saved_paths.items():
        assert path in entries and entries[path]["role"] == "native-" + label
        assert sum(entry["role"] == "native-" + label for entry in entries.values()) == 1
    captures = {path for path, entry in entries.items() if entry["role"] == "raw-jpeg"}
    ax = {path for path, entry in entries.items() if entry["role"] == "raw-ax"}
    assert captures, "No actual raw capture recorded"
    artifact_path = manifest["correlation"]["artifact"]
    native_path = manifest["correlation"]["native"]
    assert artifact_path != native_path
    for name in (artifact_path, native_path):
        assert name in entries and entries[name]["role"] == "qualified-metadata"
    artifact = strict_json(inside(artifact_path))
    native = strict_json(inside(native_path))
    for record in (artifact, native):
        assert record["record_kind"] == "AUTHORED_SUMMARY_OF_FROZEN_ACTUAL_RECORDS"
        sha256_string(record["executable_sha256"])
        assert isinstance(record["bundle_id"], str) and record["bundle_id"]
        assert isinstance(record["basis_paths"], list) and record["basis_paths"]
        assert len(set(record["basis_paths"])) == len(record["basis_paths"])
        for basis in record["basis_paths"]:
            assert basis not in (artifact_path, native_path), "Summary cannot cite itself"
            assert basis in entries and entries[basis]["role"] == "qualified-metadata"
        assert re.fullmatch(r"[0-9a-f]{40}", record["source_commit"])
        assert re.fullmatch(r"[0-9a-f]{40}", record["source_tree"])
    sha256_string(artifact["runtime_inputs_sha256"])
    for field in ("executable_sha256", "bundle_id", "source_commit", "source_tree"):
        assert native[field] == artifact[field], (field, "producer mismatch")
    assert native["save_kind"] == "UNCHANGED_RESTORED_V24"
    for label, key in (("restored", "restore"), ("stretch", "stretch"), ("reopened", "reopened")):
        assert native["native_" + key + "_path"] == saved_paths[label]
        assert native["native_" + key + "_sha256"] == entries[saved_paths[label]]["sha256"]
    for field, names in (("capture_paths", captures), ("ax_paths", ax)):
        assert isinstance(native[field], list)
        assert len(native[field]) == len(set(native[field]))
        assert set(native[field]) == names, (field, "unbound or missing raw files")
    reference = strict_json(inside("reference.json"))
    assert saved_paths["restored"] != reference["fixture"]["path"]
    assert entries[saved_paths["restored"]]["sha256"] != reference["fixture"]["sha256"], (
        "Authored reference cannot be relabelled as a native save"
    )
    result = audit_reference(saved_paths["restored"])
    changes = audit_native_changes(
        saved_paths["restored"], saved_paths["stretch"], saved_paths["reopened"]
    )
    return {
        "status": "STATIC_PACKET_CHECK_COMPLETED",
        "runtime_or_gui_validation": False,
        "manifest": manifest_path,
        "manifest_sha256": hash_file(inside(manifest_path)),
        "hashed_actual_files": len(entries),
        "raw_jpeg_count": len(captures),
        "raw_ax_count": len(ax),
        "executable_sha256": artifact["executable_sha256"],
        "native_reference_audit": result,
        "whole_native_change_audit": changes,
        "limitations": "Checks bytes, pinned native semantics and declared correlation only. "
        "It does not execute the app, verify OS signatures or independently "
        "prove source/build/GUI attestations, pixel appearance, NMR predictions, "
        "public-loader behavior or performance.",
    }


if __name__ == "__main__":
    if sys.flags.optimize:
        raise SystemExit("Run without -O: this evidence checker requires assertions.")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--manifest",
        help="Packet-relative ROOT frozen actual-file manifest; omission checks authored reference only",
    )
    args = parser.parse_args()
    try:
        result = audit_packet(args.manifest) if args.manifest else audit_reference()
        print(json.dumps(result, indent=2, allow_nan=False))
    except (AssertionError, KeyError, TypeError, ValueError, OSError, StopIteration) as error:
        raise SystemExit(f"Evidence check failed: {error}") from error
