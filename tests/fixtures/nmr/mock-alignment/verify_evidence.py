#!/usr/bin/env python3
"""Verify the fixed NMR publication packet without Cargo, GUI or dependencies."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path, PurePosixPath
from typing import Any

PACKET = PurePosixPath("tests/fixtures/nmr/mock-alignment")
IMAGES = PurePosixPath("docs/images/nmr/mock-alignment")
FROZEN_SHA = "30dba3b57746dc1cec7a16ff4aa95c34a6a1d0c6139282b0f11a2c9ee9bbad14"
SAVE_SHA = "08978db95416b8f58df2d8b12cbe949328327c8ddc0d0a75c7e32668e0f14cca"
TSV_SHA = "23464aba142911f3fb3c635069f4bc8f2a8c0028934c6925d3065c036058b876"
SIGNED_SHA = "f35b6519387059084e658fbf1b0ee9e205b14f01f6ee5f0eb644b20a85aa8851"
CLA_SHA = "db03e96541729688e1aed987119a1502ee14ec33349fbf8d4773ad8f0d421c84"
POST_BUILD = {"docs/nmr-prediction.md", ".github/workflows/checks.yml"}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read(root: Path, relative: str | PurePosixPath) -> bytes:
    path = PurePosixPath(relative)
    require(not path.is_absolute() and ".." not in path.parts, f"Unsafe path: {relative}")
    return root.joinpath(*path.parts).read_bytes()


def load(root: Path, relative: str | PurePosixPath) -> Any:
    return json.loads(read(root, relative))


def verify(root: Path, app: Path | None = None) -> dict[str, Any]:
    evidence = PACKET / "evidence"
    index = load(root, evidence / "index.json")
    require(index["schema"] == 1, "Unexpected index schema")
    for name, record in index["files"].items():
        data = read(root, name)
        require(
            len(data) == record["bytes"] and digest(data) == record["sha256"],
            f"Public bytes changed: {name}",
        )
    for name, expected in index["authored_sha256"].items():
        require(digest(read(root, name)) == expected, f"Authored file changed: {name}")

    manifest_bytes = read(root, evidence / "build/source-manifest.json")
    require(digest(manifest_bytes) == FROZEN_SHA, "Accepted frozen manifest changed")
    manifest = json.loads(manifest_bytes)
    require(len(manifest) == 2592, "Expected 2592 frozen inputs")
    require(
        set(index["post_build_existing_sha256"]) == POST_BUILD,
        "Unexpected post-build existing-file allowlist",
    )
    changed = []
    for name, original in manifest.items():
        current = digest(read(root, name))
        if current != original:
            changed.append(name)
            require(
                name in POST_BUILD and current == index["post_build_existing_sha256"][name],
                f"Frozen source/data/license changed: {name}",
            )
    require(set(changed) == POST_BUILD, "Expected exactly the documented two-file delta")

    provenance = load(root, evidence / "build/package/final-app-provenance.json")
    dependencies = load(root, evidence / "build/build/dependency-inputs.json")
    require(len(dependencies) == 568, "Expected 568 recorded shipping inputs")
    old_root = PurePosixPath(provenance["worktree"])
    for absolute, recorded in dependencies.items():
        relative = PurePosixPath(absolute).relative_to(old_root)
        require(recorded["kind"] == "frozen-worktree", "Foreign dependency input")
        require(
            manifest[str(relative)] == recorded["sha256"] == digest(read(root, relative)),
            f"Shipping input changed: {relative}",
        )
    build = load(root, evidence / "build/build/result.json")
    require(
        build["exit_code"] == 0
        and build["own_artifact_count"] == 15
        and build["all_own_fresh_false"]
        and not build["foreign_worktree_artifacts"]
        and build["default_features_enabled"],
        "Accepted fresh default build mismatch",
    )
    require(
        build["source_manifest_sha256"] == FROZEN_SHA == provenance["source_manifest_sha256"],
        "Build/source bridge mismatch",
    )
    require(
        provenance["signed_executable_sha256"] == SIGNED_SHA, "Accepted signed executable mismatch"
    )
    for group, count in (("app", 29), ("canvas", 9), ("markers", 1), ("cache", 3)):
        result = load(root, evidence / f"build/focused-{group}/result.json")
        require(
            result["exit_code"] == result["failed"] == result["ignored"] == 0
            and result["passed"] == count
            and len(result["listed_names"]) == count
            and result["source_manifest_sha256"] == FROZEN_SHA,
            f"Accepted focused-{group} result mismatch",
        )
        require(
            sorted(result["named_results"])
            == sorted([name, "ok"] for name in result["listed_names"]),
            f"Named focused-{group} results mismatch",
        )
        log = read(root, evidence / f"build/focused-{group}/tests.log").decode()
        actual = re.findall(r"^test (\S+) \.\.\. ok$", log, re.MULTILINE)
        require(
            sorted(actual) == sorted(result["listed_names"]), f"Named focused-{group} log mismatch"
        )
    for gate in ("clippy", "default-check", "format"):
        result = load(root, evidence / f"build/{gate}/result.json")
        require(
            result["exit_code"] == 0 and result["source_manifest_sha256"] == FROZEN_SHA,
            f"Accepted {gate} gate mismatch",
        )

    native = load(root, evidence / "final-native-acceptance.json")
    artifact = load(root, evidence / "final-artifact-acceptance.json")
    independent = load(root, evidence / "native-acceptance-audit.json")
    require(
        native["status"] == artifact["status"] == independent["status"] == "PASS",
        "Acceptance receipt mismatch",
    )
    require(
        native["app"]["signed_executable_sha256"] == SIGNED_SHA
        and native["app"]["provenance"]["sha256"]
        == digest(read(root, evidence / "build/package/final-app-provenance.json")),
        "Native/build provenance mismatch",
    )
    require(
        native["build_acceptance"]["sha256"]
        == digest(read(root, evidence / "final-artifact-acceptance.json"))
        and independent["root_native_receipt"]["sha256"]
        == digest(read(root, evidence / "final-native-acceptance.json")),
        "Independent receipt bridge mismatch",
    )
    require(len(native["captures"]) == 15, "Expected 15 raw native AFTER captures")
    for capture in native["captures"]:
        image = read(root, IMAGES / PurePosixPath(capture["path"]).name)
        ax = read(root, evidence / PurePosixPath(capture["ax"]["path"]).name)
        require(
            image.startswith(b"\xff\xd8\xff")
            and digest(image) == capture["sha256"]
            and digest(ax) == capture["ax"]["sha256"],
            "Native capture receipt mismatch",
        )
        for action in ("Undo", "Redo"):
            require(f"button (disabled) {action}" in ax.decode(), "Native history changed")
    comparison = native["controlled_comparison"]
    require(
        comparison["manual_zoom_percent"] == 160 and comparison["keyboard_drawing"] == "off",
        "Primary camera/state mismatch",
    )
    for key, directory in (
        ("before", IMAGES),
        ("before_ax", evidence),
        ("after", IMAGES),
        ("after_ax", evidence),
    ):
        data = read(root, directory / PurePosixPath(comparison[key]["path"]).name)
        require(digest(data) == comparison[key]["sha256"], "Primary native bytes mismatch")
    before = read(root, evidence / PurePosixPath(comparison["before_ax"]["path"]).name).decode()
    after = read(root, evidence / PurePosixPath(comparison["after_ax"]["path"]).name).decode()
    for ax in (before, after):
        require(
            "4 of 4 atom-linked sites supported" in ax
            and "Value: on, ID: nmr.site.13C.2" in ax
            and "Value: on, ID: nmr.labels" in ax,
            "Primary AX report/selection mismatch",
        )
    for atom, shift in ((1, "14.200"), (2, "61.050"), (4, "170.700"), (5, "20.900")):
        rows = [line for line in after.splitlines() if f"ID: nmr.site.13C.{atom}," in line]
        require(
            len(rows) == 1 and f"Predicted {shift} ppm" in rows[0],
            f"Primary AX atom #{atom} row mismatch",
        )
    require(
        "Value: Atom numbers, ID: nmr.labels.mode" in after, "Primary AX label dropdown mismatch"
    )
    original = read(root, evidence / "ethyl-acetate-nmr-v3-native.rsk")
    saved = read(root, evidence / "ethyl-acetate-nmr-final-native.rsk")
    require(
        original == saved and len(saved) == 3377 and digest(saved) == SAVE_SHA,
        "Whole-byte input/Save-as equality failed",
    )
    drawing = json.loads(saved)
    require(
        drawing["version"] == 19 and len(drawing["atoms"]) == 6 and len(drawing["bonds"]) == 5,
        "Native serialized drawing mismatch",
    )
    tsv = read(root, evidence / "final-carbon13.tsv")
    require(
        tsv == read(root, PACKET.parent / "ethyl-acetate-carbon-desktop.tsv")
        and len(tsv) == 1322
        and digest(tsv) == TSV_SHA,
        "Native exact TSV equality failed",
    )
    guide = read(root, "docs/changes/nmr-floating-mock-20261010.md")
    suffix = guide[guide.index(b"## Contribution license\n") :]
    require(len(suffix) == 1064 and digest(suffix) == CLA_SHA, "Contribution license changed")
    bundle = load(root, evidence / "build/package/bundle-manifest.json")
    require(
        len(bundle) == 9 and bundle["Contents/MacOS/reshiki"] == SIGNED_SHA,
        "Accepted bundle map mismatch",
    )
    if app is not None:
        for relative, expected in bundle.items():
            require(
                digest(read(app, relative)) == expected, f"Preserved bundle changed: {relative}"
            )
    return {
        "status": "PASS",
        "original_public_files": len(index["files"]),
        "frozen_inputs": 2592,
        "post_build_existing_delta": sorted(changed),
        "shipping_inputs": 568,
        "named_tests": 42,
        "native_after_captures": 15,
        "saved_drawing_whole_bytes": 3377,
        "native_tsv_exact_bytes": 1322,
        "bundle_checked_now": app is not None,
        "published_head_ci_at_packet_capture": "PENDING",
        "normal_commit_hooks_at_packet_capture": "PENDING",
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[4])
    parser.add_argument("--app", type=Path, help="Optionally verify the preserved nine-file app")
    args = parser.parse_args()
    try:
        print(json.dumps(verify(args.root, args.app), indent=2))
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f"FAIL: {error}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
