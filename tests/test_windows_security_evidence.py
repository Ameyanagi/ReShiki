"""Synthetic evidence must never turn a skipped, unrelated or failed scan into a pass."""

import argparse
import copy
import csv
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from xml.sax.saxutils import escape

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import windows_security_evidence as security

TARGET = r"C:\AV test\reshiki.exe"
DIGEST = "a" * 64
START = "2026-10-02T01:00:00+00:00"
END = "2026-10-02T01:01:00+00:00"


def event(number, fields, when="2026-10-02T01:00:02Z"):
    contents = "".join(
        f'<Data Name="{escape(key)}">{escape(value)}</Data>' for key, value in fields.items()
    )
    return (
        f'<Event xmlns="http://schemas.microsoft.com/win/2004/08/events/event">'
        f'<System><Provider Name="{security.PROVIDER}"/><EventID>{number}</EventID>'
        f'<TimeCreated SystemTime="{when}"/></System><EventData>{contents}</EventData></Event>'
    )


def fixture():
    snapshot = {
        "status": {
            "ok": True,
            "data": dict.fromkeys(security.ACTIVE_FIELDS, True)
            | {
                "AMRunningMode": "Normal",
                "AMProductVersion": "platform",
                "AMEngineVersion": "engine",
                "AntivirusSignatureVersion": "definition",
                "AntivirusSignatureLastUpdated": START,
                "DefenderSignaturesOutOfDate": False,
            },
        },
        "preferences": {
            "ok": True,
            "data": {
                "DisableRealtimeMonitoring": False,
                "DisableBehaviorMonitoring": False,
                "DisableIOAVProtection": False,
                "DisableArchiveScanning": False,
                "ExclusionPath": None,
                "ExclusionExtension": [],
                "ExclusionProcess": None,
            },
        },
        "host": {
            "ok": True,
            "data": {"caption": "Windows test fixture", "system_type": "x64-based PC"},
        },
        "threats": {"ok": True, "data": None},
    }
    return {
        "schema_version": 1,
        "request": {
            "case_id": "D-X64-PAYLOAD",
            "release_tag": "v1.0.0",
            "source_commit": "b" * 40,
            "target": TARGET,
            "expected_sha256": DIGEST,
            "architecture": "x64",
            "kind": "payload",
            "phase": "after-extraction",
            "since_utc": START,
        },
        "before": copy.deepcopy(snapshot),
        "after": copy.deepcopy(snapshot),
        "file_before": {"ok": True, "sha256": DIGEST},
        "file_after": {"ok": True, "sha256": DIGEST},
        "scan": {"state": "returned", "exit_code": 0, "started_utc": START},
        "finished_utc": END,
        "events": {
            "ok": True,
            "data": {
                "truncated": False,
                "records": [
                    event(1000, {"Scan ID": "{scan-1}", "Scan Resources": TARGET}),
                    event(1001, {"Scan ID": "{scan-1}"}, "2026-10-02T01:00:30Z"),
                ],
            },
        },
    }


