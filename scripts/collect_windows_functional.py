"""Unpublished native package assertions on disposable Windows Actions hosts.

This lane performs no antivirus qualification. Protection observations are raw
context only; the separate protected harness is neither imported nor weakened.
"""

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time
import unittest
import zipfile
from collections import Counter
from pathlib import Path
from typing import Any
from unittest.mock import patch

import build_release
import installers
import windows_security_evidence as security

ALLOWED_DIFF = {
    ".github/workflows/release.yml": "M",
    "scripts/collect_windows_functional.py": "A",
    "tests/test_windows_functional_evidence.py": "A",
}
TARGETS = {"x64": "x86_64-pc-windows-msvc", "arm64": "aarch64-pc-windows-msvc"}
NO_AV = "not_performed_functional_only"


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def completed_uninstaller_log(path, timeout=30):
    """Wait for Inno's child to close its successful log after launcher return."""
    deadline = time.monotonic() + timeout
    terminal = (
        b"Uninstallation process succeeded.",
        b"Removed all? Yes",
        b"Need to restart Windows? No",
        b"Log closed.",
    )
    timestamp = rb"[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{3}   "
    while True:
        try:
            contents = path.read_bytes()
        except FileNotFoundError:
            contents = b""
        lines = contents.splitlines(keepends=True)
        closed = [
            i
            for i, line in enumerate(lines)
            if line.removesuffix(b"\n").removesuffix(b"\r").endswith(b"   Log closed.")
        ]
        if closed:
            if closed != [len(lines) - 1] or len(lines) < len(terminal):
                raise ValueError("Uninstaller closed its log without complete terminal success")
            # The child may have written the last message but not its CRLF yet.
            # Do not pin a prefix while the last record is still being written.
            if contents.endswith(b"\n"):
                if any(
                    not re.fullmatch(timestamp + re.escape(message) + rb"\r\n", line)
                    for line, message in zip(lines[-len(terminal) :], terminal)
                ):
                    raise ValueError("Uninstaller closed its log without complete terminal success")
                # Pin exactly this final read, not a second read that could differ.
                return contents
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError(
                "Uninstaller terminal log completion was not observed within timeout"
            )
        time.sleep(min(0.05, remaining))


def git(*arguments):
    return subprocess.check_output(["git", *arguments], cwd=build_release.ROOT)


def tree(commit):
    result = {}
    for entry in git("ls-tree", "-r", "-z", commit).split(b"\0"):
        if entry:
            attributes, name = entry.split(b"\t", 1)
            result[name.decode("utf-8")] = attributes.decode("ascii")
    return result


def source_equivalence(candidate, head):
    if any(not re.fullmatch(r"[0-9a-f]{40}", value) for value in (candidate, head)):
        raise ValueError("Full candidate and harness commit hashes are required")
    if git("rev-parse", "HEAD").decode().strip() != head or os.environ.get("GITHUB_SHA") != head:
        raise ValueError("Harness checkout and workflow SHA disagree")
    parents = git("rev-list", "--parents", "-n", "1", "HEAD").decode().split()
    if parents != [head, candidate]:
        raise ValueError("Use one isolated overlay commit directly on the frozen candidate")
    if git("status", "--porcelain", "--untracked-files=no"):
        raise ValueError("Tracked worktree files changed after checkout")
    fields = git("diff", "--name-status", "--no-renames", "-z", candidate, head).split(b"\0")
    changes = {fields[i + 1].decode(): fields[i].decode() for i in range(0, len(fields) - 1, 2)}
    if changes != ALLOWED_DIFF:
        raise ValueError("Overlay must change exactly its three allowed validation files")
    before, after = tree(candidate), tree(head)
    unchanged = sorted((set(before) | set(after)) - ALLOWED_DIFF.keys())
    if any(before.get(path) != after.get(path) for path in unchanged):
        raise ValueError("Production file modes or Git objects changed")
    original_workflow = git("show", candidate + ":.github/workflows/release.yml")
    return {
        "production_candidate": candidate,
        "production_tree": git("rev-parse", candidate + "^{tree}").decode().strip(),
        "harness_head": head,
        "harness_tree": git("rev-parse", head + "^{tree}").decode().strip(),
        "allowed_diff": changes,
        "unchanged_tracked_path_count": len(unchanged),
        "unchanged_path_modes_and_git_objects": True,
        "production_workflow_sha256": hashlib.sha256(original_workflow).hexdigest(),
        "changed_files": {
            path: {
                "before_git_entry": before.get(path),
                "after_git_entry": after[path],
                "checked_out_sha256": digest(build_release.ROOT / path),
            }
            for path in ALLOWED_DIFF
        },
    }


