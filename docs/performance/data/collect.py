#!/usr/bin/env python3
"""Rebuild the committed aggregate dataset from completed local benchmark logs.

Run from the repository root. Raw per-iteration samples were not emitted by the
harnesses; this preserves every reported aggregate, without reconstructing them.
"""

import csv
import hashlib
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "artifacts/performance"
DEST = Path(__file__).resolve().parent
COMMITS = {
    "stable_v091": "167d893c5007ea665456652897791844c5ef9fce",
    "original_nightly": "ae5ec464ace991ec498bb1acb155635cc7262c1a",
    "stage1_canvas": "f88d2c472675235886dada1b96b5438cde700aee",
    "stage1_docs": "c9f2c257a4a6027f14afccc373894286dd34aac2",
    "stage2_editing": "f31ceea4a9d9062c58a504732598b9f0793edfee",
    "stage2_docs": "805b9fbf216ac90804244799568739881974aae7",
    "stage3_background_zoom": "e9818f5132215fdf308293163d41a066c49d2d8a",
    "current": "ee2ce1afe2d56fb58794d9a6872c3988cb9b67ba",
}
# These are the complete successful runs used in the three stage reports. Failed,
# superseded trial and profiler-attached runs are not mixed into timings.
HISTORICAL = [
    ("baseline-canvas.log", "macos", "original_nightly", "canvas"),
    ("windows-baseline-canvas.log", "windows", "original_nightly", "canvas"),
    ("after-canvas-final.log", "macos", "stage1_canvas", "canvas"),
    ("windows-after-canvas.log", "windows", "stage1_canvas", "canvas"),
    ("editing-before.log", "macos", "stage1_docs", "editing"),
    ("windows-editing-before.log", "windows", "stage1_docs", "editing"),
    ("editing-after.log", "macos", "stage2_editing", "editing"),
    ("reshiki-perf-editing-after.log", "windows", "stage2_editing", "editing"),
    ("zoom-followup-before.log", "macos", "stage2_docs", "canvas"),
    ("windows-broader-zoom-before.log", "windows", "stage2_docs", "canvas"),
    ("zoom-followup-after.log", "macos", "stage3_background_zoom", "canvas"),
    ("windows-broader-zoom-after.log", "windows", "stage3_background_zoom", "canvas"),
    ("inspector-before-adjacent.log", "macos", "stage2_docs", "inspector"),
    ("inspector-after.log", "macos", "stage3_background_zoom", "inspector"),
    ("windows-broader-inspector.log", "windows", "stage3_background_zoom", "inspector"),
    ("broader-autosave.log", "macos", "stage3_background_zoom", "recovery"),
    ("windows-broader-autosave.log", "windows", "stage3_background_zoom", "recovery"),
    ("broader-files.log", "macos", "stage3_background_zoom", "files"),
    ("windows-broader-files.log", "windows", "stage3_background_zoom", "files"),
    ("broader-themes.log", "macos", "rejected_theme_experiment", "themes"),
]


def decode(data):
    if data.startswith((b"\xff\xfe", b"\xfe\xff")):
        return data.decode("utf-16"), "UTF-16"
    return data.decode("utf-8-sig"), "UTF-8"


