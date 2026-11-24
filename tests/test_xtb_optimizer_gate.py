"""Exercise the xTB gate with synthetic output and isolated optimizer stand-ins."""

import argparse
import contextlib
import csv
import io
import json
import math
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import xtb_optimizer_gate as gate

# Hand-written parser examples, independent of any captured xTB calculation.
XTB_OUTPUT = """
TOTAL ENERGY -8.000000000000 Eh
GEOMETRY OPTIMIZATION CONVERGED AFTER 20 ITERATIONS
TOTAL ENERGY -8.250000000000 Eh
"""
XYZ = "4\nsynthetic molecule\nC 0 0 0\nN 1 0 0\nO 0 2 0\nH 0 0 1\n"
POSITIONS = [[0, 0, 0], [1, 0, 0], [0, 2, 0], [0, 0, 1]]


def optimization(steps=20, energy=-8.25, positions=None):
    return gate.Optimization(steps=steps, energy_hartree=energy, positions_angstrom=positions)


class ParserTests(unittest.TestCase):
    def test_xtb_uses_the_convergence_count_and_final_energy(self):
        self.assertEqual(gate.parse_xtb_output(XTB_OUTPUT), (20, -8.25))
        self.assertEqual(gate.parse_xtb_output("TOTAL ENERGY -1.234D+01 Eh\n"), (None, -12.34))
        self.assertEqual(gate.parse_xtb_output("optimization failed"), (None, None))

    def test_xtb_rejects_nonfinite_energy(self):
        for energy in ("1e999", "NaN", "Inf"):
            with self.subTest(energy=energy), self.assertRaises(ValueError):
                gate.parse_xtb_output(XTB_OUTPUT + f"TOTAL ENERGY {energy} Eh")

    def test_xyz_reads_one_molecule_and_finite_coordinates(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "molecule.xyz"
            path.write_text(XYZ, encoding="utf-8")
            self.assertEqual(
                gate.read_xyz(path), (["C", "N", "O", "H"], [tuple(row) for row in POSITIONS])
            )
            for text in (
                "",
                "0\nempty\n",
                XYZ + "1\nsecond\nH 0 0 0\n",
                XYZ.replace("N 1", "N nan"),
                XYZ.replace("N 1 0 0", "N 1 0"),
            ):
                with self.subTest(text=text), self.assertRaises(ValueError):
                    path.write_text(text, encoding="utf-8")
                    gate.read_xyz(path)

    def test_ours_accepts_required_metrics_and_optional_positions(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "result.json"
            path.write_text(
                json.dumps(
                    {
                        "steps": 23,
                        "energy_hartree": -8.25,
                        "converged": True,
                        "positions_angstrom": POSITIONS,
                    }
                ),
                encoding="utf-8",
            )
            result = gate.parse_ours(path, 4)
            self.assertEqual((result.steps, result.energy_hartree), (23, -8.25))
            self.assertEqual(result.positions_angstrom, [tuple(row) for row in POSITIONS])
            self.assertEqual(result.problems, [])
            path.write_text('{"steps":0,"energy_hartree":0,"converged":false}', encoding="utf-8")
            self.assertIn("optimizer reports not converged", gate.parse_ours(path, 4).problems)

    def test_ours_rejects_missing_or_malformed_metrics_and_positions(self):
        valid = {"steps": 1, "energy_hartree": 0.0}
        cases = [
            [],
            {},
            {**valid, "steps": True},
            {**valid, "steps": -1},
            {**valid, "steps": 1.5},
            {**valid, "energy_hartree": math.nan},
            {**valid, "energy_hartree": "-8.25"},
            {**valid, "positions_angstrom": None},
            {**valid, "positions_angstrom": [[0, 0, 0]]},
            {**valid, "positions_angstrom": [[0, 0]] * 4},
            {**valid, "positions_angstrom": [[0, 0, math.inf]] * 4},
        ]
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "result.json"
            for data in cases:
                with self.subTest(data=data), self.assertRaises((ValueError, KeyError)):
                    path.write_text(json.dumps(data), encoding="utf-8")
                    gate.parse_ours(path, 4)


class GateTests(unittest.TestCase):
    def compare(self, xtb=None, ours=None):
        return gate.compare(
            "synthetic.xyz", ["C", "N", "O", "H"], xtb or optimization(), ours or optimization()
        )

    def test_both_thresholds_include_the_boundary(self):
        result = self.compare(optimization(20, 0), optimization(30, 1e-6))
        self.assertTrue(result.passed, result.problems)
        self.assertEqual(result.step_ratio, 1.5)
        self.assertEqual(result.energy_delta_hartree, 1e-6)

    def test_step_and_energy_gates_fail_independently(self):
        steps = self.compare(ours=optimization(31))
        self.assertFalse(steps.passed)
        self.assertEqual(steps.problems, ["step ratio exceeds 1.5"])
        energy = self.compare(optimization(20, 0), optimization(20, -1.0001e-6))
        self.assertFalse(energy.passed)
        self.assertEqual(energy.problems, ["energy difference exceeds 1e-06 Eh"])

    def test_zero_steps_cannot_divide_by_zero(self):
        self.assertTrue(self.compare(optimization(0), optimization(0)).passed)
        self.assertFalse(self.compare(optimization(0), optimization(1)).passed)

    def test_process_and_convergence_failures_fail_the_gate(self):
        xtb = optimization()
        xtb.problems = ["not converged (no convergence marker)"]
        ours = optimization()
        ours.problems = ["command exited 3"]
        result = self.compare(xtb, ours)
        self.assertFalse(result.passed)
        self.assertEqual(len(result.problems), 2)

    def test_missing_metrics_never_pass_the_gate(self):
        result = self.compare(xtb=optimization(steps=None))
        self.assertFalse(result.passed)
        self.assertIn("xTB: not converged (no convergence marker)", result.problems)
        self.assertFalse(self.compare(ours=gate.Optimization()).passed)

    def test_rmsd_aligns_a_rotated_and_translated_copy(self):
        reference = [[0, 0, 0], [2, 0, 0], [0, 3, 0], [0, 0, 4]]
        angle = 0.713
        c, s = math.cos(angle), math.sin(angle)
        mobile = [[c * x - s * y + 7, s * x + c * y - 2, z + 9] for x, y, z in reference]
        self.assertAlmostEqual(gate.aligned_rmsd(reference, mobile), 0.0, delta=1e-7)
        reflected = [[-x, y, z] for x, y, z in reference]
        self.assertGreater(gate.aligned_rmsd(reference, reflected), 0.5)

    def test_rmsd_excludes_hydrogen_and_does_not_gate(self):
        moved = [row[:] for row in POSITIONS]
        moved[-1] = [100, 200, 300]
        result = self.compare(optimization(positions=POSITIONS), optimization(positions=moved))
        self.assertAlmostEqual(result.heavy_rmsd_angstrom, 0.0, delta=1e-7)
        moved[1] = [5, 0, 0]
        result = self.compare(optimization(positions=POSITIONS), optimization(positions=moved))
        self.assertGreater(result.heavy_rmsd_angstrom, 1)
        self.assertTrue(result.passed)
        hydrogen = gate.compare(
            "h2.xyz",
            ["H", "D"],
            optimization(positions=[[0, 0, 0], [1, 0, 0]]),
            optimization(positions=[[0, 0, 0], [2, 0, 0]]),
        )
        self.assertIsNone(hydrogen.heavy_rmsd_angstrom)


class RunnerTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="xtb gate test ")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.inputs = self.root / "input files"
        self.inputs.mkdir()
        self.xyz = self.inputs / "synthetic molecule.xyz"
        self.xyz.write_text(XYZ, encoding="utf-8")
        self.xtb = self.root / "xtb binary"
        self.xtb.write_text("mock binary", encoding="utf-8")
        self.out = self.root / "reports" / "gate.md"
        self.argv = [
            "--xyz-dir",
            str(self.inputs),
            "--xtb",
            str(self.xtb),
            "--ours",
            'ours --xyz "{xyz}" --json "{json}" --charge {charge} --uhf {uhf}',
            "--out",
            str(self.out),
        ]
        self.calls = []
        self.ours_convergence = {"converged": True}

    def answer(self, command, **options):
        self.calls.append((command, options))
        cwd = options["cwd"]
        self.assertFalse(cwd.resolve().is_relative_to(gate.ROOT))
        self.assertNotEqual(cwd, self.inputs)
        if command[0] == str(self.xtb):
            (cwd / "xtbopt.xyz").write_text(XYZ, encoding="utf-8")
            output = XTB_OUTPUT
        else:
            path = Path(command[command.index("--json") + 1])
            path.write_text(
                json.dumps(
                    {
                        "steps": 30,
                        "energy_hartree": -8.25,
                        "positions_angstrom": POSITIONS,
                        **self.ours_convergence,
                    }
                ),
                encoding="utf-8",
            )
            output = ""
        return subprocess.CompletedProcess(command, 0, stdout=output)

    def run_main(self, answer=None, argv=None):
        output = io.StringIO()
        with (
            patch("xtb_optimizer_gate.subprocess.run", side_effect=answer or self.answer),
            contextlib.redirect_stdout(output),
        ):
            code = gate.main(self.argv if argv is None else argv)
        return code, output.getvalue()

    def test_cli_runs_quoted_paths_with_same_input_in_separate_temporary_directories(self):
        code, output = self.run_main(
            argv=[*self.argv, "--level", "tight", "--charge", "-1", "--uhf", "2", "--threads", "3"]
        )
        self.assertEqual(code, 0)
        self.assertIn("| PASS |", output)
        self.assertEqual(self.out.read_text(encoding="utf-8"), output)
        with self.out.with_suffix(".csv").open(newline="", encoding="utf-8") as report:
            rows = list(csv.DictReader(report))
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]["molecule"], self.xyz.name)
        self.assertEqual(rows[0]["step_ratio"], "1.5")
        self.assertAlmostEqual(float(rows[0]["heavy_rmsd_angstrom"]), 0, delta=1e-7)
        xtb, ours = self.calls
        self.assertEqual(
            xtb[0],
            [
                str(self.xtb),
                str(self.xyz),
                "--opt",
                "tight",
                "--gfn",
                "2",
                "--chrg",
                "-1",
                "--uhf",
                "2",
            ],
        )
        self.assertEqual(xtb[1]["env"]["OMP_NUM_THREADS"], "3")
        self.assertEqual(ours[0][2], str(self.xyz))
        self.assertEqual(ours[0][-4:], ["--charge", "-1", "--uhf", "2"])
        self.assertNotEqual(xtb[1]["cwd"], ours[1]["cwd"])
        self.assertIsNone(ours[1]["env"])
        for _, options in self.calls:
            self.assertFalse(options["cwd"].exists())
            self.assertNotIn("shell", options)

    def test_missing_convergence_fails_even_when_the_command_succeeds(self):
        def answer(command, **options):
            result = self.answer(command, **options)
            if command[0] == str(self.xtb):
                result.stdout = "TOTAL ENERGY -8.25 Eh\n"
            return result

        code, output = self.run_main(answer)
        self.assertEqual(code, 1)
        self.assertIn("not converged", output)
        self.assertIn("| FAIL |", output)

    def test_ours_requires_explicit_convergence_even_when_the_command_succeeds(self):
        for fields in (
            {"status": "MaxSteps"},
            {"status": True},
            {"converged": "false"},
            {"converged": False},
            {"converged": 1},
            {"converged": True, "status": "MaxSteps"},
            {},
        ):
            with self.subTest(fields=fields):
                self.ours_convergence = fields
                code, output = self.run_main()
                self.assertEqual(code, 1)
                self.assertIn("optimizer reports not converged", output)

        self.ours_convergence = {"status": "Converged"}
        code, output = self.run_main()
        self.assertEqual(code, 0)
        self.assertIn("| PASS |", output)

    def test_failed_command_and_missing_json_are_reported(self):
        def answer(command, **options):
            if command[0] == str(self.xtb):
                return self.answer(command, **options)
            return subprocess.CompletedProcess(command, 7, stdout="failed")

        code, output = self.run_main(answer)
        self.assertEqual(code, 1)
        self.assertIn("ours: command exited 7", output)
        self.assertIn("result.json", output)

    def test_bad_xyz_fails_one_row_and_continues_other_molecules(self):
        (self.inputs / "bad.xyz").write_text("not XYZ", encoding="utf-8")
        code, output = self.run_main()
        self.assertEqual(code, 1)
        self.assertIn("passed 1 of 2", output)
        self.assertEqual(len(self.calls), 2)

    def test_repo_temporary_root_is_rejected_before_running_programs(self):
        with patch("xtb_optimizer_gate.tempfile.gettempdir", return_value=str(gate.ROOT)):
            code, output = self.run_main()
        self.assertEqual(code, 1)
        self.assertIn("set TMPDIR", output)
        self.assertFalse(self.calls)

    def test_launch_error_is_a_failed_gate(self):
        with (
            patch("xtb_optimizer_gate.subprocess.run", side_effect=OSError("cannot execute")),
            contextlib.redirect_stdout(io.StringIO()) as output,
        ):
            self.assertEqual(gate.main(self.argv), 1)
        self.assertIn("cannot execute", output.getvalue())

    def test_empty_corpus_fails(self):
        self.xyz.unlink()
        with contextlib.redirect_stderr(io.StringIO()) as errors:
            code, _ = self.run_main()
        self.assertEqual(code, 1)
        self.assertIn("No .xyz", errors.getvalue())