def require_disposable_host(architecture, observation):
    if sys.platform != "win32" or os.environ.get("GITHUB_ACTIONS") != "true":
        raise ValueError("Disposable Windows Actions host required")
    host = security.data(observation, "host") or {}
    if architecture not in str(host.get("system_type", "")).lower():
        raise ValueError("Native host architecture does not match the package")
    import winreg

    for name in (
        r"Software\Classes\CLSID\{3BAC2B7E-73A2-4F3A-9CE7-5E9B438C59B4}",
        r"Software\Classes\ReShiki.EmbeddedDrawing.1",
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall\dev.reshiki.editor_is1",
    ):
        try:
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, name):
                pass
        except FileNotFoundError:
            continue
        raise ValueError("Existing ReShiki registration; refusing installer checks")


def observation(mode, since):
    return security.run_probe(mode, str(build_release.ROOT / "Cargo.toml"), since, 60)


def workflow_run():
    repository = os.environ.get("GITHUB_REPOSITORY", "")
    run_id = os.environ.get("GITHUB_RUN_ID", "")
    attempt = os.environ.get("GITHUB_RUN_ATTEMPT", "")
    if not re.fullmatch(r"[^/\s]+/[^/\s]+", repository) or any(
        not re.fullmatch(r"[1-9][0-9]*", value) for value in (run_id, attempt)
    ):
        raise ValueError("Workflow repository, run ID and attempt are required")
    return {
        "repository": repository,
        "run_id": run_id,
        "run_attempt": attempt,
        "job": os.environ.get("GITHUB_JOB"),
        "workflow_ref": os.environ.get("GITHUB_WORKFLOW_REF"),
        "workflow_sha": os.environ.get("GITHUB_WORKFLOW_SHA"),
        "checkout_sha": os.environ.get("GITHUB_SHA"),
        "run_url": f"https://github.com/{repository}/actions/runs/{run_id}/attempts/{attempt}",
    }


def preflight(args):
    args.output.mkdir(parents=True, exist_ok=False)
    record = {
        "started_utc": security.utc_now(),
        "architecture": args.architecture,
        "status": "incomplete",
        "antivirus_qualification": NO_AV,
    }
    try:
        record["workflow_run"] = workflow_run()
        record["before"] = observation("snapshot", record["started_utc"])
        record["source_equivalence"] = source_equivalence(args.candidate, args.source_commit)
        require_disposable_host(args.architecture, record["before"])
        record["status"] = "disposable_native_context_verified"
    except BaseException as error:
        record["error"] = repr(error)
        raise
    finally:
        security.write_json(args.output / "preflight.json", record)


