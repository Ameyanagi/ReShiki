"""Documentation can skip native CI; uncertain or source changes cannot."""

import subprocess
import unittest
from unittest.mock import patch

from scripts.ci_scope import changed_paths, needs_native


class CheckScopeTests(unittest.TestCase):
    def test_web_only_changes_skip_native_jobs(self):
        self.assertFalse(needs_native(["docs/assistant.md", "website/package.json", "bun.lock"]))

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


if __name__ == "__main__":
    unittest.main()
