"""Faults in provenance, byte preservation and functional-only result handling."""

import argparse
import hashlib
import json
import os
import subprocess
import sys
import tempfile
import threading
import time
import types
import unittest
import zipfile
from pathlib import Path
from unittest.mock import MagicMock, Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import collect_windows_functional as functional

UNINSTALL_COMPLETION = b"".join(
    b"2026-10-03 09:37:37.728   " + message + b"\r\n"
    for message in (
        b"Uninstallation process succeeded.",
        b"Removed all? Yes",
        b"Need to restart Windows? No",
        b"Log closed.",
    )
)


class FunctionalEvidenceTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        root_patch = patch.object(functional.build_release, "ROOT", self.root)
        root_patch.start()
        self.addCleanup(root_patch.stop)
        self.state = {
            "host": {"ok": True, "data": {"system_type": "x64-based PC"}},
            "status": {
                "ok": True,
                "data": {"AMRunningMode": "Passive", "RealTimeProtectionEnabled": False},
            },
        }
        probe = patch.object(functional, "observation", return_value=self.state)
        probe.start()
        self.addCleanup(probe.stop)
        environment = patch.dict(
            os.environ,
            GITHUB_REPOSITORY="example/fixture",
            GITHUB_RUN_ID="123",
            GITHUB_RUN_ATTEMPT="1",
            GITHUB_SHA="a" * 40,
            GITHUB_JOB="functional",
        )
        environment.start()
        self.addCleanup(environment.stop)

    def case(self, name="evidence"):
        output = self.root / name
        output.mkdir()
        args = argparse.Namespace(
            output=output,
            architecture="x64",
            source_commit="a" * 40,
            candidate="b" * 40,
            target=functional.TARGETS["x64"],
        )
        initial = {
            "source_equivalence": {
                "production_candidate": args.candidate,
                "harness_head": args.source_commit,
            },
            "workflow_run": functional.workflow_run(),
        }
        return functional.FunctionalRun(args, initial)

    def file(self, name, contents=b"ordinary fixture bytes"):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(contents)
        return path

    def artifacts(self, name):
        case = self.case(name)
        paths = {}
        for role in (
            "archive",
            "payload",
            "build_metadata",
            "setup",
            "uninstaller-1",
            "uninstaller-2",
        ):
            path = paths[role] = self.file(name + "-" + role, role.encode("ascii"))
            newline = "\n" if role in {"archive", "setup"} else None
            expected = case.retain_identity(role, path, newline=newline)
            if newline is not None:
                Path(str(path) + ".sha256").write_bytes(
                    f"{expected}  {path.name}{newline}".encode("ascii")
                )
        return case, paths

    def test_existing_registration_and_wrong_native_host_are_rejected(self):
        registry = types.SimpleNamespace(
            HKEY_CURRENT_USER=1, OpenKey=Mock(side_effect=FileNotFoundError)
        )
        with (
            patch.dict(os.environ, GITHUB_ACTIONS="true"),
            patch.object(sys, "platform", "win32"),
            patch.dict(sys.modules, winreg=registry),
        ):
            functional.require_disposable_host("x64", self.state)
            with self.assertRaisesRegex(ValueError, "architecture"):
                functional.require_disposable_host("arm64", self.state)
            registry.OpenKey = Mock(return_value=MagicMock())
            with self.assertRaisesRegex(ValueError, "Existing ReShiki"):
                functional.require_disposable_host("x64", self.state)

    def test_preflight_records_inactive_protection_without_av_acceptance(self):
        args = argparse.Namespace(
            output=self.root / "preflight",
            architecture="x64",
            candidate="b" * 40,
            source_commit="a" * 40,
        )
        with (
            patch.object(functional, "source_equivalence", return_value={"verified": True}),
            patch.object(functional, "require_disposable_host"),
        ):
            functional.preflight(args)
        record = json.loads((args.output / "preflight.json").read_text())
        self.assertEqual(record["status"], "disposable_native_context_verified")
        self.assertEqual(record["antivirus_qualification"], functional.NO_AV)
        self.assertFalse(record["before"]["status"]["data"]["RealTimeProtectionEnabled"])
        self.assertEqual(record["workflow_run"]["run_id"], "123")
        self.assertEqual(record["workflow_run"]["run_attempt"], "1")

    def test_build_rejects_preflight_from_a_different_run_attempt_before_execution(self):
        initial = self.file(
            "preflight.json",
            json.dumps(
                {
                    "status": "disposable_native_context_verified",
                    "architecture": "x64",
                    "workflow_run": {**functional.workflow_run(), "run_attempt": "2"},
                }
            ).encode(),
        )
        argv = [
            "collect_windows_functional.py",
            "build",
            "--architecture",
            "x64",
            "--target",
            functional.TARGETS["x64"],
            "--candidate",
            "b" * 40,
            "--source-commit",
            "a" * 40,
            "--preflight",
            str(initial),
            "--output",
            str(self.root / "not-created"),
        ]
        with (
            patch.object(sys, "argv", argv),
            patch.object(functional.FunctionalRun, "build") as build,
        ):
            with self.assertRaisesRegex(ValueError, "different workflow run or attempt"):
                functional.main()
            build.assert_not_called()
        self.assertFalse((self.root / "not-created").exists())

    def git_command(self, *args):
        return (
            subprocess.check_output(["git", *args], cwd=self.root, stderr=subprocess.PIPE)
            .decode()
            .strip()
        )

    def commit(self, *extra):
        hooks = self.root / "empty-hooks"
        hooks.mkdir(exist_ok=True)
        self.git_command(
            "-c",
            f"core.hooksPath={hooks}",
            "-c",
            "user.name=Functional fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            "fixture",
            *extra,
        )
        return self.git_command("rev-parse", "HEAD")

    def repository(self):
        self.git_command("init", "-q")
        self.file("Cargo.toml", b"production configuration\n")
        self.file("src/main.rs", b"production source\n")
        self.file(".github/workflows/release.yml", b"production release workflow\n")
        self.git_command("add", ".")
        candidate = self.commit()
        for path in functional.ALLOWED_DIFF:
            self.file(path, b"validation overlay\n")
        self.git_command("add", ".")
        return candidate, self.commit()

    def test_real_git_overlay_requires_only_three_paths_and_exact_parent(self):
        candidate, head = self.repository()
        with patch.dict(os.environ, GITHUB_SHA=head):
            record = functional.source_equivalence(candidate, head)
            self.assertTrue(record["unchanged_path_modes_and_git_objects"])
            self.assertEqual(record["unchanged_tracked_path_count"], 2)
            self.assertEqual(record["allowed_diff"], functional.ALLOWED_DIFF)
            self.file("src/main.rs", b"unexpected working edit\n")
            with self.assertRaisesRegex(ValueError, "Tracked worktree"):
                functional.source_equivalence(candidate, head)
        self.git_command("add", "src/main.rs")
        changed_head = self.commit("--amend")
        with patch.dict(os.environ, GITHUB_SHA=changed_head):
            with self.assertRaisesRegex(ValueError, "three allowed"):
                functional.source_equivalence(candidate, changed_head)
            with self.assertRaisesRegex(ValueError, "directly on"):
                functional.source_equivalence(head, changed_head)

    def test_original_installer_calls_retain_two_uninstallers_before_deletion(self):
        case = self.case()
        app = self.file("source/reshiki.exe")
        setup = self.file("reshiki-fixture-setup.exe", b"setup fixture")
        case.retain_identity("payload", app)
        installed = self.root / "Installed fixture"
        count = 0

        def native_fixture(command, **_kwargs):
            nonlocal count
            for argument in command:
                if str(argument).startswith("/LOG="):
                    Path(str(argument)[5:]).write_bytes(
                        b"synthetic setup log"
                        if Path(command[0]) == setup
                        else UNINSTALL_COMPLETION
                    )
            if Path(command[0]) == setup:
                count += 1
                installed.mkdir(exist_ok=True)
                (installed / "reshiki.exe").write_bytes(app.read_bytes())
                (installed / "unins000.exe").write_bytes(f"generated uninstaller {count}".encode())
            else:
                (installed / "reshiki.exe").unlink()
                (installed / "unins000.exe").unlink()
            return subprocess.CompletedProcess(command, 0)

        def original_verifier(installer, source):
            self.assertEqual(source, app.parent)
            try:
                for _ in range(2):
                    functional.installers.run([installer, f"/DIR={installed}"], timeout=180)
            finally:
                functional.installers.run([installed / "unins000.exe", "/VERYSILENT"], timeout=180)

        case.original_run = Mock(side_effect=native_fixture)
        case.original_installer = original_verifier
        with patch.object(functional.installers, "run", case.installer_run):
            case.verify_installer(setup, app.parent)
        self.assertEqual(case.original_run.call_count, 3)
        self.assertFalse((installed / "unins000.exe").exists())
        self.assertEqual(len(case.record["installations"]), 2)
        for number in (1, 2):
            saved = case.args.output / f"uninstaller-{number}.exe"
            self.assertEqual(saved.read_bytes(), f"generated uninstaller {number}".encode())
            self.assertEqual(
                functional.digest(saved),
                case.record["installations"][number - 1]["uninstaller_sha256"],
            )
        self.assertEqual(case.record["antivirus_qualification"], functional.NO_AV)

    def assert_async_uninstaller_completion(self, name, prefix, completion):
        case = self.case(name)
        target = self.file(name + "-installed/unins000.exe", b"ordinary uninstaller fixture")
        case.uninstallers[str(target.resolve())] = functional.digest(target)
        launcher_returned = threading.Event()
        waiting = threading.Event()
        allow_completion = threading.Event()
        callback_done = threading.Event()
        post_checks = []
        errors = []
        log = case.args.output / "process-1.log"

        def launcher(command, **_kwargs):
            self.assertEqual(Path(command[0]), target)
            log.write_bytes(prefix)
            launcher_returned.set()
            return subprocess.CompletedProcess(command, 0)

        def wait_for_child(_seconds):
            waiting.set()
            if not allow_completion.wait(2):
                raise TimeoutError("Async fixture child was not released")

        def original_verifier_after_launcher():
            try:
                case.installer_run([target, "/VERYSILENT"], timeout=180)
                # Represents the original caller's file/user-data assertions:
                # they must observe the actual child's completed state.
                post_checks.append(log.read_bytes())
            except BaseException as error:
                errors.append(error)
            finally:
                callback_done.set()

        case.original_run = Mock(side_effect=launcher)
        clock = types.SimpleNamespace(monotonic=time.monotonic, sleep=wait_for_child)
        worker = threading.Thread(target=original_verifier_after_launcher, daemon=True)
        with patch.object(functional, "time", clock, create=True):
            try:
                worker.start()
                self.assertTrue(launcher_returned.wait(2))
                self.assertTrue(
                    waiting.wait(1),
                    "Callback returned without waiting for the child log completion",
                )
                self.assertFalse(callback_done.is_set())
                self.assertEqual(post_checks, [])
                with log.open("ab") as stream:
                    stream.write(completion)
                allow_completion.set()
                worker.join(2)
                self.assertFalse(worker.is_alive())
            finally:
                allow_completion.set()
                worker.join(2)
        self.assertEqual(errors, [])
        expected = prefix + completion
        self.assertEqual(post_checks, [expected])
        record = case.record["processes"][0]
        self.assertEqual(record["log_sha256"], hashlib.sha256(expected).hexdigest())
        self.assertEqual(record["log_size"], len(expected))
        self.assertEqual(record["log_completion"]["status"], "terminal_success_observed")
        self.assertLessEqual(
            record["launcher_returned_utc"], record["log_completion"]["observed_utc"]
        )
        self.assertLessEqual(record["log_completion"]["observed_utc"], record["finished_utc"])
        self.assertEqual(target.read_bytes(), b"ordinary uninstaller fixture")

    def test_uninstaller_callback_waits_for_async_completion_before_original_checks(self):
        self.assert_async_uninstaller_completion(
            "async-uninstaller",
            b'2026-10-03 09:37:37.195   Creating "_unins-done.tmp" file.\r\n',
            UNINSTALL_COMPLETION,
        )

    def test_uninstaller_waits_for_async_final_crlf_before_pinning_log(self):
        for missing in (2, 1):
            with self.subTest(missing_final_bytes=missing):
                self.assert_async_uninstaller_completion(
                    f"async-final-crlf-{missing}",
                    UNINSTALL_COMPLETION[:-missing],
                    UNINSTALL_COMPLETION[-missing:],
                )

    def test_uninstaller_incomplete_or_failed_terminal_log_never_returns_to_checks(self):
        for defect, contents, error in (
            ("incomplete", b"child still running\r\n", TimeoutError),
            ("missing-log", None, TimeoutError),
            (
                "failed",
                UNINSTALL_COMPLETION.replace(b"process succeeded", b"process failed"),
                ValueError,
            ),
            (
                "not-removed",
                UNINSTALL_COMPLETION.replace(b"Removed all? Yes", b"Removed all? No"),
                ValueError,
            ),
            ("closed-only", UNINSTALL_COMPLETION.splitlines(keepends=True)[-1], ValueError),
            ("lf-only", UNINSTALL_COMPLETION.replace(b"\r\n", b"\n"), ValueError),
            ("first-line-lf", UNINSTALL_COMPLETION.replace(b"\r\n", b"\n", 1), ValueError),
            ("trailing-data", UNINSTALL_COMPLETION + b"unexpected data\r\n", ValueError),
        ):
            with self.subTest(defect=defect):
                case = self.case("negative-" + defect)
                target = self.file(defect + "/unins000.exe")
                case.uninstallers[str(target.resolve())] = functional.digest(target)

                def launcher(command, **_kwargs):
                    if contents is not None:
                        (case.args.output / "process-1.log").write_bytes(contents)
                    return subprocess.CompletedProcess(command, 0)

                case.original_run = Mock(side_effect=launcher)
                clock = types.SimpleNamespace(monotonic=Mock(side_effect=[0.0, 31.0]), sleep=Mock())
                post_check = Mock()
                with patch.object(functional, "time", clock, create=True):
                    with self.assertRaises(error):
                        case.installer_run([target])
                        post_check()
                post_check.assert_not_called()
                self.assertTrue(case.record["errors"])
                record = case.record["processes"][0]
                self.assertEqual(record["status"], "launcher_returned")
                self.assertNotIn("log_sha256", record)
                self.assertIn("launcher_returned_utc", record)
                self.assertIn("error", record)

    def test_nonzero_uninstaller_launcher_rejects_even_with_success_log(self):
        case = self.case("failed-launcher")
        target = self.file("failed-launcher-target/unins000.exe")
        case.uninstallers[str(target.resolve())] = functional.digest(target)

        def launcher(command, **_kwargs):
            (case.args.output / "process-1.log").write_bytes(UNINSTALL_COMPLETION)
            return subprocess.CompletedProcess(command, 7)

        case.original_run = Mock(side_effect=launcher)
        with self.assertRaisesRegex(ValueError, "did not return successfully"):
            case.installer_run([target])
        record = case.record["processes"][0]
        self.assertEqual(record["exit_code"], 7)
        self.assertNotIn("log_sha256", record)
        self.assertTrue(case.record["errors"])

    def test_replaced_setup_is_not_executed(self):
        case = self.case()
        setup = self.file("reshiki-fixture-setup.exe")
        case.retain_identity("setup", setup, newline="\n")
        setup.write_bytes(b"changed setup")
        case.original_run = Mock()
        with self.assertRaisesRegex(ValueError, "retained identity"):
            case.installer_run([setup])
        case.original_run.assert_not_called()
        self.assertTrue(case.record["errors"])

    def test_archive_hook_delegates_original_assertions_with_bound_provenance(self):
        case = self.case()
        name = "reshiki-fixture-windows-x64"
        folder = self.root / "build/release-bundles" / name
        app = self.file(str(folder.relative_to(self.root) / "reshiki.exe"))
        metadata = {
            "commit": case.args.source_commit,
            "architecture": "x64",
            "signed": False,
            "notarized": False,
        }
        (folder / "build.json").write_text(json.dumps(metadata), encoding="utf-8")
        archive = self.root / (name + ".zip")
        with zipfile.ZipFile(archive, "w") as stream:
            stream.write(app, name + "/reshiki.exe")
        case.original_archive = Mock()
        case.verify_archive(archive)
        case.original_archive.assert_called_once_with(archive, False)
        self.assertEqual(case.record["package_metadata"]["commit"], case.args.source_commit)
        self.assertEqual(case.record["artifacts"]["payload"]["sha256"], functional.digest(app))
        self.assertEqual(case.record["artifacts"]["archive"]["checksum_newline"], "\n")

    def test_final_artifacts_require_lf_sidecars_on_every_host(self):
        for newline in ("\n", "\r\n"):
            with patch.object(functional.os, "linesep", newline):
                case, paths = self.artifacts("newlines-" + str(len(newline)))
            case.final_identity()
            self.assertEqual(case.record["errors"], [])
            self.assertEqual(
                {item["status"] for item in case.record["final_artifacts"].values()}, {"verified"}
            )
            for name in ("archive", "setup"):
                with self.subTest(host_newline=newline, artifact=name):
                    sidecar = Path(str(paths[name]) + ".sha256")
                    original = sidecar.read_bytes()
                    sidecar.write_bytes(original.replace(b"\n", b"\r\n"))
                    case.final_identity()
                    self.assertEqual(case.record["final_artifacts"][name]["status"], "incomplete")
                    sidecar.write_bytes(original)

    def test_original_archive_and_installer_writers_match_retained_lf_contract(self):
        case = self.case("original-writers")
        case.original_archive = Mock()
        self.file("target/" + case.args.target + "/release/reshiki.exe")
        with (
            patch.object(functional.build_release, "version", return_value="1.2.3"),
            patch.object(functional.build_release.platform, "system", return_value="Windows"),
            patch.object(functional.build_release.platform, "machine", return_value="AMD64"),
            patch.object(
                functional.build_release, "target_directory", return_value=self.root / "target"
            ),
            patch.object(functional.build_release, "notices"),
            patch.object(functional.build_release, "verify_binary"),
            patch.object(functional.build_release, "run"),
            patch.object(functional.build_release, "verify_archive", case.verify_archive),
            patch(
                "build_inchi_helper.dependency",
                return_value={"name": "cosmolkit-inchi", "version": "0.3.0"},
            ),
            patch.object(sys, "argv", ["build_release.py", "--target", case.args.target]),
            patch.object(functional.os, "linesep", "\r\n"),
            patch("builtins.print"),
        ):
            functional.build_release.main()
        setup = self.file("original-setup.exe")
        case.retain_identity("setup", setup, newline="\n")
        functional.installers.checksum(setup)
        for role in ("uninstaller-1", "uninstaller-2"):
            case.retain_identity(role, self.file(role))
        case.final_identity()
        self.assertEqual(case.record["errors"], [])
        for name in ("archive", "setup"):
            item = case.record["final_artifacts"][name]
            self.assertEqual(item["status"], "verified")
            self.assertNotIn(b"\r", Path(item["checksum_path"]).read_bytes())

    def test_final_container_or_saved_uninstaller_change_and_checksum_errors_fail(self):
        for defect in ("container", "saved-uninstaller", "missing", "newline", "extra-content"):
            with self.subTest(defect=defect):
                case, paths = self.artifacts(defect)
                if defect == "container":
                    paths["archive"].write_bytes(b"changed archive after assertions")
                elif defect == "saved-uninstaller":
                    paths["uninstaller-2"].write_bytes(b"changed saved uninstaller")
                elif defect == "missing":
                    paths["payload"].unlink()
                else:
                    sidecar = Path(str(paths["setup"]) + ".sha256")
                    if defect == "newline":
                        sidecar.write_bytes(sidecar.read_bytes().replace(b"\n", b"\r\n"))
                    else:
                        sidecar.write_bytes(sidecar.read_bytes() + b"\n")
                case.final_identity()
                self.assertTrue(case.record["errors"])
                self.assertTrue(
                    any(
                        item["status"] == "incomplete"
                        for item in case.record["final_artifacts"].values()
                    )
                )

    def test_protocol_skip_cannot_be_a_functional_pass(self):
        case = self.case()
        case.app = self.file("reshiki.exe")
        result = Mock(testsRun=5, skipped=[("fixture", "unavailable")])
        result.wasSuccessful.return_value = True
        with (
            patch.object(
                unittest.defaultTestLoader, "loadTestsFromNames", return_value=unittest.TestSuite()
            ),
            patch.object(unittest.TextTestRunner, "run", return_value=result),
        ):
            with self.assertRaisesRegex(ValueError, "failed, skipped or incomplete"):
                case.protocol()

    def test_protocol_loads_verified_root_from_clean_script_entrypoint(self):
        fixture = self.root / "entry-fixture"
        prefix = "entry-fixture/"
        self.file(
            prefix + "scripts/collect_windows_functional.py",
            Path(functional.__file__).read_bytes(),
        )
        self.file(
            prefix + "scripts/build_release.py",
            b"from pathlib import Path\nROOT = Path(__file__).resolve().parents[1]\n",
        )
        self.file(prefix + "scripts/installers.py", b"")
        self.file(prefix + "scripts/windows_security_evidence.py", b"")
        self.file(prefix + "tests/__init__.py", b"")
        self.file(prefix + "ordinary-app-fixture.txt", b"never executed")
        protocol_tests = "import os\nimport unittest\nfrom pathlib import Path\n\nclass ProtocolTests(unittest.TestCase):\n"
        for number in range(4):
            protocol_tests += (
                f"    def test_protocol_fixture_{number}(self):\n"
                "        self.assertEqual(os.environ['RESHIKI_REQUIRE_INCHI_HELPER'], '1')\n"
                "        self.assertEqual(Path(os.environ['RESHIKI_TEST_PACKAGED_APP']).read_text(), 'never executed')\n"
            )
        self.file(prefix + "tests/test_inchi_helper_protocol.py", protocol_tests.encode())
        self.file(
            prefix + "tests/test_inchi_distribution.py",
            b"import unittest\nclass HelperTests(unittest.TestCase):\n"
            b"    def test_packaged_application_worker_executes_when_supplied_for_validation(self):\n"
            b"        self.assertTrue(__file__.endswith('test_inchi_distribution.py'))\n",
        )
        probe = self.file(
            prefix + "scripts/probe.py",
            b"import json, os, sys\nfrom pathlib import Path\n"
            b"import collect_windows_functional as functional\n"
            b"root = Path(__file__).resolve().parents[1]\n"
            b"assert 'PYTHONPATH' not in os.environ\n"
            b"assert str(root) not in sys.path\n"
            b"assert 'tests' not in sys.modules\n"
            b"original_path = list(sys.path)\n"
            b"case = functional.FunctionalRun.__new__(functional.FunctionalRun)\n"
            b"case.app = root / 'ordinary-app-fixture.txt'\n"
            b"case.record = {}\ncase.protocol()\n"
            b"assert sys.path == original_path\n"
            b"assert Path(sys.modules['tests'].__file__).resolve() == root / 'tests/__init__.py'\n"
            b"print(json.dumps(case.record['protocol']))\n",
        )
        elsewhere = self.root / "unrelated-working-directory"
        elsewhere.mkdir()
        environment = dict(os.environ)
        environment.pop("PYTHONPATH", None)
        environment.pop("PYTHONHOME", None)
        result = subprocess.run(
            [sys.executable, "-S", str(probe)],
            cwd=elsewhere,
            env=environment,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            json.loads(result.stdout), {"tests_run": 5, "skipped": [], "successful": True}
        )
        self.assertTrue(fixture.is_dir())

    def test_build_failure_retains_raw_context_and_never_claims_av(self):
        case = self.case()
        with patch.object(
            functional.build_release, "main", side_effect=RuntimeError("compile failed")
        ):
            with self.assertRaisesRegex(RuntimeError, "compile failed"):
                case.build()
        record = json.loads((case.args.output / "functional.json").read_text())
        self.assertEqual(record["status"], "failed")
        self.assertEqual(record["antivirus_qualification"], functional.NO_AV)
        self.assertEqual(record["after"], self.state)
        self.assertIn("compile failed", record["errors"][0])

    def test_missing_preflight_still_writes_final_context_without_a_verdict(self):
        args = argparse.Namespace(
            preflight=self.root / "missing.json", output=self.root / "final.json"
        )
        functional.final_capture(args)
        record = json.loads(args.output.read_text())
        self.assertIn("preflight_error", record)
        self.assertEqual(record["antivirus_qualification"], functional.NO_AV)
        self.assertEqual(record["after"], self.state)
        self.assertEqual(record["events"], self.state)
        self.assertEqual(record["workflow_run"], functional.workflow_run())

    def test_final_context_keeps_rejected_preflight_identity_separate_from_current_run(self):
        current_run = functional.workflow_run()
        old_run = {**current_run, "run_attempt": "2"}
        preflight = self.file(
            "old-preflight.json",
            json.dumps(
                {
                    "status": "disposable_native_context_verified",
                    "workflow_run": old_run,
                    "started_utc": "2026-01-01T00:00:00Z",
                    "before": {"old_host": True},
                    "source_equivalence": {"old_source": True},
                }
            ).encode(),
        )
        args = argparse.Namespace(preflight=preflight, output=self.root / "current-final.json")
        functional.final_capture(args)
        record = json.loads(args.output.read_text())
        self.assertEqual(record["workflow_run"], current_run)
        self.assertEqual(record["preflight_workflow_run"], old_run)
        self.assertFalse(record["preflight_belongs_to_current_run"])
        self.assertIn("preflight_identity_error", record)
        self.assertNotIn("before", record)
        self.assertNotIn("source_equivalence", record)
        self.assertEqual(record["started_utc"], record["captured_utc"])
        self.assertEqual(record["after"], self.state)
        self.assertEqual(record["events"], self.state)

    def test_final_identity_and_snapshot_errors_do_not_prevent_event_capture(self):
        args = argparse.Namespace(
            preflight=self.root / "missing.json", output=self.root / "failed-context.json"
        )
        with (
            patch.dict(os.environ, GITHUB_RUN_ID=""),
            patch.object(functional, "observation", side_effect=[OSError("snapshot"), self.state]),
        ):
            functional.final_capture(args)
        record = json.loads(args.output.read_text())
        self.assertIn("workflow_run_error", record)
        self.assertIn("preflight_error", record)
        self.assertIn("after_error", record)
        self.assertNotIn("workflow_run", record)
        self.assertEqual(record["events"], self.state)


if __name__ == "__main__":
    unittest.main()
