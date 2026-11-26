"""Candidate engine wheels stay paired and get a release publishing dry run."""

import re
import shlex
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def workflow_jobs(name):
    text = (ROOT / ".github/workflows" / name).read_text()
    body = text.split("\njobs:\n", 1)[1]
    return dict(re.findall(r"(?ms)^  ([\w-]+):\n(.*?)(?=^  [\w-]+:\n|\Z)", body))


class PythonWheelWorkflowTests(unittest.TestCase):
    def test_calculation_sources_trigger_candidate_wheel_checks(self):
        workflow = (ROOT / ".github/workflows/python-wheels.yml").read_text()
        triggers = workflow.split("  workflow_call:", 1)[0]
        for path in (
            "Cargo.toml",
            "Cargo.lock",
            "crates/python/**",
            "crates/python-xtb/**",
            "crates/xtb-worker/**",
            "crates/optimize/**",
            "crates/qcdata/**",
            "native/process/**",
            "crates/process-heap/**",
        ):
            with self.subTest(path=path):
                self.assertIn(f'      - "{path}"\n', triggers)

    def test_base_wheel_tests_do_not_build_the_optional_engine(self):
        for workflow, job in (
            ("python-wheels.yml", "wheel"),
            ("checks.yml", "python-package"),
        ):
            with self.subTest(workflow=workflow):
                base = workflow_jobs(workflow)[job]
                self.assertIn("--extra excel --extra pandas --no-install-project", base)
                self.assertNotIn("--all-extras", base)
                self.assertNotIn("--extra xtb", base)
                self.assertNotIn("reshiki-xtb", base)

    def test_engine_notices_are_checked_before_adding_the_base_wheel(self):
        engine = workflow_jobs("python-wheels.yml")["xtb_wheel"]
        self.assertIn("fromJSON(needs.scope.outputs.matrix).xtb_wheel", engine)
        self.assertIn("--manifest-path crates/xtb-worker/Cargo.toml --features engine", engine)
        self.assertIn("--destination crates/python-xtb/wheel-licenses", engine)
        self.assertIn("--target ${{ matrix.target }}", engine)
        self.assertIn('scripts/check_python_wheel_notices.py --xtb "dist/*.whl"', engine)
        self.assertLess(
            engine.index("scripts/check_python_wheel_notices.py --xtb"),
            engine.index("name: Download the matching base wheel"),
        )
        # Both builds must read python-xtb/pyproject.toml, which selects bin bindings.
        self.assertEqual(engine.count("working-directory: crates/python-xtb"), 2)
        self.assertIn("manylinux: auto", engine)

    def test_macos_wheels_share_the_documented_deployment_floor(self):
        jobs = workflow_jobs("python-wheels.yml")
        for name in ("wheel", "xtb_wheel"):
            with self.subTest(job=name):
                steps = re.split(r"(?m)^      - ", jobs[name].split("    steps:\n", 1)[1])
                builds = [step for step in steps if "run: uvx maturin@" in step]
                macos = [step for step in builds if "if: runner.os == 'macOS'" in step]
                self.assertEqual(len(macos), 1)
                self.assertIn('MACOSX_DEPLOYMENT_TARGET: "14.0"', macos[0])
                for step in builds:
                    if step not in macos:
                        self.assertNotIn("MACOSX_DEPLOYMENT_TARGET", step)
                self.assertIn("manylinux: auto", jobs[name])

    def test_engine_tests_install_only_the_local_matching_candidates(self):
        engine = workflow_jobs("python-wheels.yml")["xtb_wheel"]
        self.assertIn("needs: [scope, wheel]", engine)
        self.assertIn("name: python-wheel-${{ matrix.artifact }}", engine)
        self.assertIn("uv venv --clear .venv-xtb-pair", engine)
        self.assertIn(
            'uv pip install --python .venv-xtb-pair/bin/python "numpy>=1.24" "pytest>=8,<10"',
            engine,
        )
        install = next(
            line.strip()
            for line in engine.splitlines()
            if "uv pip install " in line and "--no-index" in line
        )
        self.assertEqual(
            shlex.split(install),
            [
                "uv",
                "pip",
                "install",
                "--python",
                ".venv-xtb-pair/bin/python",
                "--no-index",
                "--find-links",
                "dist",
                "reshiki[xtb]==$EXPECTED_VERSION",
            ],
        )
        self.assertIn('glob("reshiki-*.whl")).name.split("-")[1]', engine)
        self.assertIn('assert version("reshiki") == expected', engine)
        self.assertIn('assert version("reshiki-xtb") == expected', engine)
        self.assertIn("assert reshiki.__version__ == reshiki_xtb.__version__ == expected", engine)
        self.assertNotIn("uv sync --project", engine)
        self.assertIn("uv run --no-project --python .venv-xtb-pair/bin/python pytest", engine)
        self.assertIn("os.access(worker, os.X_OK)", engine)
        self.assertIn("print(xtb.engine())", engine)
        self.assertIn(
            "pytest crates/python/tests/test_xtb.py crates/python/tests/test_qc.py", engine
        )
        self.assertNotIn("--all-extras", engine)
        self.assertNotIn("--extra xtb", engine)
        self.assertIn("name: python-xtb-wheel-${{ matrix.artifact }}", engine)
        # The wheelhouse now also holds a base wheel; it must not enter this artifact.
        self.assertIn("path: dist/reshiki_xtb-*.whl", engine)

    def test_engine_publish_dry_run_requires_the_public_tagged_source(self):
        jobs = workflow_jobs("release.yml")
        engine = jobs["xtb_wheels_check"]
        self.assertNotIn("testpypi_xtb", jobs)
        self.assertIn("needs: [publish, python_wheels]", engine)
        self.assertIn("!cancelled()", engine)
        self.assertIn("github.event_name == 'push'", engine)
        self.assertIn("startsWith(github.ref, 'refs/tags/v')", engine)
        self.assertIn("needs.publish.result == 'success'", engine)
        self.assertIn("needs.python_wheels.result == 'success'", engine)
        self.assertNotIn("workflow_dispatch", engine)
        self.assertNotIn("inputs.pypi_only", engine)
        self.assertNotIn("environment:", engine)
        self.assertIn("    permissions:\n      contents: read\n    steps:\n", engine)
        self.assertNotIn("uses: pypa/gh-action-pypi-publish@", engine)
        self.assertIn("pattern: python-xtb-wheel-*", engine)
        check = 'scripts/check_python_wheel_notices.py --release-tag "$RELEASE_TAG" "dist/reshiki_xtb-*.whl"'
        twine_check = "uvx twine@6.1.0 check dist/reshiki_xtb-*.whl"
        dry_run = (
            "uv publish --dry-run --trusted-publishing never "
            "--publish-url https://test.pypi.org/legacy/ dist/reshiki_xtb-*.whl"
        )
        self.assertIn("RELEASE_TAG: ${{ github.ref_name }}", engine)
        self.assertIn(check, engine)
        self.assertIn(twine_check, engine)
        self.assertIn(dry_run, engine)
        self.assertIn("sparse-checkout: scripts", engine)
        self.assertLess(engine.index("pattern: python-xtb-wheel-*"), engine.index(check))
        self.assertLess(engine.index(check), engine.index(twine_check))
        self.assertLess(engine.index(twine_check), engine.index(dry_run))
        self.assertIn("uses: ./.github/workflows/python-wheels.yml", jobs["python_wheels"])
        self.assertIn("pattern: python-wheel-*", jobs["pypi"])
        for name, job in jobs.items():
            if "pattern: python-xtb-wheel-*" in job:
                self.assertEqual(name, "xtb_wheels_check")


if __name__ == "__main__":
    unittest.main()
