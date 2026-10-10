"""Verify pinned rear-visibility evidence with Python's standard library.

This finite C60 audit checks bytes/native semantics and captured provenance.
It neither runs ReShiki nor certifies renderer visibility for other drawings.
"""

import hashlib
import json
import math
import sys
from collections import Counter
from copy import deepcopy
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent
SIGNED_APP = "bc9b3d61b3acb8c65e0b2ff1b6be2e0c3835ef9ebd097c1785f8210b28fb31f7"
SOURCE_AGGREGATE = "173a0dd53b1af5c66095f774f737cdde36657e865c991a8af5d5ed7a14fbb59e"
RAW_SHA256 = {
    "docs/images/rear-visibility-20261010/c60-25-after.jpg": "3b9560ad0f83a2fd2b2c699d27474e6c9019f648645e284b864155762f7fad5b",
    "docs/images/rear-visibility-20261010/c60-25-before.jpg": "b511eae8cfaffe60761793f847c86ddda9adcf4c44c91628b5c4202779bc37ff",
    "docs/images/rear-visibility-20261010/c60-25-fresh-process-reopened.jpg": "8835fb3d2df33b7c1fdaf607bf3cb23c5037ce9806a656da162a9b11bea7a267",
    "docs/images/rear-visibility-20261010/c60-zero-after.jpg": "1fe1247ced7a3583b745de098b08ff9ad314a32f84fdf2f9db17d3bc0e785d5b",
    "docs/images/rear-visibility-20261010/c60-zero-fresh-process-reopened.jpg": "48217528b42c53e3559974629457f3923d0baeeabf776adce4e0b89d251797ee",
    "docs/images/rear-visibility-20261010/c60-zero-tilted-fresh-process-reopened.jpg": "f77a5658bdc07fde0538bad8f34083c4117da5bd837c59953c8b3eb08ffb9c46",
    "docs/images/rear-visibility-20261010/c60-zero-tilted-selection-cleared.jpg": "f1178535a9c93ebf9fdf44ed801d241041475bca43f47d0844fdecb6669e06da",
    "tests/fixtures/rear-visibility-20261010/accepted-named-test-counts.json": "9c7883fa20b28888f70aa381c654d819049bffc592b20b3cd48f75da76093871",
    "tests/fixtures/rear-visibility-20261010/app-history-complete-bin.json": "4a1d091b7d9b81159ba64a57a067dd76284d580874f21ceed99a22173e921241",
    "tests/fixtures/rear-visibility-20261010/app-history-complete-bin.log": "26b8942de103a33b4a868baf176e52fd65247b051455c0337f883de63dd74200",
    "tests/fixtures/rear-visibility-20261010/before-capture-provenance.json": "c7263ec61b097f759e513bfd914c76c3fde9c98150b77e1b49dfcb4de547dfd6",
    "tests/fixtures/rear-visibility-20261010/c60-visibility-native-0-tilted.rsk": "1ca19b3dbdd78bf4b6bdd1d5fd759246d179a2fd058029d0082086bb4e7cf915",
    "tests/fixtures/rear-visibility-20261010/c60-visibility-native-0.rsk": "5ba045fc362dbc16ef3aebd08f5d41b463e64f1a810391c45c16340c5fe4373c",
    "tests/fixtures/rear-visibility-20261010/c60-visibility-native-25.rsk": "29e4ac58273964ae673267769c56e5f1fe4e036e06a4875ebbbe2ff645ec4ee1",
    "tests/fixtures/rear-visibility-20261010/compiled-validation-summary.json": "db9473c27f978863f8c1d0ca74090e5d721eb6bb0f21fac751872943e5bb1f33",
    "tests/fixtures/rear-visibility-20261010/compiler-artifact-origin.json": "621a1f8a6e5a119af7d20c862fd2ccf607b9e865c616772e81cffab725977624",
    "tests/fixtures/rear-visibility-20261010/default-build.json": "d5fd1a574e475c4f76873896ea3958f886566450cf4cdf24b0c0a282d72e5035",
    "tests/fixtures/rear-visibility-20261010/frozen-source-receipt.json": "f3c41047b14793f01eb376bcadefa9f99d2021e52d6955dc968df64c20749037",
    "tests/fixtures/rear-visibility-20261010/independent-native-semantic-audit.json": "5475ebd152340476fa7802c46652d403cd376298649a96d49fd036eefb2bebcc",
    "tests/fixtures/rear-visibility-20261010/model-actual-raster.json": "ed19204f1e6986a9a25768509e2e6a83e1c52de9183f3ba2b99efe83036dabdf",
    "tests/fixtures/rear-visibility-20261010/model-actual-raster.log": "e9a357bd30e1eac3399a19c71523ac7ae78b4da36df9ae777a9c9ec373da9c43",
    "tests/fixtures/rear-visibility-20261010/native-wgpu-bin.json": "7ba6782648697d0334ca2a0373512d647045b2c21104458224dfe4005f343602",
    "tests/fixtures/rear-visibility-20261010/native-wgpu-bin.log": "8ccd0dbea8bd30f1f434c21e104944ce2876a5aecd9ed16501cb4ea49a50fd22",
    "tests/fixtures/rear-visibility-20261010/required-check.json": "1a3cf55f05b756949d8e0940b5e9b00e60770e679065c75b31e4da11e159578d",
    "tests/fixtures/rear-visibility-20261010/required-clippy.json": "da3e99dfa1f7be13cc85955cb4e32bae88176f60f5ced0f857babb4d7db73fff",
    "tests/fixtures/rear-visibility-20261010/required-fmt.json": "01b42edc990bf37fc1a5382c963ef45d067f453670427c296774f653d4a9f142",
    "tests/fixtures/rear-visibility-20261010/root-candidate-source-provenance-audit.json": "88d9cca2fe7b49a5e4d58d055c091195cbb3643c745e57bd38dae23270443bc2",
    "tests/fixtures/rear-visibility-20261010/root-exports.json": "85b3ebd710581ec77a562d38788ab61b322baad9b664e58ff3a34db538b87ced",
    "tests/fixtures/rear-visibility-20261010/root-exports.log": "d3c60b7a23e8391e0282346a06bc724b22496b4cb6577fd03f0b9e2f57776bd9",
    "tests/fixtures/rear-visibility-20261010/root-native-acceptance.json": "13ec17b2d140e75808a42d55a2157bee7b1b1925f84d79c227a9ea5c1dc344c5",
    "tests/fixtures/rear-visibility-20261010/signed-app-provenance.json": "b3d92098d0317387ef5776853fae71c41ad2c277e01c333d2a6737c6299e6075",
    "tests/fixtures/rear-visibility-20261010/signed-cli-verification.json": "785ba93441b6f9cdf2bbaa57ec7fe72ef38d7a5b9b9a569cfa9867ee68352481",
}


