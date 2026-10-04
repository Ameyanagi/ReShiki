"""Pinned reference checkouts publish only after verification and clean up failures."""

import os
import subprocess
import sys
import tempfile
import unittest
from contextlib import ExitStack
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import setup_linux_depict_reference as linux
import setup_windows_depict_reference as windows


class ReferenceCheckoutFlowTests(unittest.TestCase):
    def setUp(self):
        directory = self.enterContext(tempfile.TemporaryDirectory())
        self.root = Path(directory)
        config = self.root / "global.gitconfig"
        config.write_text("[core]\n    autocrlf = false\n    eol = lf\n")
        self.enterContext(patch.dict(os.environ, {"GIT_CONFIG_GLOBAL": str(config)}))
        self.source = self.root / "source"
        subprocess.run(["git", "init", "--quiet", str(self.source)], check=True)
        (self.source / ".gitattributes").write_bytes(b"*.cpp text\n")
        (self.source / "Code").mkdir()
        self.contents = b"double source_value() {\n  return 1.0;\n}\n"
        (self.source / "Code/reference.cpp").write_bytes(self.contents)
        subprocess.run(["git", "-C", str(self.source), "add", "."], check=True)
        subprocess.run(
            [
                "git",
                "-C",
                str(self.source),
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--quiet",
                "-m",
                "Pinned source",
            ],
            check=True,
        )
        self.revision = subprocess.check_output(
            ["git", "-C", str(self.source), "rev-parse", "HEAD"], text=True
        ).strip()
        self.enterContext(patch.object(linux, "PIN", self.revision))
        self.enterContext(patch.object(windows, "PIN", self.revision))

    def destination(self, module, name="reference"):
        return self.root / module.__name__ / name / "rdkit"

    def trace_checkout(self, module, destination, *, fail_command=None, fail_rename=False):
        """Run real Git locally, recording the production commands and visibility."""
        events = []
        real_run, real_output, real_rename = subprocess.run, subprocess.check_output, Path.rename
        inside_output = False

        def record(kind, command, kwargs):
            events.append((kind, command.copy(), kwargs.copy(), destination.exists()))
            if fail_command and fail_command(command):
                raise subprocess.CalledProcessError(1, command)

        def run(command, **kwargs):
            if inside_output:
                return real_run(command, **kwargs)
            record("run", command, kwargs)
            local_command = command.copy()
            if command[3:4] == ["fetch"]:
                # Keep the production URL in the trace, but never contact it.
                self.assertEqual(command[5], "https://github.com/rdkit/rdkit.git")
                local_command[5] = str(self.source)
            return real_run(local_command, **kwargs, stdout=subprocess.PIPE, stderr=subprocess.PIPE)

        def output(command, **kwargs):
            nonlocal inside_output
            record("check_output", command, kwargs)
            inside_output = True
            try:
                return real_output(command, **kwargs)
            finally:
                inside_output = False

        def rename(source, target):
            record("rename", [str(source), str(target)], {})
            if fail_rename:
                raise OSError("Fixture publication failure")
            return real_rename(source, target)

        with ExitStack() as stack:
            stack.enter_context(patch.object(subprocess, "run", run))
            stack.enter_context(patch.object(subprocess, "check_output", output))
            stack.enter_context(patch.object(Path, "rename", rename))
            try:
                result = module.source_checkout(destination)
            except (subprocess.CalledProcessError, ValueError, OSError) as error:
                return events, error
        return events, result

    def verification_events(self, directory, published):
        prefix = ["git", "-C", str(directory)]
        return [
            ("check_output", [*prefix, "rev-parse", "HEAD"], {"text": True}, published),
            (
                "run",
                [*prefix, "diff", "--exit-code", "HEAD", "--", "Code"],
                {"check": True},
                published,
            ),
            (
                "check_output",
                [*prefix, "ls-files", "--others", "--", "Code"],
                {"text": True},
                published,
            ),
        ]

    def assert_clean_staging(self, destination):
        self.assertEqual(list(destination.parent.glob("rdkit-fetch-*")), [])

    def test_exact_commands_verify_before_publish_and_return_detached_pinned_source(self):
        for module in (linux, windows):
            with self.subTest(platform=module.__name__):
                destination = self.destination(module)
                events, result = self.trace_checkout(module, destination)
                temporary = Path(events[0][1][-1])
                self.assertEqual(temporary.parent, destination.parent)
                self.assertTrue(temporary.name.startswith("rdkit-fetch-"))
                prefix = ["git", "-C", str(temporary)]
                expected = [
                    ("run", ["git", "init", "--quiet", str(temporary)], {"check": True}, False)
                ]
                if module is windows:
                    expected.extend(
                        ("run", [*prefix, "config", key, value], {"check": True}, False)
                        for key, value in (("core.autocrlf", "false"), ("core.eol", "lf"))
                    )
                expected.extend(
                    [
                        (
                            "run",
                            [
                                *prefix,
                                "fetch",
                                "--depth=1",
                                "https://github.com/rdkit/rdkit.git",
                                self.revision,
                            ],
                            {"check": True, "timeout": 300},
                            False,
                        ),
                        (
                            "run",
                            [*prefix, "checkout", "--quiet", "--detach", "FETCH_HEAD"],
                            {"check": True},
                            False,
                        ),
                        *self.verification_events(temporary, False),
                        ("rename", [str(temporary), str(destination)], {}, False),
                        *self.verification_events(destination, True),
                    ]
                )
                self.assertEqual(events, expected)
                self.assertEqual(result, destination.resolve())
                self.assertEqual((destination / "Code/reference.cpp").read_bytes(), self.contents)
                self.assertEqual((destination / ".git/HEAD").read_text().strip(), self.revision)
                self.assert_clean_staging(destination)

    def test_existing_checkout_is_verified_without_fetch_or_publication(self):
        for module in (linux, windows):
            with self.subTest(platform=module.__name__):
                destination = self.destination(module)
                _, result = self.trace_checkout(module, destination)
                self.assertEqual(result, destination.resolve())
                outside = destination / "outside-Code.txt"
                outside.write_text("Allowed untracked file outside Code")
                events, result = self.trace_checkout(module, destination)
                self.assertEqual(events, self.verification_events(destination, True))
                self.assertEqual(result, destination.resolve())
                self.assertEqual(outside.read_text(), "Allowed untracked file outside Code")

    def test_command_failures_before_publication_leave_no_checkout_or_temporary_directory(self):
        failures = {
            "init": lambda command: command[1:2] == ["init"],
            "fetch": lambda command: command[3:4] == ["fetch"],
            "checkout": lambda command: command[3:4] == ["checkout"],
            "verify-head": lambda command: command[3:4] == ["rev-parse"],
            "verify-tracked": lambda command: command[3:4] == ["diff"],
            "verify-untracked": lambda command: command[3:4] == ["ls-files"],
        }
        for module in (linux, windows):
            for name, predicate in failures.items():
                with self.subTest(platform=module.__name__, failure=name):
                    destination = self.destination(module, name)
                    events, result = self.trace_checkout(
                        module, destination, fail_command=predicate
                    )
                    self.assertIsInstance(result, subprocess.CalledProcessError)
                    self.assertTrue(predicate(events[-1][1]))
                    self.assertTrue(all(not event[3] for event in events))
                    self.assertFalse(destination.exists())
                    self.assert_clean_staging(destination)

    def test_windows_configuration_failures_clean_up_before_fetch(self):
        for key in ("core.autocrlf", "core.eol"):
            with self.subTest(key=key):
                destination = self.destination(windows, key)
                events, result = self.trace_checkout(
                    windows,
                    destination,
                    fail_command=lambda command: command[3:5] == ["config", key],
                )
                self.assertIsInstance(result, subprocess.CalledProcessError)
                self.assertEqual(events[-1][1][-2], key)
                self.assertFalse(any(event[1][3:4] == ["fetch"] for event in events))
                self.assertFalse(destination.exists())
                self.assert_clean_staging(destination)

    def test_rename_failure_removes_verified_staging_without_publishing(self):
        for module in (linux, windows):
            with self.subTest(platform=module.__name__):
                destination = self.destination(module)
                events, result = self.trace_checkout(module, destination, fail_rename=True)
                self.assertIsInstance(result, OSError)
                self.assertEqual(events[-1][0], "rename")
                temporary = Path(events[-1][1][0])
                self.assertEqual(events[-4:-1], self.verification_events(temporary, False))
                self.assertFalse(destination.exists())
                self.assert_clean_staging(destination)

    def test_final_verification_failure_keeps_the_published_checkout(self):
        for module in (linux, windows):
            with self.subTest(platform=module.__name__):
                destination = self.destination(module)
                events, result = self.trace_checkout(
                    module,
                    destination,
                    fail_command=lambda command: (
                        command[1:4] == ["-C", str(destination), "rev-parse"]
                    ),
                )
                self.assertIsInstance(result, subprocess.CalledProcessError)
                self.assertEqual(events[-2][0], "rename")
                self.assertTrue(events[-1][3])
                self.assertEqual((destination / "Code/reference.cpp").read_bytes(), self.contents)
                self.assert_clean_staging(destination)

    def test_existing_wrong_head_dirty_code_and_untracked_code_are_rejected_without_fetch(self):
        for module in (linux, windows):
            for damage in ("wrong-head", "dirty-code", "untracked-code"):
                with self.subTest(platform=module.__name__, damage=damage):
                    destination = self.destination(module, damage)
                    _, result = self.trace_checkout(module, destination)
                    self.assertEqual(result, destination.resolve())
                    if damage == "wrong-head":
                        (destination / ".git/HEAD").write_text("a" * 40 + "\n")
                    elif damage == "dirty-code":
                        (destination / "Code/reference.cpp").write_bytes(b"Changed source\n")
                    else:
                        (destination / "Code/untracked.cpp").write_bytes(b"Unexpected source\n")
                    events, result = self.trace_checkout(module, destination)
                    self.assertIsInstance(result, (ValueError, subprocess.CalledProcessError))
                    count = {"wrong-head": 1, "dirty-code": 2, "untracked-code": 3}[damage]
                    self.assertEqual(events, self.verification_events(destination, True)[:count])
                    self.assertTrue(destination.exists())
                    self.assert_clean_staging(destination)


if __name__ == "__main__":
    unittest.main()
