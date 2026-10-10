"""Independent checks of ROOT's actual desktop native saves; no app code imports."""

import copy
import hashlib
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parent
GLOBAL = ROOT.parent


def load(name):
    return json.loads((ROOT / (name + ".rsk")).read_text())


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def delta(before, after):
    return {axis: after[axis] - before[axis] for axis in ("x", "y")}


def carries(before, after, displacement):
    return all(
        abs(after[axis] - before[axis] - displacement[axis]) < 5e-6
        for axis in ("x", "y")
    )


before = load("methanol-marked-input")
created = load("methanol-attached-arrow-desktop")
creation_undo = load("methanol-attached-arrow-undo")
creation_redo = load("methanol-attached-arrow-redo")
tangent = load("methanol-attached-tangent-desktop")
target = load("methanol-attached-target-moved-desktop")
source = load("methanol-attached-source-moved-desktop")

expected_undo = copy.deepcopy(before)
expected_undo["version"] = 21
expected_undo["atoms"][0]["label_h"] = 3
assert creation_undo == expected_undo
assert creation_redo == created
assert len(created["arrows"]) == 1
assert not creation_undo["arrows"]

expected_created = copy.deepcopy(expected_undo)
expected_created["atoms"][1]["marks"][0]["id"] = 1
expected_created["atoms"][1]["mark_serial"] = 1
expected_created["arrows"] = created["arrows"]
assert expected_created == created
arrow = created["arrows"][0]
assert arrow["start_anchor"]["target"] == {"kind": "lone_pair", "atom": 2, "mark": 1}
assert arrow["end_anchor"]["target"] == {"kind": "atom", "atom": 1}

tangent_arrow = tangent["arrows"][0]
expected_tangent = copy.deepcopy(created)
expected_tangent["arrows"][0]["cubic"][0] = tangent_arrow["cubic"][0]
assert expected_tangent == tangent
assert tangent_arrow["cubic"][0] != arrow["cubic"][0]

target_arrow = target["arrows"][0]
target_delta = delta(tangent["atoms"][0]["position"], target["atoms"][0]["position"])
assert carries(tangent_arrow["end"], target_arrow["end"], target_delta)
assert carries(tangent_arrow["cubic"][1], target_arrow["cubic"][1], target_delta)
expected_target = copy.deepcopy(tangent)
expected_target["atoms"][0]["position"] = target["atoms"][0]["position"]
expected_target["arrows"][0]["end"] = target_arrow["end"]
expected_target["arrows"][0]["cubic"][1] = target_arrow["cubic"][1]
assert expected_target == target
assert load("methanol-attached-target-undo") == tangent
assert load("methanol-attached-target-redo") == target

deleted = load("methanol-attached-target-deleted")
assert len(deleted["atoms"]) == 1 and not deleted["bonds"]
assert "end_anchor" not in deleted["arrows"][0]
for field in ("start", "end", "cubic", "start_anchor", "kind", "style"):
    assert deleted["arrows"][0][field] == target_arrow[field]
assert load("methanol-attached-target-deletion-undo") == target

source_arrow = source["arrows"][0]
source_delta = delta(target["atoms"][1]["position"], source["atoms"][1]["position"])
assert carries(target_arrow["start"], source_arrow["start"], source_delta)
assert carries(target_arrow["cubic"][0], source_arrow["cubic"][0], source_delta)
expected_source = copy.deepcopy(target)
expected_source["atoms"][1]["position"] = source["atoms"][1]["position"]
expected_source["arrows"][0]["start"] = source_arrow["start"]
expected_source["arrows"][0]["cubic"][0] = source_arrow["cubic"][0]
source_offset_roundoff = (
    source_arrow["start_anchor"]["offset"]["x"]
    - target_arrow["start_anchor"]["offset"]["x"]
)
assert abs(source_offset_roundoff) < 5e-6
expected_source["arrows"][0]["start_anchor"]["offset"]["x"] = source_arrow["start_anchor"]["offset"]["x"]
assert expected_source == source

removed = load("methanol-attached-lone-pair-removed")
assert not removed["atoms"][1].get("marks")
assert "start_anchor" not in removed["arrows"][0]
for field in ("start", "end", "cubic", "end_anchor", "kind", "style"):
    assert removed["arrows"][0][field] == source_arrow[field]
assert load("methanol-attached-lone-pair-removal-undo") == source

detached = load("methanol-attached-end-detached")
expected_detached = copy.deepcopy(source)
del expected_detached["arrows"][0]["end_anchor"]
assert detached == expected_detached
assert load("methanol-attached-end-detach-undo") == source
dragged = load("methanol-attached-endpoint-dragged")
assert "start_anchor" not in dragged["arrows"][0]
assert dragged["arrows"][0]["start"] != source_arrow["start"]
for field in ("end", "end_anchor", "kind", "style"):
    assert dragged["arrows"][0][field] == source_arrow[field]
assert dragged["arrows"][0]["cubic"][1] == source_arrow["cubic"][1]
assert load("methanol-attached-endpoint-drag-undo") == source
assert load("methanol-attached-pending-cancelled") == source

free = load("methanol-explicit-free-arrow-desktop")
assert len(free["arrows"]) == 2 and free["arrows"][0] == source_arrow
assert "start_anchor" not in free["arrows"][1] and "end_anchor" not in free["arrows"][1]
expected_free = copy.deepcopy(source)
expected_free["arrows"] = free["arrows"]
assert expected_free == free
assert load("methanol-explicit-free-arrow-undo") == source

