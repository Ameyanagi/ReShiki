#!/usr/bin/env python3
"""Check this finite saved-data packet; never run the app, parser, or GUI.

The S reference C[C@H](O)C(=O)O is independently specified by ChEBI:422.
Native stereo/H display caches are ignored. This is not a general CIP engine.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from collections import Counter
from copy import deepcopy
from pathlib import Path

HERE = Path(__file__).resolve().parent
ALIAS = "(+)-lactic acid"
SYSTEMATIC = "(2S)-2-hydroxypropanoic acid"
REFERENCE = "C[C@H](O)C(=O)O"
TREE = "bfbce003804680bd18aa39048490b035a09975dc"
RAW_GUI = "410bbb3dce4e8f2ada613d2d157389e2f78e1f7b599c4e249fe6fe2b0d7e4d3e"
SIGNED_GUI = "8d96eff1c44f124b2364da68a39e9808c5dc1889066dc980249fb88eba79477d"
HELPER = "9e770329748a6eab06615a600fca3b306be04b31b562f8440542d7af3cb4618a"
NATIVE_HASHES = {
    "plus-lactic-final.rsk": "93de1c1af124f6e2a881b65050e97876beb4bcdfa7bd0cfb666f3ad672013a87",
    "plus-lactic-generated-final.rsk": "9ee17d469528cb8ae238f7243e8ac0c0e34aaa8ec4597854941969585b0a67b1",
    "reaction-map-labels-final.rsk": "81665fa6645545a524386ae515dd4c03e1cce290a18bcf6ce8711a3deba0c0e0",
    "reaction-map-manual27-final.rsk": "09a4b4721850fcaf4c9624eb1f9e9dc476f89884d3e1b3fb8fb36c67883dbf7b",
    "reaction-map-undo-final.rsk": "81665fa6645545a524386ae515dd4c03e1cce290a18bcf6ce8711a3deba0c0e0",
}


def need(condition, message):
    if not condition:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def inside(relative):
    path = Path(relative)
    need(not path.is_absolute(), "Absolute packet path")
    resolved = (HERE / path).resolve()
    need(resolved.is_relative_to(HERE), "Packet path escapes its directory")
    return resolved


def exact_bytes(data, expected, label):
    need(digest(data) == expected, f"Changed original bytes: {label}")


def native(document, caption):
    need(document.get("version") == 25, "Changed authentic native version")
    rows = document["atoms"]
    atoms = {atom["id"]: atom for atom in rows}
    need(len(rows) == len(atoms) == 6, "Not six unique atoms")
    need(all(type(i) is int and i > 0 for i in atoms), "Invalid atom ID")
    bonds = document["bonds"]
    need(len(bonds) == 5, "Not five bonds")
    edges = {i: [] for i in atoms}
    seen = set()
    for bond in bonds:
        a, b, order = bond["a"], bond["b"], bond["order"]
        need(a in atoms and b in atoms and a != b, "Invalid bond endpoints")
        need(order in (1, 2), "Unexpected bond order")
        pair = frozenset((a, b))
        need(pair not in seen, "Duplicate bond")
        seen.add(pair)
        edges[a].append((b, order))
        edges[b].append((a, order))
    need(Counter(a["element"] for a in rows) == {"C": 3, "O": 3}, "Not C3O3")
    hydrogen = 0
    for i, atom in atoms.items():
        need(
            not any(atom.get(k, 0) for k in ("charge", "isotope", "radical_electrons", "aromatic")),
            "Changed charge/isotope/radical/aromatic state",
        )
        explicit = atom.get("explicit_h", 0)
        need(type(explicit) is int and explicit >= 0, "Invalid explicit hydrogen")
        available = {"C": 4, "O": 2}[atom["element"]] - sum(n for _, n in edges[i]) - explicit
        need(available >= 0, "Invalid valence")
        hydrogen += explicit + (0 if atom.get("no_implicit", False) else available)
    need(hydrogen == 6, "Not C3H6O3")
    methyls = [i for i in atoms if atoms[i]["element"] == "C" and len(edges[i]) == 1]
    need(len(methyls) == 1, "Wrong methyl group")
    methyl = methyls[0]
    center, order = edges[methyl][0]
    need(
        order == 1 and atoms[center]["element"] == "C" and len(edges[center]) == 3,
        "Wrong alpha carbon",
    )
    need(all(n == 1 for _, n in edges[center]), "Unsaturated alpha carbon")
    hydroxyls = [i for i, _ in edges[center] if atoms[i]["element"] == "O"]
    carboxyls = [i for i, _ in edges[center] if i != methyl and atoms[i]["element"] == "C"]
    need(len(hydroxyls) == len(carboxyls) == 1, "Wrong lactic-acid connectivity")
    hydroxyl, carboxyl = hydroxyls[0], carboxyls[0]
    need(edges[hydroxyl] == [(center, 1)], "Substituted alpha hydroxyl")
    need(
        sorted((atoms[i]["element"], n) for i, n in edges[carboxyl])
        == [("C", 1), ("O", 1), ("O", 2)],
        "Wrong carboxylic acid",
    )
    need(
        all(len(edges[i]) == 1 for i, _ in edges[carboxyl] if atoms[i]["element"] == "O"),
        "Substituted acid oxygen",
    )
    need(sum(a.get("stereo") is not None for a in rows) == 1, "Missing/extra stereo center")
    stereo = atoms[center].get("stereo")
    need(isinstance(stereo, dict), "Unspecified absolute stereo")
    reference_order = [methyl, hydroxyl, carboxyl]
    neighbors = stereo["neighbors"]
    need(
        len(neighbors) == 3 and set(neighbors) == set(reference_order), "Wrong stereo neighbor IDs"
    )
    permutation = [reference_order.index(i) for i in neighbors]
    odd = sum(a > b for j, a in enumerate(permutation) for b in permutation[j + 1 :]) % 2
    need(stereo["winding"] == ("cw" if odd else "ccw"), "Specified S reference inverted")
    need(atoms[center].get("explicit_h") == 1, "Lost center hydrogen")
    annotations = document["annotations"]
    names = document["molecule_names"]
    need(len(annotations) == len(names) == 1, "Missing/duplicate linked caption")
    annotation, linked = annotations[0], names[0]
    need(
        annotation["id"] not in atoms and linked["annotation"] == annotation["id"],
        "Dangling/reused caption ID",
    )
    need(annotation["text"] == linked["text"] == caption, "Changed caption text")
    need(
        len(linked["atoms"]) == 6 and set(linked["atoms"]) == set(atoms),
        "Incomplete linked molecule IDs",
    )
    need(linked["smiles"] == REFERENCE, "Linked identity lost specific S graph")
    need(annotation["format"]["alignment"] == "center", "Uncentered caption")
    need(
        annotation["position"]["y"] > max(a["position"]["y"] for a in rows),
        "Caption is not underneath molecule",
    )
    for field in ("arrows", "graphics", "groups"):
        need(not document.get(field), "Unexpected extra drawing objects")
    return center


def pair(alias, generated):
    native(alias, ALIAS)
    native(generated, SYSTEMATIC)

    def strip(d):
        return {k: v for k, v in d.items() if k not in ("annotations", "molecule_names")}

    need(
        strip(alias) == strip(generated),
        "Generating caption changed graph, stereo, positions or other drawing fields",
    )
    need(
        alias["annotations"][0]["id"] == generated["annotations"][0]["id"],
        "Caption was duplicated instead of reused",
    )
    need(
        alias["molecule_names"][0]["atoms"] == generated["molecule_names"][0]["atoms"],
        "Caption generation changed molecule IDs",
    )


def mapping(baseline, manual, undo):
    need(
        baseline.get("version") == manual.get("version") == undo.get("version") == 25,
        "Changed authentic native version",
    )
    rows = baseline["atoms"]
    atoms = {a["id"]: a for a in rows}
    need(
        len(rows) == len(atoms) == 6 and set(atoms) == set(range(1, 7)),
        "Changed reaction atom IDs/count",
    )
    need(
        [atoms[i]["element"] for i in range(1, 7)] == ["C", "C", "O", "C", "C", "O"],
        "Changed reference reaction elements",
    )
    need(
        [atoms[i]["map_num"] for i in range(1, 7)] == [1, 2, 3, 1, 2, 3],
        "Changed paired baseline maps",
    )
    need(
        len(baseline["bonds"]) == 4
        and sorted(
            (min(b["a"], b["b"]), max(b["a"], b["b"]), b["order"]) for b in baseline["bonds"]
        )
        == [(1, 2, 1), (2, 3, 1), (4, 5, 1), (5, 6, 2)],
        "Changed reference reaction connectivity",
    )
    need(len(baseline["arrows"]) == len(baseline["reactions"]) == 1, "Changed reaction/arrow count")
    reaction = baseline["reactions"][0]
    need(reaction["arrow"] == baseline["arrows"][0]["id"] == 7, "Broken reaction arrow reference")
    need(
        reaction["reactants"] == [{"atoms": [1, 2, 3], "coefficient": 1}]
        and reaction["products"] == [{"atoms": [4, 5, 6], "coefficient": 1}]
        and not reaction["agents"]
        and not reaction["annotations"],
        "Changed complete reactant/product roles",
    )
    changed = deepcopy(manual)
    target = [a for a in changed["atoms"] if a["id"] == 2]
    need(len(target) == 1 and target[0]["map_num"] == 27, "Wrong manually edited atom/map")
    target[0]["map_num"] = 2
    need(
        changed == baseline,
        "Manual edit changed another field, including graph, positions, styles, or roles",
    )
    need(undo == baseline, "Undo changed native document data")


def recorded_provenance():
    def raw(name):
        return json.loads(inside("raw/" + name + ".json.raw").read_bytes())

    bundle = raw("bundle-review")
    need(
        bundle["source_tree"] == TREE
        and bundle["raw_executable_sha256"] == RAW_GUI
        and bundle["signed_executable_sha256"] == SIGNED_GUI
        and bundle["test_helper_sha256"] == HELPER
        and not bundle["test_helper_shipped"]
        and bundle["single_executable"],
        "Different package/source qualification",
    )
    prelaunch = raw("root-prelaunch-verification-v1")
    need(
        prelaunch["signed_executable"] == SIGNED_GUI
        and prelaunch["codesign_verified"]
        and prelaunch["bundle_id"] == bundle["bundle_id"]
        and all(v == "" for v in prelaunch["launchservices_environment_overrides"].values()),
        "Different recorded LaunchServices preflight",
    )
    fresh = raw("fresh-process-reopen-preimage-v1")
    need(
        fresh["first_pid"] == 41162
        and fresh["first_pid_after_quit"] == "absent"
        and fresh["fresh_pid"] == 42659
        and fresh["save_after_reopen_byte_exact"]
        and fresh["sha256"] == NATIVE_HASHES["plus-lactic-generated-final.rsk"],
        "Fresh-process/save record mismatch",
    )
    graph = raw("root-native-graph-check-v1")
    need(
        graph["pure_rust_name_native_graph"] == "PASS" and len(graph["checks"]) == 2,
        "Missing recorded S graph checks",
    )
    need(
        {row["file"] for row in graph["checks"]}
        == {"plus-lactic-final.rsk", "plus-lactic-generated-final.rsk"},
        "Different recorded native graph files",
    )
    for row in graph["checks"]:
        need(
            row["sha256"] == row["old_sha256"] == NATIVE_HASHES[row["file"]]
            and row["byte_exact_to_qualified22a"]
            and (row["version"], row["atoms"], row["bonds"]) == (25, 6, 5),
            "Recorded earlier/current native bytes differ",
        )
    maps = raw("root-map-edit-undo-check-v1")
    need(
        maps["source_index_tree"] == TREE
        and maps["fresh_process_pid"] == 42659
        and maps["baseline_sha256"]
        == maps["undo_sha256"]
        == NATIVE_HASHES["reaction-map-labels-final.rsk"]
        and maps["manual27_sha256"] == NATIVE_HASHES["reaction-map-manual27-final.rsk"]
        and maps["whole_document_except_that_field_equal"]
        and maps["undo_entire_native_bytes_equal"],
        "Recorded map edit/Undo mismatch",
    )


def final_record():
    path = inside("raw/root-native-acceptance-final-v1.json.raw")
    exact_bytes(
        path.read_bytes(),
        "b3c522606945e47844afb9d61417beedd0b3037c9d8e7898e9efed0d4d00d934",
        "final Root native receipt",
    )
    record = json.loads(path.read_bytes())
    need(
        record["source_index_tree"] == TREE
        and record["status"] == "PASS for the focused fresh native acceptance sequence",
        "Different final native qualification",
    )
    need(
        len(record["native_files"]) == 5
        and {r["path"] for r in record["native_files"]} == set(NATIVE_HASHES),
        "Missing/duplicate final native record",
    )
    for row in record["native_files"]:
        data = inside(row["path"]).read_bytes()
        need(
            row["sha256"] == NATIVE_HASHES[row["path"]] == digest(data)
            and row["bytes"] == len(data),
            "Final receipt/native mismatch",
        )
    for key, name in (
        ("package_receipt", "bundle-review.json.raw"),
        ("prelaunch_receipt", "root-prelaunch-verification-v1.json.raw"),
    ):
        data = inside("raw/" + name).read_bytes()
        need(
            record[key]["sha256"] == digest(data) and record[key]["bytes"] == len(data),
            "Final receipt/package mismatch",
        )
    for row in record["supporting_receipts"]:
        data = inside("raw/" + row["path"] + ".raw").read_bytes()
        need(
            row["sha256"] == digest(data) and row["bytes"] == len(data),
            "Final receipt/supporting snapshot mismatch",
        )
    need(
        len(record["captures"]) == 11 and len({r["step"] for r in record["captures"]}) == 11,
        "Changed final original capture inventory",
    )
    copied = 0
    for row in record["captures"]:
        for kind in ("screenshot", "accessibility"):
            item = row[kind]
            path = inside("captures/" + item["path"])
            if path.exists():
                data = path.read_bytes()
                need(
                    item["sha256"] == digest(data) and item["bytes"] == len(data),
                    "Selected original capture/final receipt mismatch",
                )
                copied += 1
    need(copied == 12, "Missing six selected screenshot/AX pairs")


def verify():
    manifest = json.loads((HERE / "sha256.json").read_bytes())
    for name, row in manifest["raw_files"].items():
        data = inside(name).read_bytes()
        exact_bytes(data, row["sha256"], name)
        need(len(data) == row["bytes"], f"Changed byte length: {name}")
        if name.endswith(".jpg"):
            need(data.startswith(b"\xff\xd8\xff"), f"Not original JPEG: {name}")
    saved = {}
    for name, expected in NATIVE_HASHES.items():
        data = inside(name).read_bytes()
        exact_bytes(data, expected, name)
        saved[name] = json.loads(data)
    alias = saved["plus-lactic-final.rsk"]
    generated = saved["plus-lactic-generated-final.rsk"]
    pair(alias, generated)
    maps = [
        saved[name]
        for name in (
            "reaction-map-labels-final.rsk",
            "reaction-map-manual27-final.rsk",
            "reaction-map-undo-final.rsk",
        )
    ]
    mapping(*maps)
    need(
        inside("reaction-map-labels-final.rsk").read_bytes()
        == inside("reaction-map-undo-final.rsk").read_bytes(),
        "Undo bytes differ",
    )
    recorded_provenance()
    final_record()
    print(f"PASS: {len(manifest['raw_files'])} original byte copies; five v25 native files;")
    print("S graph/linked captions; only atom2.map_num 2->27; entire native Undo byte-exact.")
    print("Recorded provenance is reconciled, not independently reproduced; no app/GUI runs.")
    return alias, generated, maps


def self_test(alias, generated, maps):
    def rejects(call, label):
        try:
            call()
        except (ValueError, KeyError, TypeError, IndexError):
            return
        raise ValueError(f"Negative control accepted: {label}")

    flipped = deepcopy(alias)
    center = native(alias, ALIAS)
    next(a for a in flipped["atoms"] if a["id"] == center)["stereo"]["winding"] = "cw"
    rejects(lambda: native(flipped, ALIAS), "inverted S winding")
    detached = deepcopy(alias)
    detached["molecule_names"][0]["annotation"] = 999
    rejects(lambda: native(detached, ALIAS), "broken caption ID")
    moved = deepcopy(generated)
    moved["atoms"][0]["position"]["x"] += 1
    rejects(lambda: pair(alias, moved), "generation changed graph position")
    baseline, manual, undo = maps
    wrong_map = deepcopy(manual)
    next(a for a in wrong_map["atoms"] if a["id"] == 2)["map_num"] = 26
    rejects(lambda: mapping(baseline, wrong_map, undo), "different manual map")
    product = deepcopy(manual)
    next(a for a in product["atoms"] if a["id"] == 5)["map_num"] = 27
    rejects(lambda: mapping(baseline, product, undo), "additional product edit")
    label = deepcopy(manual)
    next(a for a in label["atoms"] if a["id"] == 2)["display"]["mapping"]["offset"]["x"] += 1
    rejects(lambda: mapping(baseline, label, undo), "unintended map label movement")
    arrow = deepcopy(manual)
    arrow["arrows"][0]["end"]["x"] += 1
    rejects(lambda: mapping(baseline, arrow, undo), "changed arrow position")
    changed_undo = deepcopy(undo)
    changed_undo["atoms"][0]["charge"] = 1
    rejects(lambda: mapping(baseline, manual, changed_undo), "Undo chemistry loss")
    data = inside("plus-lactic-final.rsk").read_bytes()
    rejects(
        lambda: exact_bytes(data + b" ", NATIVE_HASHES["plus-lactic-final.rsk"], "control"),
        "raw byte corruption",
    )
    rejects(lambda: inside("../outside"), "escaping packet path")
    cached = deepcopy(alias)
    for atom in cached["atoms"]:
        atom["cip_label"] = "R"
        atom["label_h"] = 99
    native(cached, ALIAS)
    print("PASS: 10 corruption controls rejected; misleading CIP/H display caches ignored.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    options = parser.parse_args()
    try:
        result = verify()
        if options.self_test:
            self_test(*result)
    except (OSError, ValueError, KeyError, TypeError, IndexError) as error:
        parser.exit(1, f"FAIL: {error}\n")