def require(ok, message):
    if not ok:
        raise ValueError(message)


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def load(name):
    return json.loads((HERE / name).read_text())


def jpeg_size(raw):
    require(raw[:3] == b"\xff\xd8\xff", "JPEG magic")
    i = 2
    while i < len(raw):
        require(raw[i] == 255, "JPEG marker")
        while raw[i] == 255:
            i += 1
        marker = raw[i]
        i += 1
        if marker in (0xD8, 0xD9):
            continue
        size = int.from_bytes(raw[i : i + 2], "big")
        require(size >= 2, "JPEG segment size")
        if marker in (0xC0, 0xC1, 0xC2):
            return (
                int.from_bytes(raw[i + 5 : i + 7], "big"),
                int.from_bytes(raw[i + 3 : i + 5], "big"),
            )
        i += size
    raise ValueError("JPEG frame missing")


def validate_bytes():
    for relative, expected in RAW_SHA256.items():
        raw = (ROOT / relative).read_bytes()
        require(digest(raw) == expected, "Original bytes changed: " + relative)
        if relative.endswith(".jpg"):
            require(jpeg_size(raw) == (2560, 1704), "Capture dimensions: " + relative)


def differences(a, b, path=""):
    if type(a) is not type(b):
        return [(path, a, b)]
    if isinstance(a, dict):
        require(a.keys() == b.keys(), "Changed native keys at " + path)
        return [d for k in sorted(a) for d in differences(a[k], b[k], path + "/" + k)]
    if isinstance(a, list):
        require(len(a) == len(b), "Changed native list length at " + path)
        return [
            d for i, (x, y) in enumerate(zip(a, b)) for d in differences(x, y, path + "/" + str(i))
        ]
    return [] if a == b else [(path, a, b)]


def xyz(atom):
    return [atom["position"]["x"], atom["position"]["y"], atom["depth"]]