def main():
    specs = [("historical", *item) for item in HISTORICAL]
    for platform in ["macos", "windows"]:
        for source, checkpoint in [("v091", "stable_v091"), ("head", "current")]:
            for workload in ["canvas", "editing", "inspector", "recovery", "files"]:
                if source != "head" and workload == "files":
                    continue  # No historical file/library harness was run.
                specs.append(
                    (
                        "matched_release_current",
                        f"history-{platform}-{source}-{workload}.log",
                        platform,
                        checkpoint,
                        workload,
                    )
                )
    records, runs, unavailable = [], [], []
    previous = {}
    if (DEST / "manifest.json").exists():
        previous = {
            run["run_id"]: run for run in json.loads((DEST / "manifest.json").read_text())["runs"]
        }
    for series, name, platform, checkpoint, harness in specs:
        path = SOURCE / name
        archived = not path.exists()
        if archived:
            path = DEST / "logs" / name
        if not path.exists():
            unavailable.append(name)
            continue
        original = path.read_bytes()
        raw, encoding = decode(original)
        if not re.search(r"test result: ok\. 1 passed; 0 failed;", raw):
            unavailable.append(name + " (incomplete)")
            continue
        # Retain benchmark stdout and the successful result, without local paths,
        # compiler output or PowerShell's wrapper for native stderr.
        raw = raw[raw.index("running 1 test") :].strip() + "\n"
        raw = raw.replace("\r\n", "\n")
        out = DEST / "logs" / name
        out.write_text(raw)
        notes = []
        if checkpoint == "rejected_theme_experiment":
            series = "rejected_experiment"
            notes.append(
                "Uncommitted trial based on stage2; rejected and absent from final runtime."
            )
        if series == "historical" and platform == "windows":
            renderer = "automatic_selection_not_recorded"
            notes.append(
                "Exact selected headless renderer was not logged; do not pool with the forced tiny-skia matched series."
            )
        elif platform == "macos":
            renderer = "default_wgpu"
            notes.append("macOS build enables wgpu only; selected adapter was not logged.")
        elif harness == "canvas":
            renderer = "tiny-skia"
        else:
            renderer = "not_rendered"
        if harness != "canvas":
            renderer = "not_rendered"
        if (
            series == "historical"
            and platform == "macos"
            and checkpoint == "stage3_background_zoom"
        ):
            notes.append(
                "Candidate working-tree run of this optimization stage before its final commit; exact sampled tree was not separately committed."
            )
        run_id = name.removesuffix(".log")
        run = dict(
            run_id=run_id,
            series=series,
            platform=platform,
            checkpoint=checkpoint,
            source_commit=COMMITS.get(checkpoint, ""),
            harness=harness,
            renderer=renderer,
            date="2026-09-29",
            warmups=3,
            log=f"logs/{name}",
            source_encoding=encoding,
            source_sha256=hashlib.sha256(original).hexdigest(),
            log_sha256=hashlib.sha256(out.read_bytes()).hexdigest(),
            notes=notes,
        )
        if archived and run_id in previous:
            for key in ["source_encoding", "source_sha256"]:
                run[key] = previous[run_id][key]
        selected = re.findall(r"^.*(?:renderer|Renderer).*$", raw, flags=re.MULTILINE)
        if selected:
            run["renderer_output"] = selected
        count = 0
        for line in raw.splitlines():
            if match := re.fullmatch(r"PERF,([^,]+),(\d+),([\d.]+),([\d.]+),([\d.]+)", line):
                workload, iterations, median, p95, mean = match.groups()
            elif match := re.fullmatch(r"([^,]+),([\d.]+),([\d.]+)", line):
                workload, median, p95 = match.groups()
                iterations, mean = "30", ""
            elif match := re.fullmatch(r"(.+): median ([\d.]+) ms, p95 ([\d.]+) ms", line):
                workload, median, p95 = match.groups()
                iterations, mean = "30", ""
            else:
                continue
            canonical = workload.replace("ethanol_", "methanol_")
            record = {
                key: run[key]
                for key in [
                    "run_id",
                    "series",
                    "platform",
                    "checkpoint",
                    "source_commit",
                    "harness",
                    "renderer",
                    "date",
                    "warmups",
                ]
            }
            record.update(
                workload=canonical,
                workload_original=workload,
                iterations=iterations,
                median_ms=median,
                p95_ms=p95,
                mean_ms=mean,
                source_log=run["log"],
            )
            records.append(record)
            count += 1
        if not count:
            raise ValueError(f"No measurements found in completed run {name}")
        run["measurement_rows"] = count
        runs.append(run)
    with (DEST / "measurements.csv").open("w", newline="") as file:
        fields = [
            "run_id",
            "series",
            "platform",
            "checkpoint",
            "source_commit",
            "date",
            "harness",
            "renderer",
            "workload",
            "workload_original",
            "warmups",
            "iterations",
            "median_ms",
            "p95_ms",
            "mean_ms",
            "source_log",
        ]
        writer = csv.DictWriter(file, fieldnames=fields)
        writer.writeheader()
        writer.writerows(records)
    metadata = {
        "schema_version": 1,
        "description": "All reported aggregate rows from the selected completed checkpoint runs, not raw per-iteration observations.",
        "commits": COMMITS,
        "fixture": {
            "path": "assets/examples/shortcut-examples.rsk",
            "git_blob": "5f05afc369699c0ec00ec4e55d8d255a6bf1ef19",
            "unchanged_across_all_checkpoints": True,
        },
        "machines": {
            "macos": {
                "cpu": "Apple M4",
                "ram_gib": 32,
                "os": "macOS 26.5.1",
                "build": "Rust release",
            },
            "windows": {
                "cpu": "AMD Ryzen 9 7940HS test VM",
                "os": "Windows 11",
                "target": "x86_64-pc-windows-msvc",
                "build": "Rust release",
                "caveat": "Remote/software display adapters; not the physical Intel Iris Xe GPU.",
            },
        },
        "notes": [
            "Canvas measures actual Program::draw/mouse_interaction preparation, not GPU presentation or FPS.",
            "The oxygen_hotkey row is an O then C edit pair. It excludes background task execution and rendering.",
            "Historical ethanol_* row names were a harness typo: the fixture is the same two-atom methanol fragment; workload_original preserves the raw name.",
            "gallery_0x in editing is the two-atom fragment alone; gallery_1x/4x editing includes that extra fragment.",
            "Inspector workloads add one selected-capable O atom and include no/one/all-selected and collapsed/expanded sections; view differs from panel.",
            "Recovery tick in v0.9.1 includes synchronous durable save. Current dispatch returns before worker/disk completion; roundtrip is measured separately.",
            "Recovery synchronous on the updated build is a synchronous cost control, not a historical app benchmark.",
            "The pre-change profile-attached runs and superseded trials are excluded from reported timing comparisons.",
            "The theme experiment is retained as rejected_experiment evidence; neither timing is claimed as a final implementation gain.",
        ],
        "runs": runs,
        "unavailable_runs": unavailable,
    }
    (DEST / "manifest.json").write_text(json.dumps(metadata, indent=2, ensure_ascii=False) + "\n")
    print(
        f"Wrote {len(records)} measurements from {len(runs)} completed runs; {len(unavailable)} not yet available"
    )


if __name__ == "__main__":
    main()
