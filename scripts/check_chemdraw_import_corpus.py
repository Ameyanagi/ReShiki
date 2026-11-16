#!/usr/bin/env python3
"""Exercise generated ChemDraw files against ReShiki's production Rust importer."""

import argparse
import json
import subprocess
import time
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]


def first_difference(before, after, path="", tolerance=0.1):
    """Allow <0.035pt geometry rounding in the exported text baseline/16.16 data."""
    if isinstance(before, (float, int)) and not isinstance(before, bool):
        if isinstance(after, (float, int)) and abs(before - after) <= tolerance:
            return None
    elif isinstance(before, dict) and isinstance(after, dict):
        if before.keys() == after.keys():
            for key in before:
                difference = first_difference(before[key], after[key], f"{path}.{key}", tolerance)
                if difference:
                    return difference
            return None
    elif isinstance(before, list) and isinstance(after, list) and len(before) == len(after):
        for index, (left, right) in enumerate(zip(before, after, strict=True)):
            difference = first_difference(left, right, f"{path}[{index}]", tolerance)
            if difference:
                return difference
        return None
    elif before == after:
        return None
    return f"{path}: {before!r} -> {after!r}"


def drawing_origin(document):
    points = [atom["position"] for atom in document.get("atoms", [])]
    if not points:
        points = [caption["position"] for caption in document.get("annotations", [])]
        points += [arrow[key] for arrow in document.get("arrows", []) for key in ("start", "end")]
    return {axis: min((point[axis] for point in points), default=0) for axis in ("x", "y")}


def drawing_semantics(document, ordered_atoms=False):
    """Ignore generated IDs and global page translation, preserving visible data."""
    atoms = document.get("atoms", [])
    origin = drawing_origin(document)

    def point_key(point):
        return tuple(round(point[axis] - origin[axis], 1) for axis in ("x", "y"))

    if not ordered_atoms:
        atoms = sorted(atoms, key=lambda atom: (*point_key(atom["position"]), atom["element"]))
    atom_ids = {atom["id"]: index for index, atom in enumerate(atoms)}
    graphic_ids = {
        graphic["id"]: index for index, graphic in enumerate(document.get("graphics", []))
    }

    def position(point):
        return {axis: point[axis] - origin[axis] for axis in ("x", "y")}

    projected_atoms = []
    style = document.get("drawing_style", {})
    default_text = dict(
        family=style.get("font_family", "Arial"),
        size_pt=style.get("font_size_pt", 10),
        bold=False,
        italic=False,
        underline=False,
        color=[0, 0, 0],
        script="normal",
        formula=False,
    )
    for atom in atoms:
        projected = {
            key: atom.get(key)
            for key in [
                "element",
                "charge",
                "radical_electrons",
                "isotope",
                "aromatic",
                "stereo",
                "map_num",
                "text_style",
                "color",
            ]
        }
        projected["position"] = position(atom["position"])
        projected["text_style"] = atom.get("text_style") or default_text
        projected_atoms.append(projected)
    bonds = []
    for bond in document.get("bonds", []):
        bonds.append(
            {
                **{
                    key: bond.get(key)
                    for key in ["order", "display", "stereo", "double_position", "color"]
                },
                "a": atom_ids[bond["a"]],
                "b": atom_ids[bond["b"]],
            }
        )
    bonds.sort(key=lambda bond: (bond["a"], bond["b"]))
    captions = [
        {
            "text": caption["text"],
            "position": position(caption["position"]),
            "format": caption["format"],
        }
        for caption in sorted(
            document.get("annotations", []),
            key=lambda item: (item["text"], point_key(item["position"])),
        )
    ]

    def arrow_points(value):
        if isinstance(value, dict):
            if value.keys() == {"x", "y"}:
                return position(value)
            return {key: arrow_points(item) for key, item in value.items() if key != "id"}
        if isinstance(value, list):
            return [arrow_points(item) for item in value]
        return value

    generic = json.loads(json.dumps(document.get("generic", {})))
    for kind in ("alternatives", "links"):
        for site in generic.get(kind, []):
            site["atom"] = atom_ids[site["atom"]]
    for unit in generic.get("repeats", []):
        for key in ("bracket",):
            if unit.get(key) is not None:
                unit[key] = graphic_ids[unit[key]]
        for key in ("bracket_objects", "crossing_brackets"):
            if key in unit:
                unit[key] = [graphic_ids[identifier] for identifier in unit[key]]
        unit["members"] = [atom_ids[identifier] for identifier in unit["members"]]
        for crossing in unit["crossings"]:
            for key in ("inside", "outside"):
                crossing[key] = atom_ids[crossing[key]]
    arrows = arrow_points(document.get("arrows", []))
    arrows.sort(key=lambda arrow: (round(arrow["start"]["x"], 1), round(arrow["start"]["y"], 1)))
    return {
        "atoms": projected_atoms,
        "bonds": bonds,
        "captions": captions,
        "arrows": arrows,
        "generic": generic,
    }