def validate_native(docs):
    a, b, c = docs
    for doc in docs:
        require(doc["version"] == 22, "Native version")
        require([x["id"] for x in doc["atoms"]] == list(range(1, 61)), "C60 atom IDs")
        require(len(doc["bonds"]) == 90, "C60 bond count")
        for atom in doc["atoms"]:
            require(atom["element"] == "C", "Carbon identity")
            for key in (
                "charge",
                "isotope",
                "radical_electrons",
                "explicit_h",
                "label_h",
                "map_num",
            ):
                require(atom[key] == 0, "Unexpected atom " + key)
            require(
                atom["no_implicit"] is True and atom["aromatic"] is False, "Hydrogen/aromatic state"
            )
            require(atom["stereo"] is None, "Unexpected atom stereo")
        require(Counter(x["order"] for x in doc["bonds"]) == {1: 60, 2: 30}, "C60 bond orders")
        edges = set()
        adjacency = {i: [] for i in range(1, 61)}
        valence = Counter()
        for bond in doc["bonds"]:
            require(bond["display"] == "plain" and bond["color"] == [0, 0, 0], "Bond appearance")
            require(bond["stereo"] is None and bond["stereo_atoms"] == [], "Bond stereo")
            u, v = bond["a"], bond["b"]
            require(u in adjacency and v in adjacency and u != v, "Bond endpoints")
            edge = tuple(sorted((u, v)))
            require(edge not in edges, "Duplicate bond")
            edges.add(edge)
            adjacency[u].append(v)
            adjacency[v].append(u)
            valence[u] += bond["order"]
            valence[v] += bond["order"]
        require(all(len(n) == 3 for n in adjacency.values()), "C60 degree")
        require(all(v == 4 for v in valence.values()), "C60 bond-order valence")
        seen, pending = set(), [1]
        while pending:
            i = pending.pop()
            if i not in seen:
                seen.add(i)
                pending.extend(adjacency[i])
        require(len(seen) == 60, "Disconnected C60")
    require(
        differences(a, b) == [("/depth_appearance/0/rear_opacity", 0.25, 0.0)],
        "Opacity-only difference",
    )
    tilted_diff = differences(b, c)
    expected_paths = {
        f"/atoms/{i}/{field}" for i in range(60) for field in ("position/x", "position/y", "depth")
    }
    require(
        len(tilted_diff) == 180 and {x[0] for x in tilted_diff} == expected_paths,
        "Tilt changed other fields",
    )
    original, tilted = [xyz(x) for x in b["atoms"]], [xyz(x) for x in c["atoms"]]
    center = [sum(p[i] for p in original) / 60 for i in range(3)]
    sx, cx = math.sin(math.radians(-21.25)), math.cos(math.radians(-21.25))
    sy, cy = math.sin(math.radians(35)), math.cos(math.radians(35))
    matrix = [[cy, sy * sx, sy * cx], [0, cx, -sx], [-sy, cy * sx, cy * cx]]
    residual = max(
        abs(
            tilted[n][i]
            - center[i]
            - sum(matrix[i][j] * (original[n][j] - center[j]) for j in range(3))
        )
        for n in range(60)
        for i in range(3)
    )
    require(math.isfinite(residual) and residual < 0.00002, "Expected rigid tilt")
    pair_drift = max(
        abs(math.dist(original[i], original[j]) - math.dist(tilted[i], tilted[j]))
        for i in range(60)
        for j in range(i)
    )
    require(pair_drift < 0.00003, "Rigid pair distances")
    return residual, pair_drift