class FunctionalRun:
    def __init__(self, args, initial):
        self.args = args
        self.original_run = installers.run
        self.original_archive = build_release.verify_archive
        self.original_installer = installers.verify_windows_installer
        self.app = None
        self.uninstallers = {}
        self.record: dict[str, Any] = {
            "started_utc": security.utc_now(),
            "source_equivalence": initial["source_equivalence"],
            "workflow_run": initial["workflow_run"],
            "architecture": args.architecture,
            "antivirus_qualification": NO_AV,
            "scope": "Native functional assertions only; no protected AV, browser/MOTW, standard-user, clean-client CRT, GUI/Office, Windows10 or Bitdefender acceptance.",
            "status": "running",
            "errors": [],
            "artifacts": {},
            "processes": [],
            "installations": [],
        }
        self.write()

    def write(self):
        try:
            self.record["finished_utc"] = security.utc_now()
            security.write_json(self.args.output / "functional.json", self.record)
        except BaseException as error:
            self.record["errors"].append(f"Evidence write failed: {error}")
            self.record["status"] = "failed"
            raise

    def retain_identity(self, name, path, *, newline=None):
        if name in self.record["artifacts"]:
            raise ValueError(f"Duplicate artifact identity: {name}")
        item = {"path": str(path.resolve()), "sha256": digest(path), "size": path.stat().st_size}
        if newline is not None:
            item.update(checksum_path=str(path.resolve()) + ".sha256", checksum_newline=newline)
        self.record["artifacts"][name] = item
        self.write()
        return item["sha256"]

    def verify_archive(self, archive, signed=False):
        if signed:
            raise ValueError("Functional lane produces unsigned packages only")
        folder = build_release.ROOT / "build/release-bundles" / archive.stem
        metadata = json.loads((folder / "build.json").read_text(encoding="utf-8"))
        if (
            metadata["commit"] != self.args.source_commit
            or metadata["architecture"] != self.args.architecture
            or metadata["signed"] is not False
            or metadata["notarized"] is not False
        ):
            raise ValueError("Package provenance does not match this unsigned native job")
        self.record["package_metadata"] = metadata
        self.app = folder / "reshiki.exe"
        app_hash = self.retain_identity("payload", self.app)
        self.retain_identity("build_metadata", folder / "build.json")
        self.retain_identity("archive", archive, newline="\n")
        with zipfile.ZipFile(archive) as stream:
            if hashlib.sha256(stream.read(folder.name + "/reshiki.exe")).hexdigest() != app_hash:
                raise ValueError("Archive payload does not match the original source bytes")
        return self.original_archive(archive, signed)

    def verify_installer(self, setup, source):
        if digest(source / "reshiki.exe") != self.record["artifacts"]["payload"]["sha256"]:
            raise ValueError("Installer source differs from portable payload")
        self.retain_identity("setup", setup, newline="\n")
        return self.original_installer(setup, source)

    def installer_run(self, command, **kwargs):
        executable = Path(command[0]).resolve()
        setup = self.record["artifacts"].get("setup", {})
        if str(executable) == setup.get("path"):
            role, expected = "setup", setup["sha256"]
        elif re.fullmatch(r"unins\d{3}\.exe", executable.name.lower()):
            role, expected = "uninstaller", self.uninstallers.get(str(executable))
        else:
            return self.original_run(command, **kwargs)
        process = {
            "role": role,
            "command": [str(value) for value in command],
            "started_utc": security.utc_now(),
            "expected_sha256": expected,
            "status": "not_run",
        }
        self.record["processes"].append(process)
        try:
            self.write()
            if expected is None or digest(executable) != expected:
                raise ValueError("Installer process bytes differ from retained identity")
            log = (self.args.output / f"process-{len(self.record['processes'])}.log").resolve()
            process["log"] = str(log)
            result = self.original_run([*command, f"/LOG={log}"], **kwargs)
            process.update(
                status="launcher_returned",
                launcher_returned_utc=security.utc_now(),
                exit_code=result.returncode,
            )
            if result.returncode != 0:
                raise ValueError("Installer process did not return successfully")
            if role == "uninstaller":
                log_bytes = completed_uninstaller_log(log)
                process["log_completion"] = {
                    "status": "terminal_success_observed",
                    "observed_utc": security.utc_now(),
                }
            else:
                log_bytes = log.read_bytes()
            process.update(
                status="returned",
                log_sha256=hashlib.sha256(log_bytes).hexdigest(),
                log_size=len(log_bytes),
            )
            if role == "setup":
                destination = Path(
                    next(str(arg)[5:] for arg in command if str(arg).startswith("/DIR="))
                )
                app_hash = digest(destination / "reshiki.exe")
                if app_hash != self.record["artifacts"]["payload"]["sha256"]:
                    raise ValueError("Installed application differs from retained payload")
                number = len(self.record["installations"]) + 1
                uninstaller = destination / "unins000.exe"
                uninstaller_hash = digest(uninstaller)
                saved = self.args.output / f"uninstaller-{number}.exe"
                shutil.copyfile(uninstaller, saved)
                if self.retain_identity(f"uninstaller-{number}", saved) != uninstaller_hash:
                    raise ValueError("Saved uninstaller differs from installed bytes")
                self.uninstallers[str(uninstaller.resolve())] = uninstaller_hash
                self.record["installations"].append(
                    {
                        "number": number,
                        "setup_sha256": expected,
                        "installed_app_sha256": app_hash,
                        "uninstaller_sha256": uninstaller_hash,
                        "saved_uninstaller": str(saved),
                        "native_architecture": self.args.architecture,
                        "harness_source_commit": self.args.source_commit,
                    }
                )
            return result
        except BaseException as error:
            process["error"] = repr(error)
            self.record["errors"].append(f"Installer evidence/assertion failed: {error}")
            raise
        finally:
            process["finished_utc"] = security.utc_now()
            self.write()

    def protocol(self):
        if self.app is None:
            raise ValueError("No verified packaged native application")
        with (
            patch.object(sys, "path", [str(build_release.ROOT), *sys.path]),
            patch.dict(
                os.environ,
                RESHIKI_REQUIRE_INCHI_HELPER="1",
                RESHIKI_TEST_PACKAGED_APP=str(self.app),
            ),
        ):
            suite = unittest.defaultTestLoader.loadTestsFromNames(
                [
                    "tests.test_inchi_helper_protocol",
                    "tests.test_inchi_distribution.HelperTests.test_packaged_application_worker_executes_when_supplied_for_validation",
                ]
            )
            result = unittest.TextTestRunner(verbosity=2).run(suite)
            self.record["protocol"] = {
                "tests_run": result.testsRun,
                "skipped": result.skipped,
                "successful": result.wasSuccessful(),
            }
            if not result.wasSuccessful() or result.skipped or result.testsRun < 5:
                raise ValueError("Native worker protocol assertions failed, skipped or incomplete")

    def final_identity(self):
        expected = {
            "archive",
            "payload",
            "build_metadata",
            "setup",
            "uninstaller-1",
            "uninstaller-2",
        }
        if set(self.record["artifacts"]) != expected:
            self.record["errors"].append("Required final artifact inventory is incomplete")
        observations = self.record["final_artifacts"] = {}
        for name, identity in self.record["artifacts"].items():
            item = observations[name] = {**identity, "status": "incomplete"}
            try:
                path = Path(identity["path"])
                item["observed_sha256"] = digest(path)
                if item["observed_sha256"] != identity["sha256"]:
                    raise ValueError("Final bytes changed after native package verification")
                if "checksum_path" in identity:
                    expected_text = (
                        f"{identity['sha256']}  {path.name}{identity['checksum_newline']}".encode(
                            "ascii"
                        )
                    )
                    sidecar = Path(identity["checksum_path"])
                    with sidecar.open("rb") as stream:
                        actual = stream.read(len(expected_text) + 1)
                    item["checksum_prefix"] = actual.decode("ascii", errors="replace")
                    if actual != expected_text:
                        raise ValueError(
                            "Final checksum bytes differ from original writer contract"
                        )
                    item["checksum_sha256"] = digest(sidecar)
                item["status"] = "verified"
            except BaseException as error:
                item["error"] = repr(error)
                self.record["errors"].append(f"{name}: {error}")

    def build(self):
        try:
            with (
                patch.object(build_release, "verify_archive", self.verify_archive),
                patch.object(installers, "verify_windows_installer", self.verify_installer),
                patch.object(installers, "run", self.installer_run),
                patch.object(
                    sys, "argv", ["build_release.py", "--target", self.args.target, "--installer"]
                ),
            ):
                build_release.main()
            self.record["original_package_assertions"] = "passed"
            self.record["post_uninstall_context"] = observation(
                "snapshot", self.record["started_utc"]
            )
            require_disposable_host(self.args.architecture, self.record["post_uninstall_context"])
            self.protocol()
            roles = Counter(p["role"] for p in self.record["processes"] if p.get("exit_code") == 0)
            if roles != {"setup": 2, "uninstaller": 1} or len(self.record["installations"]) != 2:
                raise ValueError("Install, upgrade and uninstall evidence is incomplete")
            if (
                source_equivalence(self.args.candidate, self.args.source_commit)
                != self.record["source_equivalence"]
            ):
                raise ValueError("Source equivalence changed during package execution")
        except BaseException as error:
            self.record["errors"].append(f"Build/native assertion failed: {error}")
            raise
        finally:
            self.final_identity()
            try:
                self.record["after"] = observation("snapshot", self.record["started_utc"])
                self.record["events"] = observation("events", self.record["started_utc"])
            except BaseException as error:
                self.record["protection_observation_error"] = repr(error)
            self.record["status"] = (
                "failed" if self.record["errors"] else "functional_assertions_passed"
            )
            self.write()
        return int(bool(self.record["errors"]))