def drawing_semantic_difference(before, after):
    """Match atoms geometrically so rounded sort-key boundaries cannot swap them."""
    first, second = before.get("atoms", []), after.get("atoms", [])
    if len(first) != len(second):
        return f"atom count: {len(first)} -> {len(second)}"
    origins = [drawing_origin(document) for document in (before, after)]
    available, matched = list(second), []
    for atom in first:
        distances = [
            (
                max(
                    abs(
                        atom["position"][axis]
                        - origins[0][axis]
                        - candidate["position"][axis]
                        + origins[1][axis]
                    )
                    for axis in ("x", "y")
                ),
                index,
            )
            for index, candidate in enumerate(available)
            if candidate["element"] == atom["element"]
        ]
        distance, index = min(distances, default=(float("inf"), 0))
        if distance > 0.1:
            return f"atom {atom['element']} at {atom['position']} has no matching position/element"
        matched.append(available.pop(index))
    return first_difference(
        drawing_semantics(before, ordered_atoms=True),
        drawing_semantics({**after, "atoms": matched}, ordered_atoms=True),
    )


def style_changes(before, after):
    changes = []
    for key, value in before.items():
        if key == "name":
            continue
        restored = after.get(key)
        if (
            isinstance(value, (int, float))
            and not isinstance(value, bool)
            and isinstance(restored, (int, float))
        ):
            # CDX coordinates are 16.16; font sizes use twentieths of a point.
            tolerance = {"font_size_pt": 0.05, "bond_spacing_ratio": 0.0005}.get(key, 0.0001)
            changed = abs(value - restored) > tolerance
        else:
            changed = value != restored
        if changed:
            changes.append(f"{key}: {value} -> {restored}")
    return changes


