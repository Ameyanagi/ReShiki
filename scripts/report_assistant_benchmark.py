"""Score retained native graphs independently and report every planned model run.

Completed drafts, provisional previews, clarification-only results, failed runs
and not-run cases have separate denominators. A model's visual review is never
used as a chemical reference or proof of correctness.
"""

import argparse
import json
import statistics
from collections import Counter
from pathlib import Path

from assistant_benchmark_evaluate import score_document


def read_json(path, default=None):
    return json.loads(path.read_text()) if path.exists() else default


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def metric_counts(score):
    if not score or not score.get("localized_metrics_available"):
        return None
    result = {
        key: len(score[key])
        for key in (
            "atom_errors",
            "hydrogen_errors",
            "missing_atoms",
            "extra_atoms",
            "bond_errors",
            "missing_bonds",
            "extra_bonds",
            "stereochemistry_errors",
            "missing_fragments",
            "extra_fragments",
        )
    }
    abbreviations = score["abbreviations"]
    result["missing_abbreviation_labels"] = sum(abbreviations["missing_display_labels"].values())
    result["extra_abbreviation_labels"] = sum(abbreviations["extra_display_labels"].values())
    result["abbreviation_member_errors"] = len(abbreviations["member_chemistry_errors"])
    return result


def report(manifest_path, output):
    manifest = read_json(manifest_path)
    metadata = read_json(output / "metadata.json")
    cases = {case["id"]: case for case in manifest["cases"]}
    rows = []
    for planned in metadata["plan"]:
        case = cases[planned["case"]]
        directory = output / case["id"] / f"run-{planned['repeat']}"
        run = read_json(directory / "run.json", {"status": "not_run", "reason": "No run artifact"})
        proposal = read_json(directory / "proposal.json", {})
        review = read_json(directory / "review.json", {})
        row = dict(planned, category=case["category"], **run)
        row["apply_validation"] = read_json(directory / "apply-validation.json")
        completed = directory / "drawing.rsk"
        provisional = directory / "last-preview.rsk"
        graph = completed if completed.exists() else provisional if provisional.exists() else None
        row["graph_artifact"] = str(graph.relative_to(output)) if graph else None
        row["graph_stage"] = "completed" if graph == completed else "provisional" if graph else None
        row["score"] = None
        if graph and case["reference"]:
            score = score_document(
                read_json(graph), read_json(manifest_path.parent / case["reference"])
            )
            row["score"] = score
            write_json(directory / "score.json", score)
        row["metric_counts"] = metric_counts(row["score"])
        if not case["reference"]:
            # Retain the actual evidence instead of treating any warning as a
            # verified recognition of missing or unreadable image information.
            row["ambiguity_evidence"] = {
                "explanation": proposal.get("explanation", ""),
                "review_issues": review.get("issues", []),
                "visual_review_verified": review.get("verified", False),
                "empty_completed_drawing": completed.exists()
                and not read_json(completed).get("atoms"),
                "policy": "No unique complete reference; inspect these messages manually. A warning or empty result is not automatically a correct clarification.",
            }
        rows.append(row)
    finite = [r for r in rows if cases[r["case"]]["reference"]]
    attempted = [r for r in rows if r["status"] != "not_run"]
    completed = [
        r for r in finite if r["status"] == "completed" and r["graph_stage"] == "completed"
    ]
    exact = [r for r in completed if r["score"] and r["score"]["graph_identity"]]
    latency = [r["elapsed_seconds"] for r in attempted if r.get("elapsed_seconds") is not None]
    totals = Counter()
    for row in completed:
        if row["metric_counts"]:
            totals.update(row["metric_counts"])
    provisional = [
        r for r in finite if r["graph_stage"] == "provisional" and r["status"] != "not_run"
    ]
    provisional_totals = Counter()
    for row in provisional:
        if row["metric_counts"]:
            provisional_totals.update(row["metric_counts"])
    repeated = {}
    for name in cases:
        actual = [r for r in rows if r["case"] == name and r["status"] != "not_run"]
        canonical = [
            r["score"]["observed_smiles"]
            for r in actual
            if r.get("score") and r["score"].get("observed_smiles")
        ]
        repeated[name] = {
            "attempted": len(actual),
            "statuses": [r["status"] for r in actual],
            "graph_stages": [r["graph_stage"] for r in actual],
            "observed_graphs": canonical,
            "distinct_observed_graphs": len(set(canonical)),
            "repeat_variation_assessable": len(canonical) > 1,
        }
    summary = {
        "planned": len(rows),
        "attempted": len(attempted),
        "statuses": dict(Counter(r["status"] for r in rows)),
        "finite_reference_planned": len(finite),
        "finite_reference_attempted": sum(r["status"] != "not_run" for r in finite),
        "finite_reference_completed": len(completed),
        "finite_reference_exact_completed": len(exact),
        "finite_reference_exact_applicable": sum(
            (r.get("apply_validation") or {}).get("accepted") is True for r in exact
        ),
        "finite_reference_completed_apply_validation": dict(
            Counter(
                "accepted"
                if validation.get("accepted") is True
                else "rejected"
                if validation.get("accepted") is False
                else "unavailable"
                for r in completed
                for validation in [r.get("apply_validation") or {}]
            )
        ),
        "provisional_exact": sum(
            bool(r.get("score", {}) and r["score"].get("graph_identity"))
            for r in finite
            if r["graph_stage"] == "provisional" and r["status"] != "not_run"
        ),
        "finite_reference_provisional": len(provisional),
        "provisional_localized_metric_runs": sum(
            r["metric_counts"] is not None for r in provisional
        ),
        "provisional_localized_error_totals": dict(provisional_totals),
        "completed_localized_metric_runs": sum(r["metric_counts"] is not None for r in completed),
        "completed_reference_adapter_failures": sum(
            bool(r["score"] and r["score"].get("reference_adapter_error")) for r in completed
        ),
        "completed_localized_error_totals": dict(totals),
        "attempt_latency_seconds": {
            "count": len(latency),
            "min": min(latency),
            "median": statistics.median(latency),
            "max": max(latency),
        }
        if latency
        else None,
        "token_usage": "Not exposed by this application backend; unavailable, not estimated.",
        "repeat_variation": repeated,
        "limitations": [
            "Small synthetic original cohort, not a real-scan or general chemistry accuracy estimate.",
            "Native candidate acceptance and model visual-review completion do not prove chemical graph identity.",
            "Provisional previews are scored separately and never counted as completed successes.",
            "No full haptic/variable/coordination reference policy; the rhodium handoff fixture is excluded.",
            "Strict protonation, tautomer and stereo identity; aromatic/Kekule and ordinary explicit/implicit H normalize.",
            "Error localization uses bounded common topology matching and is descriptive, not a unique edit-distance proof.",
        ],
    }
    result = {"metadata": metadata, "summary": summary, "runs": rows}
    write_json(output / "report.json", result)
    markdown = [
        "# Assistant image benchmark",
        "",
        f"Baseline `{metadata['backend_commit']}` / Nightly `{metadata['nightly_version']}`.",
        "",
        f"Requested `{metadata['model']}` / `{metadata['effort']}` / default tier; `{metadata['codex_version']}`. Each run records the effective configuration.",
        "",
        f"{len(attempted)}/{len(rows)} planned workflows attempted. Finite references: {len(exact)} exact completed graphs / {len(completed)} completed / {summary['finite_reference_attempted']} attempted / {len(finite)} planned.",
        "",
        "| Case | Repeat | Status | Seconds | Graph stage | Exact identity | Review issues |",
        "| --- | ---: | --- | ---: | --- | --- | ---: |",
    ]
    for row in rows:
        seconds = f"{row['elapsed_seconds']:.1f}" if row.get("elapsed_seconds") is not None else "—"
        score = row["score"]
        identity = "yes" if score and score["graph_identity"] else "no" if score else "—"
        issues = (row.get("review") or {}).get("issues", [])
        markdown.append(
            f"| {row['case']} | {row['repeat']} | {row['status']} | {seconds} | {row['graph_stage'] or '—'} | {identity} | {len(issues)} |"
        )
    markdown += [
        "",
        "Full atom/bond/stereo/fragment/abbreviation errors, messages, apply validation, repeated graph variation and configuration are in `report.json` and per-run `score.json`.",
        "",
        *[f"- {item}" for item in summary["limitations"]],
        "",
        summary["token_usage"],
        "",
    ]
    (output / "README.md").write_text("\n".join(markdown))
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = report(args.manifest, args.output)
    print(json.dumps(result["summary"], indent=2))