def final_capture(args):
    record = {"antivirus_qualification": NO_AV, "captured_utc": security.utc_now()}
    since = record["captured_utc"]
    try:
        record["workflow_run"] = workflow_run()
    except Exception as error:
        record["workflow_run_error"] = repr(error)
    try:
        initial = json.loads(args.preflight.read_text(encoding="utf-8-sig"))
        record["preflight_status"] = initial["status"]
        record["preflight_workflow_run"] = initial.get("workflow_run")
        record["preflight_before"] = initial.get("before")
        record["preflight_source_equivalence"] = initial.get("source_equivalence")
        record["preflight_started_utc"] = initial.get("started_utc")
        same_run = "workflow_run" in record and record["workflow_run"] == initial.get(
            "workflow_run"
        )
        record["preflight_belongs_to_current_run"] = same_run
        if same_run:
            record["before"] = initial.get("before")
            record["source_equivalence"] = initial.get("source_equivalence")
            since = initial["started_utc"]
        else:
            record["preflight_identity_error"] = "Preflight does not belong to the current run"
    except Exception as error:
        record["preflight_error"] = repr(error)
    record["started_utc"] = since
    for name, mode in (("after", "snapshot"), ("events", "events")):
        try:
            record[name] = observation(mode, since)
        except Exception as error:
            record[name + "_error"] = repr(error)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    if args.output.exists():
        raise ValueError("Existing final context will not be overwritten")
    security.write_json(args.output, record)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("preflight", "build", "final-capture"))
    parser.add_argument("--architecture", choices=TARGETS, required=True)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--target")
    parser.add_argument("--preflight", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.mode == "preflight":
        return preflight(args)
    if args.mode == "final-capture":
        return final_capture(args)
    if args.target != TARGETS[args.architecture]:
        raise ValueError("Native architecture and build target disagree")
    initial = json.loads(args.preflight.read_text(encoding="utf-8-sig"))
    if (
        initial["status"] != "disposable_native_context_verified"
        or initial["architecture"] != args.architecture
    ):
        raise ValueError("No verified disposable native context")
    if initial["workflow_run"] != workflow_run():
        raise ValueError("Preflight belongs to a different workflow run or attempt")
    if source_equivalence(args.candidate, args.source_commit) != initial["source_equivalence"]:
        raise ValueError("Preflight does not match source equivalence")
    require_disposable_host(args.architecture, observation("snapshot", initial["started_utc"]))
    args.output.mkdir(parents=True, exist_ok=False)
    return FunctionalRun(args, initial).build()


if __name__ == "__main__":
    sys.exit(main())