class SecurityEvidenceTests(unittest.TestCase):
    def test_complete_scan_is_limited_to_exact_file_and_recorded_versions(self):
        report = security.evaluate(fixture())
        self.assertEqual(report["status"], "scan_completed_no_detection")
        self.assertEqual(report["scan_id"], "{scan-1}")
        self.assertEqual(report["versions_before"], report["versions_after"])
        self.assertIn("install", report["scope"])
        self.assertIsNone(report["signature"], "No signature must not fabricate trust")

    def test_native_file_resource_prefix_and_localized_scan_name(self):
        evidence = fixture()
        evidence["events"]["data"]["records"][0] = event(
            1000,
            {
                "Scan ID": "{scan-1}",
                "Scan Resources": "file:_" + TARGET,
                "Scan Parameters": "カスタム スキャン",
                "Scan Parameters Index": "3",
            },
        )
        self.assertEqual(security.evaluate(evidence)["status"], "scan_completed_no_detection")

    def test_exit_zero_without_unique_matching_completion_is_incomplete(self):
        replacements = [
            [],
            [event(1001, {"Scan ID": "{scan-1}"})],
            [event(1000, {"Scan ID": "{scan-1}", "Scan Resources": TARGET})],
            [
                event(1000, {"Scan ID": "{other}", "Scan Resources": TARGET}),
                event(1001, {"Scan ID": "{scan-1}"}),
            ],
            [
                event(1000, {"Scan ID": "{scan-1}", "Scan Resources": TARGET + ".old"}),
                event(1001, {"Scan ID": "{scan-1}"}),
            ],
        ]
        for records in replacements:
            with self.subTest(records=records):
                evidence = fixture()
                evidence["events"]["data"]["records"] = records
                self.assertEqual(security.evaluate(evidence)["status"], "incomplete")
        for additional in [
            event(1002, {"Scan ID": "{scan-1}"}),
            event(1005, {"Scan ID": "{scan-1}"}),
            fixture()["events"]["data"]["records"][0],
        ]:
            evidence = fixture()
            evidence["events"]["data"]["records"].append(additional)
            self.assertEqual(security.evaluate(evidence)["status"], "incomplete")

    def test_unavailable_truncated_malformed_or_out_of_window_events_never_pass(self):
        for section in [
            {"ok": False, "error": "Access denied"},
            {
                "ok": True,
                "data": {"truncated": True, "records": fixture()["events"]["data"]["records"]},
            },
            {"ok": True, "data": {"truncated": False, "records": ["broken xml"]}},
            {
                "ok": True,
                "data": {"truncated": False, "records": [event(1000, {}, "2026-10-01T01:00:00Z")]},
            },
        ]:
            with self.subTest(section=section):
                evidence = fixture()
                evidence["events"] = section
                self.assertEqual(security.evaluate(evidence)["status"], "incomplete")

    def test_disabled_passive_unknown_stale_and_excluded_protection_never_pass(self):
        mutations = [("status", key, False) for key in security.ACTIVE_FIELDS] + [
            ("status", "AMRunningMode", "Passive"),
            ("status", "AMRunningMode", None),
            ("status", "AMEngineVersion", None),
            ("status", "DefenderSignaturesOutOfDate", True),
            ("preferences", "ExclusionPath", ["C:\\"]),
            ("preferences", "ExclusionExtension", ["exe"]),
            ("preferences", "DisableArchiveScanning", True),
        ]
        for phase in ("before", "after"):
            for section, key, value in mutations:
                with self.subTest(phase=phase, key=key):
                    evidence = fixture()
                    evidence[phase][section]["data"][key] = value
                    self.assertEqual(security.evaluate(evidence)["status"], "incomplete")
            for section in ("status", "preferences", "threats", "host"):
                evidence = fixture()
                evidence[phase][section] = {"ok": False, "error": "No provider"}
                self.assertEqual(security.evaluate(evidence)["status"], "incomplete")

    def test_missing_changed_hash_timeout_skip_and_definition_change_never_pass(self):
        for name in ("file_before", "file_after"):
            for value in ({"ok": False, "error": "file missing"}, {"ok": True, "sha256": "c" * 64}):
                evidence = fixture()
                evidence[name] = value
                self.assertEqual(security.evaluate(evidence)["status"], "incomplete")
        for state in ("not_requested", "skipped", "timed_out", "completion_not_observed", "error"):
            evidence = fixture()
            evidence["scan"]["state"] = state
            self.assertEqual(security.evaluate(evidence)["status"], "incomplete")
        evidence = fixture()
        evidence["after"]["status"]["data"]["AntivirusSignatureVersion"] = "new definition"
        self.assertEqual(security.evaluate(evidence)["status"], "incomplete")

    def test_quarantined_target_is_a_detection_even_when_command_succeeds(self):
        for fields in (
            {"Path": "file:_" + TARGET, "Action": "Quarantine"},
            {"Path": "containerfile:_" + TARGET + ";file:_nested.exe", "Action": "Allow"},
        ):
            evidence = fixture()
            evidence["events"]["data"]["records"].append(event(1117, fields))
            evidence["file_after"] = {"ok": False, "error": "Quarantined"}
            self.assertEqual(security.evaluate(evidence)["status"], "detection_recorded")
        evidence = fixture()
        evidence["after"]["threats"]["data"] = [{"Resources": ["file:_" + TARGET]}]
        self.assertEqual(security.evaluate(evidence)["status"], "detection_recorded")

    def test_unattributed_new_threat_requires_review_but_old_unrelated_history_does_not(self):
        evidence = fixture()
        old = {"Resources": [r"file:_C:\other.exe"], "DetectionID": "old"}
        evidence["before"]["threats"]["data"] = [old]
        evidence["after"]["threats"]["data"] = [old]
        self.assertEqual(security.evaluate(evidence)["status"], "scan_completed_no_detection")
        evidence["after"]["threats"]["data"] = [old, {"Resources": ["file:_" + TARGET + ".old"]}]
        self.assertEqual(security.evaluate(evidence)["status"], "incomplete")
        self.assertFalse(security.resource_matches("file:_" + TARGET + ".old", TARGET))

    def test_portable_matrix_recomputes_status_instead_of_trusting_stale_report(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "evidence.json"
            evidence = fixture()
            evidence["status"] = "clean"
            evidence["file_after"]["sha256"] = "0" * 64
            security.write_json(source, evidence)
            self.assertEqual(security.summarize([source], root / "matrix"), 1)
            reports = json.loads((root / "matrix/matrix.json").read_text())
            self.assertEqual(reports[0]["status"], "incomplete")
            with (root / "matrix/matrix.csv").open(encoding="utf-8-sig", newline="") as stream:
                self.assertEqual(list(csv.DictReader(stream))[0]["expected_sha256"], DIGEST)
            with self.assertRaises(FileExistsError):
                security.summarize([source], root / "matrix")

    def test_probe_timeout_and_unavailable_powershell_preserve_failure(self):
        with patch.object(
            security.subprocess, "run", side_effect=security.subprocess.TimeoutExpired("scan", 1)
        ):
            self.assertEqual(security.run_probe("scan", TARGET, START, 1)["state"], "timed_out")
        with patch.object(
            security.subprocess, "run", side_effect=FileNotFoundError("powershell.exe")
        ):
            self.assertEqual(security.run_probe("snapshot", TARGET, START, 1)["ok"], False)

    def test_collector_writes_raw_evidence_and_skips_wrong_hash_before_scan(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / "ordinary text fixture.txt"
            target.write_text(
                "A harmless collector fixture, not an application or AV test sample.\n"
            )
            actual = security.fingerprint(target)["sha256"]
            for digest in (actual, DIGEST):
                calls = []

                def probe(mode, target, since, timeout):
                    calls.append(mode)
                    if mode == "snapshot":
                        return fixture()["before"]
                    if mode == "scan":
                        return {"state": "returned", "exit_code": 0}
                    return {
                        "ok": True,
                        "data": {
                            "truncated": False,
                            "records": [
                                event(
                                    1000,
                                    {"Scan ID": "{fixture-scan}", "Scan Resources": target},
                                    END,
                                ),
                                event(1001, {"Scan ID": "{fixture-scan}"}, END),
                            ],
                        },
                    }

                args = argparse.Namespace(
                    output=root / digest,
                    target=target,
                    since=START,
                    case_id="FIXTURE",
                    release_tag="fixture",
                    source_commit="b" * 40,
                    sha256=digest,
                    architecture="x64",
                    kind="fixture",
                    phase="collector-test",
                    source_url="local:fixture",
                    container_sha256=None,
                    scan=True,
                    timeout=1,
                )
                with (
                    patch.object(security.sys, "platform", "win32"),
                    patch.object(security, "utc_now", return_value=END),
                    patch.object(security, "run_probe", side_effect=probe),
                ):
                    result = security.collect(args)
                raw = json.loads((args.output / "evidence.json").read_text())
                report = json.loads((args.output / "report.json").read_text())
                self.assertEqual(raw["file_after"]["sha256"], actual)
                if digest == actual:
                    self.assertEqual(result, 0)
                    self.assertEqual(report["status"], "scan_completed_no_detection")
                    self.assertEqual(calls.count("scan"), 1)
                else:
                    self.assertEqual(result, 1)
                    self.assertEqual(report["status"], "incomplete")
                    self.assertNotIn("scan", calls)

    def test_fingerprint_records_pe_stub_architecture_separately_from_target_label(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "synthetic.exe"
            header = bytearray(64)
            header[:2] = b"MZ"
            header[60:64] = (64).to_bytes(4, "little")
            path.write_bytes(header + b"PE\0\0\x4c\x01")
            self.assertEqual(security.fingerprint(path)["pe_machine"], "0x014c")


if __name__ == "__main__":
    unittest.main()