def validate_provenance():
    source = load("frozen-source-receipt.json")
    inputs = source["source_inputs"]
    require(source["source_input_count"] == len(inputs) == 1071, "Source count")
    aggregate = digest(json.dumps(inputs, sort_keys=True, separators=(",", ":")).encode())
    require(aggregate == source["source_aggregate"] == SOURCE_AGGREGATE, "Source aggregate")
    for relative, expected in inputs.items():
        require(
            digest((ROOT / relative).read_bytes()) == expected,
            "Compiled source changed: " + relative,
        )
    require(len(source["changed_hashes"]) == 13, "Correction source count")
    require(all(inputs[k] == v for k, v in source["changed_hashes"].items()), "Frozen correction")
    artifacts = load("compiler-artifact-origin.json")
    require(
        artifacts["source_aggregate"] == aggregate and artifacts["own_artifact_count"] == 15,
        "Compiler source linkage",
    )
    require(
        len(artifacts["own_artifacts"]) == 15
        and all(x["fresh"] is False for x in artifacts["own_artifacts"]),
        "Own fresh:false records",
    )
    require(
        artifacts["source_receipt_sha256"]
        == digest((HERE / "frozen-source-receipt.json").read_bytes()),
        "Compiler receipt linkage",
    )
    for dep in artifacts["depfiles"].values():
        for relative, expected in dep["frozen_source_dependencies"].items():
            require(inputs[relative] == expected, "Compiler dependency linkage")
        for relative, entry in dep["included_non_rust_files_exact_to_base"].items():
            require(entry["unchanged_from_base_head"] is True, "Embedded dependency baseline")
            require(
                digest((ROOT / relative).read_bytes()) == entry["sha256"],
                "Embedded source changed: " + relative,
            )
    signed = load("signed-app-provenance.json")
    native = load("root-native-acceptance.json")
    require(
        signed["signed_binary_sha256"] == native["signed_candidate_sha256"] == SIGNED_APP,
        "Signed candidate linkage",
    )
    require(
        signed["source_aggregate"] == aggregate
        and signed["raw_binary_sha256"] == artifacts["raw_binary_sha256"],
        "Linked binary provenance",
    )
    require(
        native["opacity_only_diff"] == [["/depth_appearance/0/rear_opacity", 0.25, 0.0]]
        and native["tilt_diff_count"] == 180,
        "Native receipt linkage",
    )
    require(
        native["fresh_pid"] == 24802 and native["tilt_coordinates_change_not_originalXYZ"] is True,
        "Fresh/tilt receipt",
    )
    before = load("before-capture-provenance.json")
    require(
        before["executable_sha256"]
        == "4d35fe616bc9cb1388e4a6ec45312c65cc21aa6b67e42e17b077dd848fcc4043",
        "Earlier implementation linkage",
    )
    require(
        before["image_sha256"]
        == RAW_SHA256["docs/images/rear-visibility-20261010/c60-25-before.jpg"],
        "Before image linkage",
    )
    tests = load("accepted-named-test-counts.json")
    require(
        tests["unique_accepted_pass_count"]
        == sum(x["passed"] for x in tests["accepted"].values())
        == 26,
        "Accepted test count",
    )
    for name in (
        "model-actual-raster",
        "root-exports",
        "app-history-complete-bin",
        "native-wgpu-bin",
    ):
        entry = tests["accepted"][name]
        require(
            digest((HERE / (name + ".log")).read_bytes()) == entry["stdout_sha256"],
            "Original test log linkage",
        )
        require(load(name + ".json")["exit_code"] == 0, "Accepted test phase")
    for name in ("default-build", "required-check", "required-clippy", "required-fmt"):
        require(load(name + ".json")["exit_code"] == 0, "Required compiled phase")


def rejects(call, label):
    try:
        call()
    except ValueError:
        return
    raise ValueError("Corruption control was accepted: " + label)


def corruption_controls(docs):
    controls = []
    for label, field, value in (
        ("element", "element", "N"),
        ("isotope", "isotope", 13),
        ("charge", "charge", 1),
        ("label_h", "label_h", 1),
    ):
        bad = deepcopy(docs)
        for doc in bad:
            doc["atoms"][0][field] = value
        controls.append((label, bad))
    for label, field, value in (("bond order", "order", 2), ("bond display", "display", "dashed")):
        bad = deepcopy(docs)
        for doc in bad:
            doc["bonds"][0][field] = value
        controls.append((label, bad))
    bad = deepcopy(docs)
    bad[1]["depth_appearance"][0]["rear_opacity"] = 0.5
    controls.append(("opacity", bad))
    bad = deepcopy(docs)
    bad[2]["atoms"][0]["depth"] += 0.1
    controls.append(("single depth", bad))
    for label, factor in (("reflection", -1), ("uniform scaling", 1.02)):
        bad = deepcopy(docs)
        for atom in bad[2]["atoms"]:
            atom["position"]["x"] *= factor
            if factor > 0:
                atom["position"]["y"] *= factor
                atom["depth"] *= factor
        controls.append((label, bad))
    for label, bad in controls:
        rejects(lambda value=bad: validate_native(value), label)
    sample = (HERE / "c60-visibility-native-25.rsk").read_bytes()
    rejects(
        lambda: require(digest(sample + b" ") == digest(sample), "Corrupted bytes"), "byte checksum"
    )
    return len(controls) + 1


def main():
    validate_bytes()
    docs = [load("c60-visibility-native-" + suffix + ".rsk") for suffix in ("25", "0", "0-tilted")]
    residual, pair_drift = validate_native(docs)
    validate_provenance()
    controls = corruption_controls(docs)
    print(
        f"PASS: {len(RAW_SHA256)} raw originals, 7 JPEGs, 3 complete native drawings, 1071 source hashes, 15 fresh:false artifacts, {controls} corruption controls"
    )
    print(
        f"Rigid tilt max XYZ residual {residual:.9g}; max pair-distance drift {pair_drift:.9g} document units"
    )
    print("Scope: pinned C60/native/source receipts; no compilation, GUI or renderer replay")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, OSError) as error:
        print("FAIL:", error, file=sys.stderr)
        sys.exit(1)
