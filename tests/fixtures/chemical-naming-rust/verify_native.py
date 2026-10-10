"""Check this finite native evidence set with Python's standard library only.

The explicit R reference is C[C@@H](O)C(=O)O. Native winding is relative to
listed heavy neighbours; cw relative to methyl/OH/carboxyl is that R graph.
This fixture checker is not a general CIP or nomenclature implementation.
"""

import copy
import hashlib
import json
from collections import Counter
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def need(condition, message):
    if not condition:
        raise ValueError(message)


def native(document, family):
    need(document["version"] == 19, "Unexpected native version")
    atoms = {atom["id"]: atom for atom in document["atoms"]}
    need(len(atoms) == len(document["atoms"]), "Duplicate atom ID")
    edges = {identifier: [] for identifier in atoms}
    for bond in document["bonds"]:
        need(bond["order"] in (1, 2), "Unexpected bond order")
        need(bond["a"] != bond["b"], "Self bond")
        for a, b in ((bond["a"], bond["b"]), (bond["b"], bond["a"])):
            need(a in atoms and b in atoms, "Missing endpoint")
            edges[a].append((b, bond["order"]))
    for atom in atoms.values():
        need(atom["element"] in ("C", "O"), "Unexpected fixture element")
        need(
            not any(
                atom.get(field, 0)
                for field in ("charge", "isotope", "radical_electrons", "aromatic")
            ),
            "Changed charge/isotope/radical/aromatic state",
        )
    counts = Counter(atom["element"] for atom in atoms.values())
    hydrogen = 0
    for identifier, atom in atoms.items():
        explicit = atom.get("explicit_h", 0)
        valence = {"C": 4, "O": 2}[atom["element"]]
        available = valence - sum(order for _, order in edges[identifier]) - explicit
        need(available >= 0, "Invalid fixture valence")
        hydrogen += explicit + (0 if atom.get("no_implicit", False) else available)
    # label_h and cip_label are display caches and are deliberately not read.
    if family == "ethanol":
        need(counts == {"C": 2, "O": 1} and hydrogen == 6, "Not C2H6O")
        need(len(document["bonds"]) == 2, "Not two ethanol bonds")
        oxygen = next(i for i, a in atoms.items() if a["element"] == "O")
        need(len(edges[oxygen]) == 1 and edges[oxygen][0][1] == 1, "Not terminal OH")
        carbon = edges[oxygen][0][0]
        need(atoms[carbon]["element"] == "C" and len(edges[carbon]) == 2, "Not C-C-O")
        need(all(order == 1 for bond in edges.values() for _, order in bond), "Unsaturated ethanol")
        need(
            all(atom.get("stereo") is None for atom in atoms.values()), "Unexpected ethanol stereo"
        )
        return "C2H6O; C-C-O"
    need(family == "r-lactic", "Unknown reference family")
    need(counts == {"C": 3, "O": 3} and hydrogen == 6, "Not C3H6O3")
    need(len(document["bonds"]) == 5, "Not five lactic-acid bonds")
    carbons = [i for i, atom in atoms.items() if atom["element"] == "C"]
    methyl = next(i for i in carbons if len(edges[i]) == 1)
    center = edges[methyl][0][0]
    need(
        len(edges[center]) == 3 and all(order == 1 for _, order in edges[center]),
        "Wrong alpha carbon",
    )
    hydroxyl = next(i for i, _ in edges[center] if atoms[i]["element"] == "O")
    carboxyl = next(i for i, _ in edges[center] if i != methyl and atoms[i]["element"] == "C")
    need(edges[hydroxyl] == [(center, 1)], "Wrong alpha hydroxyl")
    need(
        sorted((atoms[i]["element"], order) for i, order in edges[carboxyl])
        == [("C", 1), ("O", 1), ("O", 2)],
        "Wrong carboxylic acid",
    )
    for i, _ in edges[carboxyl]:
        if atoms[i]["element"] == "O":
            need(len(edges[i]) == 1, "Substituted acid oxygen")
    stereo = atoms[center].get("stereo")
    need(stereo is not None, "Lost specified R stereo")
    reference = [methyl, hydroxyl, carboxyl]
    need(
        len(stereo["neighbors"]) == 3 and set(stereo["neighbors"]) == set(reference),
        "Wrong stereo neighbours",
    )
    permutation = [reference.index(i) for i in stereo["neighbors"]]
    odd = sum(a > b for j, a in enumerate(permutation) for b in permutation[j + 1 :]) % 2
    need(stereo["winding"] == ("ccw" if odd else "cw"), "R reference was inverted")
    need(sum(atom.get("stereo") is not None for atom in atoms.values()) == 1, "Extra stereo center")
    return "C3H6O3; specified R-lactic acid"


def verify():
    manifest = json.loads((HERE / "sha256.json").read_bytes())
    for path, expected in manifest.items():
        need(
            hashlib.sha256((ROOT / path).read_bytes()).hexdigest() == expected,
            f"Changed bytes: {path}",
        )
    expected = json.loads((HERE / "native-expectations.json").read_bytes())
    documents = {}
    for name, family in expected["families"].items():
        document = json.loads((HERE / name).read_bytes())
        print(name, native(document, family))
        need(
            not any(document[key] for key in ("arrows", "graphics", "groups")), "Unexpected artwork"
        )
        documents[name] = document
    base = copy.deepcopy(documents["ethanol-rust-native.rsk"])
    caption = copy.deepcopy(documents["ethanol-rust-caption-native.rsk"])
    need(base.pop("annotations") == [], "Unexpected original annotation")
    annotations = caption.pop("annotations")
    need(base == caption, "Caption changed another native field")
    need(len(annotations) == 1 and annotations[0]["text"] == "ethan-1-ol", "Wrong caption")
    for names in expected.get("byte_equal_groups", []):
        need(
            len({(HERE / name).read_bytes() for name in names}) == 1,
            f"History/reopen changed {names}",
        )
    # Prove semantic checks do not merely trust checksums or cached labels.
    acid = next(
        documents[name] for name, family in expected["families"].items() if family == "r-lactic"
    )
    cached = copy.deepcopy(acid)
    for atom in cached["atoms"]:
        atom["cip_label"] = "S"
        atom["label_h"] = 999
    native(cached, "r-lactic")
    for field in ("winding", "charge", "isotope"):
        changed = copy.deepcopy(acid)
        center = next(atom for atom in changed["atoms"] if atom.get("stereo"))
        if field == "winding":
            center["stereo"][field] = "cw" if center["stereo"][field] == "ccw" else "ccw"
        else:
            center[field] = 1
        try:
            native(changed, "r-lactic")
        except ValueError:
            continue
        raise ValueError(f"Semantic canary accepted changed {field}")
    print(
        f"PASS: {len(manifest)} immutable files, {len(documents)} native graphs, caption-only delta and semantic canaries"
    )


if __name__ == "__main__":
    verify()