reopened = ROOT / "methanol-attached-arrow-reopened-desktop.rsk"
assert reopened.read_bytes() == (ROOT / "methanol-attached-arrow-desktop.rsk").read_bytes()
crowded = load("crowded-methoxide-carbonyl-input")
crowded_undo = load("crowded-carbonyl-atom-undo")
expected_crowded = copy.deepcopy(crowded)
expected_crowded["version"] = 21
expected_crowded["atoms"][0]["label_h"] = 3
expected_crowded["atoms"][2]["label_h"] = 2
for index in (2, 3):
    assert expected_crowded["atoms"][index]["marks"] == []
    del expected_crowded["atoms"][index]["marks"]
assert crowded_undo == expected_crowded
for name, target_reference in (
    ("crowded-carbonyl-atom-attached-desktop", {"kind": "atom", "atom": 3}),
    ("crowded-carbonyl-bond-attached-desktop", {"kind": "bond", "a": 3, "b": 4}),
):
    result = load(name)
    assert len(result["arrows"]) == 1
    expected = copy.deepcopy(expected_crowded)
    expected["atoms"][1]["marks"][0]["id"] = 1
    expected["atoms"][1]["mark_serial"] = 1
    expected["arrows"] = result["arrows"]
    assert expected == result
    actual_reference = result["arrows"][0]["end_anchor"]["target"]
    assert all(actual_reference[k] == value for k, value in target_reference.items())
    if target_reference["kind"] == "bond":
        assert 0 < actual_reference["fraction"] < 1

provenance = json.loads((ROOT / "final-artifact-provenance.json").read_text())
assert sha(Path(provenance["signed_binary"])) == provenance["signed_binary_sha256"]
assert provenance["own_workspace_artifacts_all_fresh_false"]
assert all(not artifact["fresh"] for artifact in provenance["own_workspace_artifacts"])

images = [
    GLOBAL / "evidence/baseline/mechanism-attachment-two-free-arrows-before.png",
    *[GLOBAL / ("evidence/after/" + name) for name in (
        "mechanism-attachment-pending-source.jpg",
        "mechanism-attachment-controls.jpg",
        "mechanism-attachment-one-arrow-after.jpg",
        "mechanism-attachment-crowded-carbonyl-f8-off.jpg",
        "mechanism-attachment-native-reopened.jpg",
        "mechanism-attachment-reopened-attachment-controls-verified.jpg",
    )],
]
receipt = {
    "production_head": provenance["head"],
    "signed_application_sha256": provenance["signed_binary_sha256"],
    "actual_desktop": "macOS26.5.1 arm64; 2560x1704;250% zoom;light;JACS/ACS;Arial10;Inspector visible;F8off",
    "matched_control": "Original methanol LP fixture: two source/destination clicks, two free baseline arrows versus one native21 attached cubic",
    "input_adapters": "Version19 input writes21 and hydrates only recorded derived carbon label_h; native writer omits two empty marks arrays in additional carbonyl input; successful link assigns only O mark ID1/highwater1",
    "passed": [
        "one arrow and one creation Undo",
        "creation Redo entire JSON exact",
        "only departure cubic control changed by tangent drag; both links/endpoints unchanged",
        "target-only motion carries only end plus arrival control by actual atom displacement",
        "target movement Undo/Redo entire JSON exact",
        "target deletion detaches last end without changing resolved curve; Undo entire JSON exact",
        "source-only motion carries only start plus departure control by actual atom displacement",
        "lone-pair deletion detaches last start without changing resolved curve; Undo entire JSON exact",
        "named Detach end changes only the reference; Undo entire JSON exact",
        "endpoint drag detaches only start; opposite end/control/reference unchanged; Undo exact",
        "pending-source Escape leaves entire saved native JSON unchanged",
        "explicit free mode adds one free arrow; other document fields exact; Undo exact",
        "fresh-process reopen and Save As byte-exact; disabled Undo/Redo and enabled start/end detach UI observed",
        "separate carbonyl atom/bond contexts retain all molecular fields except documented label cache and mark IDs",
        "signed application and own compiler artifact freshness verified",
    ],
    "target_drag_actual_world_delta": target_delta,
    "source_drag_actual_world_delta": source_delta,
    "source_anchor_offset_x_f32_roundoff": source_offset_roundoff,
    "native_files": {p.name: {"bytes": p.stat().st_size, "sha256": sha(p)} for p in sorted(ROOT.glob("*.rsk"))},
    "raw_captures": {str(p.relative_to(GLOBAL)): {"bytes": p.stat().st_size, "sha256": sha(p)} for p in images},
    "limits": [
        "Native drag lengths/angles remained enabled; actual displacement is verified, not assumed equal to raw pointer delta",
        "Fresh reopen Fit recenters the same250% camera because the saved arrow is included in bounds",
        "Crowded context is an additional input, not the matched before fixture",
        "Alt bypass, held-drag Escape, both-selected affine transforms and copy/remap are compiled-test evidence rather than actual desktop operations here",
        "No automatic reaction prediction or obstacle-free routing claim",
        "User personal visual acceptance remains pending",
    ],
}
(ROOT / "desktop-independent-check.json").write_text(json.dumps(receipt, indent=2) + "\n")
print("PASS", len(receipt["passed"]), "independent desktop checks;", len(receipt["native_files"]), "native files;", len(images), "raw captures")