class ReportTests(unittest.TestCase):
    def test_relative_executable_resolves_before_the_temporary_cwd(self):
        options = argparse.Namespace(
            ours='./target/debug/reshiki "{xyz}" "{json}"',
            charge=0,
            uhf=0,
            level="normal",
            threads=1,
        )
        command = gate.ours_command(options, Path("input.xyz"), Path("result.json"))
        self.assertEqual(command[0], str(Path("target/debug/reshiki").resolve()))
        options.ours = 'reshiki "{xyz}" "{json}"'
        self.assertEqual(
            gate.ours_command(options, Path("input.xyz"), Path("result.json"))[0], "reshiki"
        )

    def test_report_stems_and_suffixes_produce_both_formats(self):
        expected = (Path("report.md"), Path("report.csv"))
        for name in ("report", "report.md", "report.csv"):
            self.assertEqual(gate.report_paths(Path(name)), expected)
        self.assertEqual(
            gate.report_paths(Path("report.v1")), (Path("report.v1.md"), Path("report.v1.csv"))
        )

    def test_markdown_escapes_table_delimiters_and_newlines(self):
        result = gate.compare("a|b.xyz", ["C"], optimization(), optimization())
        result.problems = ["bad | result\nsecond line"]
        options = argparse.Namespace(level="normal", charge=0, uhf=0, threads=1)
        text = gate.markdown([result], options)
        self.assertIn("a\\|b.xyz", text)
        self.assertIn("bad \\| result second line", text)


if __name__ == "__main__":
    unittest.main()
