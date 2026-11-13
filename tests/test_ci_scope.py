"""Documentation can skip native CI; uncertain or source changes cannot."""

import subprocess
import unittest
from unittest.mock import patch

from scripts.ci_scope import changed_paths, matrices, needs_geometry, needs_macos, needs_native


class CheckScopeTests(unittest.TestCase):
    def test_web_only_changes_skip_native_jobs(self):
        self.assertFalse(needs_native(["docs/assistant.md", "website/package.json", "bun.lock"]))
        self.assertFalse(needs_native(["bunfig.toml", "package.json"]))
        self.assertTrue(needs_native(["bunfig.toml", "src/lib.rs"]))

    def test_native_unknown_and_removed_files_require_checks(self):
        for path in (
            "src/lib.rs",
            "tests/fixtures/new-capture.json.gz",
            "scripts/build_release.py",
            "Cargo.lock",
            ".github/workflows/checks.yml",
            ".github/actions/setup-msvc/setup.ps1",
            "new-runtime-input.dat",
        ):
            with self.subTest(path=path):
                self.assertTrue(needs_native(["docs/development.md", path]))
        self.assertTrue(needs_native(None))
        self.assertTrue(needs_native([]))

    def test_app_only_changes_skip_geometry(self):
        self.assertFalse(
            needs_geometry(["src/app.rs", "packages/editor/index.ts", "docs/arrows.md"])
        )
        for path in (
            "crates/model/src/lib.rs",
            "native/geometry/src/lib.rs",
            "assets/templates.json",
            "tests/fixtures/coordination-layout.rsk",
            "Cargo.lock",
            ".github/workflows/checks.yml",
        ):
            with self.subTest(path=path):
                self.assertTrue(needs_geometry(["src/app.rs", path]))
        self.assertTrue(needs_geometry(None))

    def test_naming_integration_inputs_require_native_checks(self):
        for path in (
            "Cargo.toml",
            "Cargo.lock",
            "src/naming/catalog/data.json",
            "tests/fixtures/s2n/baseline.tsv",
            "tests/fixtures/openclatura/graph_oracle.json",
        ):
            with self.subTest(path=path):
                self.assertTrue(needs_native(["docs/development.md", path]))

    def test_rename_retains_deleted_source_and_handles_unusual_names(self):
        event = {"pull_request": {"base": {"sha": "a" * 40}}}
        with patch("scripts.ci_scope.subprocess.check_output") as git:
            git.return_value = b"src/old.rs\0docs/new name\nwith newline.md\0"
            paths = changed_paths("pull_request", event)
        self.assertIn("--no-renames", git.call_args.args[0])
        self.assertEqual(paths, ["src/old.rs", "docs/new name\nwith newline.md"])
        self.assertTrue(needs_native(paths))

    def test_missing_base_and_git_failure_default_to_native(self):
        for event in ({}, {"before": "0" * 40}, {"before": "--option"}):
            self.assertIsNone(changed_paths("push", event))
        self.assertIsNone(changed_paths("workflow_dispatch", {}))
        with patch(
            "scripts.ci_scope.subprocess.check_output",
            side_effect=subprocess.CalledProcessError(1, "git"),
        ):
            self.assertIsNone(changed_paths("push", {"before": "b" * 40}))

    def test_macos_runs_on_stacked_labelled_and_non_pr_events(self):
        def pull(ref, *labels):
            return {"pull_request": {"head": {"ref": ref}, "labels": [{"name": n} for n in labels]}}

        self.assertFalse(needs_macos("pull_request", pull("feat/766-stereo-groups", "bug")))
        self.assertFalse(needs_macos("pull_request", pull("fix/merge/x")))
        self.assertTrue(needs_macos("pull_request", pull("merge/round2")))
        self.assertTrue(needs_macos("pull_request", pull("feat/x", "full-ci")))
        for event_name in ("push", "schedule", "workflow_dispatch"):
            self.assertTrue(needs_macos(event_name, {}))

    def test_matrices_leave_out_only_macos_rows(self):
        full, reduced = matrices(True), matrices(False)
        for job, rows in full.items():
            with self.subTest(job=job):
                self.assertTrue(any(row["os"].startswith("macos") for row in rows))
                self.assertEqual(
                    reduced[job], [row for row in rows if not row["os"].startswith("macos")]
                )
                self.assertTrue(reduced[job])


if __name__ == "__main__":
    unittest.main()
