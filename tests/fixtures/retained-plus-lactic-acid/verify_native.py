#!/usr/bin/env python3
"""Finite lactic-acid fixture/byte checker, not a general CIP or GUI validator.

The independently declared S reference is C[C@H](O)C(=O)O (ChEBI:422).
Native winding is relative to its recorded neighbour order. Display caches
cip_label and label_h are ignored. Paths depend only on this public packet.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from collections import Counter
from copy import deepcopy
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
ALIAS = "(+)-lactic acid"
SYSTEMATIC = "(2S)-2-hydroxypropanoic acid"
REFERENCE = "C[C@H](O)C(=O)O"


def need(condition, message):
    if not condition:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def inside(relative):
    path = Path(relative)
    need(not path.is_absolute(), "Absolute packet path")
    resolved = (ROOT / path).resolve()
    need(resolved.is_relative_to(ROOT), "Packet path escapes root")
    return resolved


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


def verify():
    manifest = json.loads((HERE / "sha256.json").read_bytes())
    for name, expected in manifest.items():
        need(digest(inside(name).read_bytes()) == expected, f"Changed bytes: {name}")
    alias = json.loads((HERE / "plus-lactic-acid.rsk").read_bytes())
    generated = json.loads((HERE / "plus-lactic-generated-name.rsk").read_bytes())
    pair(alias, generated)
    fresh = json.loads((HERE / "native-fresh-reopen-readback.json").read_bytes())
    need(
        fresh["status"] == "PASS_FRESH_PROCESS_NATIVE_ROUND_TRIP" and fresh["old_process_exited"],
        "Missing qualified ROOT fresh-process record",
    )
    need(
        fresh["source_index_tree"] == "22a60b33376bfbcb9e2af32880b128f27877ec65",
        "Fresh readback has different source tree",
    )
    need(len(fresh["documents"]) == 2, "Missing fresh native document record")
    need(
        {r["path"] for r in fresh["documents"]}
        == {"plus-lactic-acid.rsk", "plus-lactic-generated-name.rsk"},
        "Unexpected fresh document paths",
    )
    for row in fresh["documents"]:
        data = (HERE / row["path"]).read_bytes()
        need(
            row["second_save_byte_exact"]
            and row["sha256_before"] == row["sha256_after"] == digest(data),
            "Fresh native second save not byte-identical",
        )
    review = json.loads((HERE / "scoped-test-review.json").read_bytes())
    observed = []
    for filename, count in (("local-tests.log", 13), ("import-dock-tests.log", 10)):
        data = (HERE / filename).read_text()
        names = re.findall(r"^test (\S+) \.\.\. ok$", data, re.MULTILINE)
        need(len(names) == len(set(names)) == count, "Unexpected named test passes")
        need(
            re.search(rf"test result: ok\. {count} passed; 0 failed; 0 ignored;", data),
            "Missing successful test summary",
        )
        observed.extend(names)
    need(len(observed) == len(set(observed)) == 23, "Duplicate combined test names")
    need(set(observed) == set(review["all_passed_names"]), "Test review/log mismatch")
    need(
        len(review["new_tests"]) == 3 and set(review["new_tests"]) <= set(observed),
        "New regressions missing",
    )
    print(f"PASS: {len(manifest)} original byte hashes; two six-atom/five-bond S native files;")
    print(
        "linked alias/systematic captions; unchanged graph/positions; 23 named test passes (3 new)."
    )
    print(
        "This checks finite saved data and bytes, not GUI actions, pixel layout, process provenance or general CIP."
    )
    return alias, generated


def self_test(alias, generated):
    center = native(alias, ALIAS)

    def rejects(fn, label):
        try:
            fn()
        except (ValueError, KeyError, TypeError, IndexError):
            return
        raise ValueError(f"Negative control accepted: {label}")

    cases = []
    for label, mutate in (
        (
            "inverted absolute stereo",
            lambda d: next(a for a in d["atoms"] if a["id"] == center)["stereo"].update(
                winding="cw"
            ),
        ),
        (
            "unspecified stereo",
            lambda d: next(a for a in d["atoms"] if a["id"] == center).update(stereo=None),
        ),
        ("isotope", lambda d: d["atoms"][0].update(isotope=18)),
        ("charge", lambda d: d["atoms"][0].update(charge=-1)),
        ("removed bond", lambda d: d["bonds"].pop()),
        ("dangling caption", lambda d: d["molecule_names"][0].update(annotation=999)),
        ("incomplete caption targets", lambda d: d["molecule_names"][0]["atoms"].pop()),
        (
            "wrong linked enantiomer",
            lambda d: d["molecule_names"][0].update(smiles="C[C@@H](O)C(=O)O"),
        ),
    ):
        changed = deepcopy(alias)
        mutate(changed)
        rejects(lambda d=changed: native(d, ALIAS), label)
        cases.append(label)
    moved = deepcopy(generated)
    moved["atoms"][0]["position"]["x"] += 1
    rejects(lambda: pair(alias, moved), "caption changed graph coordinates")
    stale = deepcopy(generated)
    stale["annotations"][0]["text"] = ALIAS
    rejects(lambda: pair(alias, stale), "stale generated caption")
    rejects(lambda: inside("../../outside"), "escaping path")
    data = (HERE / "plus-lactic-acid.rsk").read_bytes()
    need(digest(data + b" ") != digest(data), "Changed bytes escaped hash guard")
    cached = deepcopy(alias)
    for atom in cached["atoms"]:
        atom["cip_label"] = "R"
        atom["label_h"] = 99
    native(cached, ALIAS)
    need(len(cases) == 8, "Incomplete negative controls")
    print("PASS: 12 corruption controls rejected; misleading CIP/H display caches ignored.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    options = parser.parse_args()
    try:
        alias, generated = verify()
        if options.self_test:
            self_test(alias, generated)
    except (ValueError, KeyError, TypeError, IndexError, json.JSONDecodeError, OSError) as error:
        raise SystemExit(f"FAIL: {error}") from error