def assess(case, variant, result):
    findings = []
    if result.get("probe_error"):
        return [{"kind": "probe_error", "detail": result["probe_error"]}]
    if result.get("mode") == "style":
        style = result["style"]
        if style["import"]["status"] != "ok":
            return [{"kind": "style_import_rejected", "detail": style["import"].get("error")}]
        if style["export"]["status"] != "ok" or style["export"].get("write_error"):
            findings.append(
                {
                    "kind": "style_export_rejected",
                    "detail": style["export"].get("error", style["export"].get("write_error")),
                }
            )
        elif style.get("reimport", {}).get("status") != "ok":
            findings.append(
                {
                    "kind": "style_reimport_rejected",
                    "detail": style.get("reimport", {}).get("error"),
                }
            )
        else:
            for change in style_changes(style["import"]["style"], style["reimport"]["style"]):
                findings.append({"kind": "style_roundtrip_changed", "detail": change})
        expected_style = case.get("expected_style", {})
        settings = expected_style.get("settings", {})
        keys = {
            "BondLength": "bond_length_pt",
            "LineWidth": "line_width_pt",
            "BoldWidth": "bold_width_pt",
            "MarginWidth": "margin_width_pt",
            "HashSpacing": "hash_spacing_pt",
            "LabelSize": "font_size_pt",
        }
        expected_values = {
            target: float(settings[source]) for source, target in keys.items() if source in settings
        }
        if "BondSpacing" in settings:
            expected_values["bond_spacing_ratio"] = float(settings["BondSpacing"]) / 100
        if "font_family" in expected_style:
            expected_values["font_family"] = expected_style["font_family"]
        for change in style_changes(expected_values, style["import"]["style"]):
            findings.append({"kind": "style_input_mismatch", "detail": change})
        return findings

    imported = result["import"]
    expected = case["expected_import"]
    if imported["status"] != "ok":
        if expected["outcome"] == "accept":
            findings.append({"kind": "import_rejected", "detail": imported.get("error")})
        return findings
    if expected["outcome"] == "reject":
        findings.append(
            {
                "kind": "unexpected_acceptance",
                "detail": "Input expected to be rejected was accepted",
            }
        )
    warnings = " ".join(imported.get("warnings", [])).lower()
    for text in expected.get("required_warning_substrings", []):
        if text.lower() not in warnings:
            findings.append(
                {"kind": "missing_import_notice", "detail": f"Expected notice containing {text!r}"}
            )
    for key, value in expected.get("counts", {}).items():
        actual = imported["counts"].get("text" if key == "annotations" else key)
        if actual is None:
            continue
        matches = (
            value.get("min", actual) <= actual <= value.get("max", actual)
            if isinstance(value, dict)
            else actual == value
        )
        if not matches:
            findings.append(
                {
                    "kind": "import_count_mismatch",
                    "detail": f"{key}: expected {value}, got {actual}",
                }
            )
    snapshot = result.get("artifacts", {}).get("snapshot", {})
    imported_document = None
    if snapshot.get("status") == "ok":
        document = imported_document = json.loads(Path(snapshot["path"]).read_text())
        expected_text = Counter(expected.get("caption_texts", []))
        actual_text = Counter(a["text"] for a in document.get("annotations", []))
        if "caption_texts" in expected and expected_text != actual_text:
            findings.append(
                {
                    "kind": "caption_text_mismatch",
                    "detail": f"Missing {list((expected_text - actual_text).elements())}; extra {list((actual_text - expected_text).elements())}",
                }
            )
    else:
        findings.append(
            {"kind": "snapshot_write_failed", "detail": snapshot.get("error", "No import snapshot")}
        )
    for format_name in ("cdxml", "cdx", "cds"):
        if format_name not in result.get("roundtrips", {}):
            findings.append({"kind": "missing_roundtrip", "format": format_name})
    for format_name, roundtrip in result.get("roundtrips", {}).items():
        exported = roundtrip["export"]
        if exported["status"] != "ok" or exported.get("write_error"):
            findings.append(
                {
                    "kind": "export_rejected",
                    "format": format_name,
                    "detail": exported.get("error", exported.get("write_error")),
                }
            )
            continue
        reopened = roundtrip.get("reimport")
        if not reopened or reopened["status"] != "ok":
            findings.append(
                {
                    "kind": "export_reimport_rejected",
                    "format": format_name,
                    "detail": reopened.get("error") if reopened else "No reimport result",
                }
            )
            continue
        # Shapes can legitimately split into paths during editable exchange.
        # Chemical graph and caption/arrow/picture counts must remain stable.
        for key in (
            "atoms",
            "bonds",
            "text",
            "arrows",
            "pictures",
            "repeats",
            "alternatives",
            "links",
        ):
            before, after = imported["counts"].get(key), reopened["counts"].get(key)
            if before != after:
                findings.append(
                    {
                        "kind": "roundtrip_count_mismatch",
                        "format": format_name,
                        "detail": f"{key}: {before} -> {after}",
                    }
                )
        for change in style_changes(imported["drawing_style"], reopened["drawing_style"]):
            findings.append(
                {"kind": "drawing_style_roundtrip_changed", "format": format_name, "detail": change}
            )
        difference = first_difference(
            imported.get("graphic_bounds_relative_to_anchor"),
            reopened.get("graphic_bounds_relative_to_anchor"),
            "graphic_bounds",
        )
        if difference:
            findings.append(
                {
                    "kind": "roundtrip_graphic_geometry_changed",
                    "format": format_name,
                    "detail": difference,
                }
            )
        snapshot = roundtrip.get("snapshot", {})
        if snapshot.get("status") == "ok":
            document = json.loads(Path(snapshot["path"]).read_text())
            actual_text = Counter(a["text"] for a in document.get("annotations", []))
            if "caption_texts" in expected and Counter(expected["caption_texts"]) != actual_text:
                findings.append(
                    {
                        "kind": "roundtrip_caption_text_mismatch",
                        "format": format_name,
                        "detail": "Caption text changed",
                    }
                )
            if imported_document is not None:
                difference = drawing_semantic_difference(imported_document, document)
                if difference:
                    findings.append(
                        {
                            "kind": "roundtrip_drawing_semantics_changed",
                            "format": format_name,
                            "detail": difference,
                        }
                    )
        else:
            findings.append(
                {
                    "kind": "snapshot_write_failed",
                    "format": format_name,
                    "detail": snapshot.get("error", "No roundtrip snapshot"),
                }
            )
    return findings


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--manifest",
        type=Path,
        default=ROOT / "tests/fixtures/chemdraw-import-corpus/manifest.json",
    )
    parser.add_argument(
        "--output-dir", type=Path, default=ROOT / "artifacts/chemdraw-import-corpus"
    )
    parser.add_argument(
        "--probe", type=Path, help="Use an already compiled chemdraw_import_probe executable"
    )
    parser.add_argument("--jobs", type=int, choices=range(1, 9), default=4)
    parser.add_argument("--timeout", type=float, default=30, help="Seconds allowed per input")
    parser.add_argument(
        "--strict", action="store_true", help="Exit 1 for any finding, including export limitations"
    )
    args = parser.parse_args()
    manifest_path = args.manifest.resolve()
    manifest = json.loads(manifest_path.read_text())
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    probe = (
        args.probe.resolve() if args.probe else ROOT / "target/debug/examples/chemdraw_import_probe"
    )
    if not args.probe:
        subprocess.run(
            ["cargo", "build", "-p", "reshiki-io", "--example", "chemdraw_import_probe"],
            cwd=ROOT,
            check=True,
        )
    tasks = [
        (case, variant, manifest_path.parent / name)
        for case in manifest["cases"]
        for variant, name in case["files"].items()
        if (variant in ("cdxml", "cdx", "cds") or variant.startswith("style_cds")) and name
    ]

    def run(task):
        case, variant, path = task
        started = time.monotonic()
        result: dict[str, Any]
        try:
            command = [
                str(probe),
                "--output-dir",
                str(output / "exports" / case["id"] / variant),
                str(path),
            ]
            if variant == "cds":
                command.insert(1, "--drawing-cds")
            completed = subprocess.run(
                command,
                capture_output=True,
                text=True,
                timeout=args.timeout,
            )
            if completed.returncode:
                result = {
                    "path": str(path),
                    "probe_error": f"Exit {completed.returncode}: {completed.stderr.strip()}",
                }
            else:
                result = json.loads(completed.stdout)
        except subprocess.TimeoutExpired:
            result = {"path": str(path), "probe_error": f"Timed out after {args.timeout:g}s"}
        except (OSError, json.JSONDecodeError) as error:
            result = {"path": str(path), "probe_error": str(error)}
        result.update(
            case_id=case["id"],
            category=case["category"],
            classification=case["classification"],
            variant=variant,
        )
        result["findings"] = assess(case, variant, result)
        result["wall_seconds"] = round(time.monotonic() - started, 3)
        return result

    with ThreadPoolExecutor(max_workers=args.jobs) as executor:
        results = list(executor.map(run, tasks))
    with (output / "results.jsonl").open("w") as stream:
        for result in results:
            stream.write(json.dumps(result, ensure_ascii=False) + "\n")
    findings = [
        {"case_id": r["case_id"], "variant": r["variant"], **f}
        for r in results
        for f in r["findings"]
    ]
    summary = {
        "source_commit": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
        ).strip(),
        "source_note": "Production Rust importer built from the working tree. Input provenance is recorded in the manifest.",
        "input_provenance": manifest.get("provenance"),
        "seed": manifest.get("seed"),
        "extra_cases": manifest.get("extra_cases", 0),
        "cases": len(manifest["cases"]),
        "inputs": len(results),
        "drawing_imports_ok": sum(r.get("import", {}).get("status") == "ok" for r in results),
        "drawing_imports_rejected": sum(
            r.get("import", {}).get("status") == "error" for r in results
        ),
        "drawing_imports_by_format": dict(
            Counter(r["format"] for r in results if r.get("mode") == "drawing")
        ),
        "drawing_roundtrips_ok_by_format": dict(
            Counter(
                format_name
                for result in results
                for format_name, roundtrip in result.get("roundtrips", {}).items()
                if roundtrip["export"]["status"] == "ok"
                and roundtrip.get("reimport", {}).get("status") == "ok"
            )
        ),
        "style_imports_ok": sum(
            r.get("style", {}).get("import", {}).get("status") == "ok" for r in results
        ),
        "probe_errors": sum(bool(r.get("probe_error")) for r in results),
        "findings_by_kind": dict(Counter(f["kind"] for f in findings)),
        "findings": findings,
    }
    (output / "summary.json").write_text(json.dumps(summary, indent=2, ensure_ascii=False) + "\n")
    print(json.dumps({k: v for k, v in summary.items() if k != "findings"}, indent=2))
    print(f"Full results: {output / 'results.jsonl'}")
    print(f"Findings: {output / 'summary.json'}")
    return int(args.strict and bool(findings))


if __name__ == "__main__":
    raise SystemExit(main())
