import copy
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / "scripts"
spec = importlib.util.spec_from_file_location(
    "benchmark", SCRIPTS / "assistant_benchmark_evaluate.py"
)
benchmark = importlib.util.module_from_spec(spec)
spec.loader.exec_module(benchmark)
FIXTURES = ROOT / "tests/fixtures/assistant-benchmark/v1"
sys.path.insert(0, str(SCRIPTS))
import run_assistant_benchmark as driver  # noqa: E402
from report_assistant_benchmark import report  # noqa: E402
from run_assistant_benchmark import bounded_run  # noqa: E402


def fixture(name):
    return json.loads((FIXTURES / f"{name}.rsk").read_text()), json.loads(
        (FIXTURES / f"{name}.reference.json").read_text()
    )


class BenchmarkScoringTests(unittest.TestCase):
    def test_independently_checked_references_score_exactly(self):
        for name in ["ethanol", "gly-l-ala", "cyclic-gly4", "boc-l-ala-sodium"]:
            with self.subTest(name=name):
                doc, expected = fixture(name)
                score = benchmark.score_document(doc, expected)
                self.assertTrue(score["graph_identity"], score)
                for key in [
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
                ]:
                    self.assertEqual(score[key], [], (name, key, score))
                self.assertEqual(score["abbreviations"]["member_chemistry_errors"], [])

    def test_atom_and_bond_errors_are_localized(self):
        doc, expected = fixture("ethanol")
        doc["atoms"][-1]["element"] = "N"
        score = benchmark.score_document(doc, expected)
        self.assertFalse(score["graph_identity"])
        self.assertEqual(len(score["atom_errors"]), 1)
        self.assertEqual(score["atom_errors"][0]["observed_atom_id"], 3)
        self.assertEqual(score["bond_errors"], [])
        doc, expected = fixture("ethanol")
        doc["bonds"][0]["order"] = 2
        score = benchmark.score_document(doc, expected)
        self.assertFalse(score["graph_identity"])
        self.assertEqual(len(score["bond_errors"]), 1)
        self.assertEqual(score["bond_errors"][0]["reference_atoms"], [1, 2])

    def test_missing_stereo_and_wrong_stereo_are_distinct_from_connectivity(self):
        original, expected = fixture("gly-l-ala")
        for mutation in ["remove", "invert"]:
            doc = copy.deepcopy(original)
            center = next(a for a in doc["atoms"] if a.get("stereo"))
            if mutation == "invert":
                center["stereo"]["winding"] = "ccw" if center["stereo"]["winding"] == "cw" else "cw"
            else:
                center["stereo"] = None
                for bond in doc["bonds"]:
                    bond["display"] = "plain"
            score = benchmark.score_document(doc, expected)
            self.assertFalse(score["graph_identity"])
            self.assertEqual(len(score["stereochemistry_errors"]), 1)
            self.assertEqual(score["bond_errors"], [])
            self.assertEqual(score["missing_atoms"], [])

    def test_missing_salt_fragment_is_not_hidden_by_largest_component_matching(self):
        doc, expected = fixture("boc-l-ala-sodium")
        doc["atoms"] = [a for a in doc["atoms"] if a["element"] != "Na"]
        score = benchmark.score_document(doc, expected)
        self.assertFalse(score["graph_identity"])
        self.assertEqual(len(score["missing_fragments"]), 1)
        self.assertEqual(score["missing_atoms"][0]["element"], "Na")
        self.assertEqual(score["bond_errors"], [])
        self.assertEqual(score["missing_bonds"], [])

    def test_abbreviation_display_and_underlying_chemistry_are_separate(self):
        doc, expected = fixture("boc-l-ala-sodium")
        expanded = copy.deepcopy(doc)
        expanded["abbreviations"] = []
        score = benchmark.score_document(expanded, expected)
        self.assertTrue(score["graph_identity"])
        self.assertEqual(score["abbreviations"]["missing_display_labels"], {"Boc": 1})
        self.assertEqual(score["abbreviations"]["member_chemistry_errors"], [])
        malformed = copy.deepcopy(doc)
        malformed["abbreviations"][0]["members"].append(999)
        score = benchmark.score_document(malformed, expected)
        self.assertTrue(score["graph_identity"])
        self.assertEqual(len(score["abbreviations"]["member_chemistry_errors"]), 1)

    def test_double_bond_stereo_and_authoritative_unspecified_are_separate(self):
        from rdkit import Chem

        from engine.worker import to_document

        doc = to_document(Chem.MolFromSmiles("F/C=C/F"))
        expected = {"smiles": "F/C=C/F"}
        self.assertTrue(benchmark.score_document(doc, expected)["graph_identity"])
        double = next(b for b in doc["bonds"] if b["order"] == 2)
        double["stereo"] = "cis"
        wrong = benchmark.score_document(doc, expected)
        self.assertFalse(wrong["graph_identity"])
        self.assertEqual(len(wrong["stereochemistry_errors"]), 1)
        self.assertEqual(wrong["bond_errors"], [])
        double["stereo"] = None
        double["stereo_authoritative"] = True
        missing = benchmark.score_document(doc, expected)
        self.assertFalse(missing["graph_identity"])
        self.assertEqual(len(missing["stereochemistry_errors"]), 1)

    def test_report_keeps_failures_provisional_graphs_and_not_run_denominators(self):
        doc, _ = fixture("ethanol")
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            metadata = {
                "backend_commit": "exact-test-baseline",
                "nightly_version": "test-nightly",
                "model": "test-model",
                "effort": "xhigh",
                "codex_version": "test-cli",
                "plan": [
                    {"case": "ethanol", "repeat": 1},
                    {"case": "ethanol", "repeat": 2},
                    {"case": "gly-l-ala", "repeat": 1},
                ],
            }
            (output / "metadata.json").write_text(json.dumps(metadata))
            for repeat, status, artifact in [
                (1, "completed", "drawing.rsk"),
                (2, "failed", "last-preview.rsk"),
            ]:
                run = output / "ethanol" / f"run-{repeat}"
                run.mkdir(parents=True)
                (run / "run.json").write_text(json.dumps({"status": status, "elapsed_seconds": 1}))
                (run / artifact).write_text(json.dumps(doc))
                (run / "apply-validation.json").write_text(json.dumps({"accepted": False}))
            result = report(FIXTURES / "manifest.json", output)
            summary = result["summary"]
            self.assertEqual(summary["finite_reference_planned"], 3)
            self.assertEqual(summary["finite_reference_attempted"], 2)
            self.assertEqual(summary["finite_reference_completed"], 1)
            self.assertEqual(summary["finite_reference_exact_completed"], 1)
            self.assertEqual(summary["finite_reference_exact_applicable"], 0)
            self.assertEqual(
                summary["finite_reference_completed_apply_validation"], {"rejected": 1}
            )
            self.assertEqual(summary["provisional_exact"], 1)
            self.assertEqual(summary["finite_reference_provisional"], 1)
            self.assertEqual(summary["provisional_localized_metric_runs"], 1)
            self.assertEqual(summary["statuses"], {"completed": 1, "failed": 1, "not_run": 1})

    def test_unavailable_exact_model_stops_remaining_probes_without_fallback(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            runner = directory / "runner"
            runner.write_bytes(b"offline-runner")
            cli = directory / "codex"
            cli.write_bytes(b"offline-cli")
            image = directory / "source.png"
            image.write_bytes(b"offline-image")
            import hashlib

            manifest = directory / "manifest.json"
            manifest.write_text(
                json.dumps(
                    {
                        "cases": [
                            {
                                "id": "one",
                                "image": image.name,
                                "image_sha256": hashlib.sha256(image.read_bytes()).hexdigest(),
                            }
                        ]
                    }
                )
            )
            output = directory / "results"
            commands = []

            def unavailable(command, seconds, environment):
                commands.append(command)
                self.assertEqual(command[command.index("--model") + 1], "gpt-6.1-sol")
                self.assertEqual(command[command.index("--effort") + 1], "xhigh")
                run_dir = Path(command[command.index("--output") + 1])
                (run_dir / "run.json").write_text(
                    json.dumps(
                        {
                            "status": "not_run",
                            "stage": "availability",
                            "reason": "The exact requested model is absent from the Codex catalog",
                        }
                    )
                )
                return subprocess.CompletedProcess(command, 0, "", "")

            arguments = [
                "benchmark",
                "--manifest",
                str(manifest),
                "--runner",
                str(runner),
                "--output",
                str(output),
                "--backend-commit",
                "test",
                "--nightly-version",
                "test",
                "--run",
            ]
            with (
                mock.patch.object(sys, "argv", arguments),
                mock.patch.object(driver.shutil, "which", return_value=str(cli)),
                mock.patch.object(driver.platform, "platform", return_value="offline-platform"),
                mock.patch.object(
                    driver.subprocess,
                    "run",
                    return_value=subprocess.CompletedProcess([], 0, "test-cli", ""),
                ),
                mock.patch.object(driver, "bounded_run", side_effect=unavailable),
            ):
                driver.main()
            self.assertEqual(len(commands), 1)
            metadata = json.loads((output / "metadata.json").read_text())
            self.assertEqual(metadata["runner_invocations"], 1)
            self.assertEqual(metadata["inference_workflows_started"], 0)
            self.assertIn("absent", metadata["availability_blocked_reason"])
            for repeat in [1, 2]:
                run = json.loads((output / "one" / f"run-{repeat}" / "run.json").read_text())
                self.assertEqual(run["status"], "not_run")
                self.assertEqual(run["stage"], "availability")

    @unittest.skipUnless(
        os.name == "posix", "Native baseline watchdog check uses POSIX process groups"
    )
    def test_watchdog_stops_runner_and_child_without_waiting_for_their_deadlines(self):
        started = time.monotonic()
        with self.assertRaises(subprocess.TimeoutExpired):
            bounded_run(
                [
                    sys.executable,
                    "-c",
                    "import subprocess,sys,time;subprocess.Popen([sys.executable,'-c','import time;time.sleep(30)']);time.sleep(30)",
                ],
                0.1,
            )
        self.assertLess(time.monotonic() - started, 2)

    def test_haptic_documents_have_an_explicit_reference_limitation(self):
        doc, expected = fixture("ethanol")
        doc["atoms"][0]["centroid"] = [2, 3]
        score = benchmark.score_document(doc, expected)
        self.assertFalse(score["localized_metrics_available"])
        self.assertIn("haptic", score["reference_adapter_error"])


if __name__ == "__main__":
    unittest.main()
