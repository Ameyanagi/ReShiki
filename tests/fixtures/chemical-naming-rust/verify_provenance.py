"""Compare this checkout with the frozen clean Rust build's recorded inputs.

This verifies receipt consistency and source bytes; it does not rebuild the
application, repeat GUI actions or prove runtime checks on foreign platforms.
"""

import hashlib
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def need(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load(name):
    return json.loads((HERE / name).read_bytes())


def verify():
    manifest = load("sha256.json")
    for path, expected in manifest.items():
        need(digest(ROOT / path) == expected, f"Changed recorded bytes: {path}")
    source = load("runtime-inputs-final2-before.json")
    compiler = load("final2-own-compiler-receipt.json")
    bridge = load("final2-source-patch-correlation-bridge.json")
    package = load("package-provenance.json")
    desktop = load("native-verification.json")
    need(source["source_patch_sha256"] is None, "Original omitted field was rewritten")
    need(bridge["original_source_patch_sha256"] is None, "Bridge changed omission history")
    need(
        bridge["original_manifest_sha256"] == digest(HERE / "runtime-inputs-final2-before.json"),
        "Wrong source bridge",
    )
    need(
        compiler["runtime_inputs_manifest_sha256"] == bridge["original_manifest_sha256"],
        "Wrong compiler inputs",
    )
    need(
        package["compiler_receipt_sha256"] == digest(HERE / "final2-own-compiler-receipt.json"),
        "Wrong package/compiler binding",
    )
    need(
        desktop["package_provenance_sha256"] == digest(HERE / "package-provenance.json"),
        "Wrong desktop/package binding",
    )
    need(
        desktop["source_bridge_sha256"]
        == digest(HERE / "final2-source-patch-correlation-bridge.json"),
        "Wrong desktop/source bridge",
    )
    need(
        bridge["corrected_source_patch_sha256"]
        == package["production_patch_sha256"]
        == desktop["source_patch_sha256"],
        "Inconsistent complete patch identity",
    )
    need(
        source["parent_head"] == package["source_parent"] == desktop["source_parent"],
        "Inconsistent parent",
    )
    need(
        package["unsigned_sha256"]
        == compiler["executable_sha256"]
        == desktop["unsigned_default_executable_sha256"],
        "Inconsistent compiler output",
    )
    need(
        package["signed_sha256"] == desktop["signed_executable_sha256"],
        "Inconsistent signed output",
    )
    need(len(source["all_review_inputs"]) == 1226, "Unexpected reviewed-input count")
    for path, expected in source["all_review_inputs"].items():
        need(digest(ROOT / path) == expected, f"Changed reviewed input: {path}")
    need(len(source["runtime_inputs"]) == 1068, "Unexpected runtime-input count")
    for path, expected in source["runtime_inputs"].items():
        need(digest(ROOT / path) == expected, f"Changed runtime input: {path}")
    need(len(bridge["paths"]) == 34, "Unexpected complete patch path count")
    for path, record in bridge["paths"].items():
        need(
            digest(ROOT / path) == record["applied_patch_blob_sha256"],
            f"Changed integration path: {path}",
        )
    need(len(compiler["own_compiler_artifacts"]) == 17, "Unexpected own artifact count")
    need(
        all(not artifact["fresh"] for artifact in compiler["own_compiler_artifacts"]),
        "An own artifact was reused",
    )
    prefix = source["worktree"] + "/"
    for depfile in compiler["depfile_hashes"]:
        for path, expected in depfile["resolved_current_absolute_inputs"].items():
            need(path.startswith(prefix), "Foreign-worktree compiler input")
            need(
                digest(ROOT / path.removeprefix(prefix)) == expected,
                f"Changed depfile input: {path}",
            )
    for path, record in compiler["additional_embedded_asset_inputs_matching_parent"].items():
        need(digest(ROOT / path) == record["sha256"], f"Changed embedded asset: {path}")
    smoke = load("signed-offline-naming-smoke.json")
    need(smoke["signed_executable_sha256"] == package["signed_sha256"], "Wrong offline worker")
    need(
        smoke["empty_PATH"] and smoke["java_python_helper_environment_removed"],
        "Wrong offline environment",
    )
    need(
        "(deny network*)" in smoke["sandbox_policy"] and len(smoke["cases"]) == 5,
        "Wrong recorded offline scope",
    )
    baseline = load("baseline-import-source-provenance.json")
    before = load("preimplementation-unique-native-verification.json")
    alias = load("baseline-unique-package-provenance.json")
    need(
        before["source_parity_receipt_sha256"]
        == digest(HERE / "baseline-import-source-provenance.json"),
        "Wrong baseline source binding",
    )
    need(
        before["original_signed_executable_sha256"] == baseline["signed_executable_sha256"],
        "Wrong baseline executable",
    )
    need(
        before["package_bridge_sha256"] == digest(HERE / "baseline-unique-package-provenance.json"),
        "Wrong unique baseline bridge",
    )
    need(
        alias["original_signed_executable_sha256"] == baseline["signed_executable_sha256"],
        "Wrong original baseline package",
    )
    need(
        alias["new_signed_executable_sha256"] == before["signed_executable_sha256"],
        "Wrong unique baseline package",
    )
    need(
        alias["copy_byte_equal_before_metadata_and_signing"]
        and alias["all_other_bundle_files_unchanged"],
        "Baseline package copy differs",
    )
    need(
        set(alias["changed_bundle_paths"]) == {"Contents/Info.plist", "Contents/MacOS/reshiki"},
        "Unexpected baseline package changes",
    )
    need(len(baseline["source_sha256"]) == 1063, "Unexpected baseline source count")
    need(
        baseline["retained_build_log_sha256"] == digest(HERE / "baseline-projection-build.log"),
        "Wrong baseline build log",
    )
    for path, expected in baseline["import_ui_paths_byte_identical_to_declared_parent"].items():
        need(
            baseline["source_sha256"][path] == expected, "Baseline UI source parity record differs"
        )
    print(
        "PASS: recorded artifact bytes, 1,226 reviewed/1,068 runtime inputs, 34 integration paths, 17 fresh compiler artifacts and source/package/desktop bindings"
    )


if __name__ == "__main__":
    verify()
