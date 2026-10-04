"""Synthetic evidence must never turn a skipped, unrelated or failed scan into a pass."""

import argparse
import base64
import copy
import csv
import json
import shutil
import subprocess
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
                "UnknownThreatDefaultAction": 0,
                "LowThreatDefaultAction": 0,
                "ModerateThreatDefaultAction": 0,
                "HighThreatDefaultAction": 0,
                "SevereThreatDefaultAction": 0,
                "ThreatIDDefaultAction_Ids": None,
                "ThreatIDDefaultAction_Actions": None,
                "MAPSReporting": 2,
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

    def test_recovered_protection_failure_or_history_purge_never_passes(self):
        for number in (3002, 3007, 5001, 5004, 5007, 5008, 5010, 5012, 5101, 1013, 1014):
            with self.subTest(number=number):
                evidence = fixture()
                evidence["events"]["data"]["records"].append(
                    event(number, {}, "2026-10-02T01:00:15Z")
                )
                report = security.evaluate(evidence)
                self.assertEqual(report["status"], "incomplete")
                self.assertTrue(any(str(number) in reason for reason in report["reasons"]))

    def test_preexisting_target_detection_survives_history_disappearance(self):
        for purge in (False, True):
            evidence = fixture()
            evidence["before"]["threats"]["data"] = [
                {"Resources": ["file:_" + TARGET], "DetectionID": "target-before"}
            ]
            if purge:
                evidence["events"]["data"]["records"].append(event(1013, {}))
            report = security.evaluate(evidence)
            self.assertEqual(report["status"], "detection_recorded")
            self.assertTrue(any("disappeared" in reason for reason in report["reasons"]))
        evidence = fixture()
        evidence["before"]["threats"]["data"] = [
            {"Resources": [r"file:_C:\other.exe"], "DetectionID": "unrelated-before"}
        ]
        self.assertEqual(security.evaluate(evidence)["status"], "incomplete")

    def test_non_remediating_unknown_or_missing_default_actions_never_pass(self):
        keys = (
            "UnknownThreatDefaultAction",
            "LowThreatDefaultAction",
            "ModerateThreatDefaultAction",
            "HighThreatDefaultAction",
            "SevereThreatDefaultAction",
        )
        for key in keys:
            for action in (6, 8, 9, 11, 99, "Allow", None, False):
                with self.subTest(key=key, action=action):
                    evidence = fixture()
                    for phase in ("before", "after"):
                        evidence[phase]["preferences"]["data"][key] = action
                    self.assertEqual(security.evaluate(evidence)["status"], "incomplete")
            evidence = fixture()
            for phase in ("before", "after"):
                del evidence[phase]["preferences"]["data"][key]
            self.assertEqual(security.evaluate(evidence)["status"], "incomplete")

    def test_threat_specific_actions_require_known_remediation_and_valid_pairs(self):
        for ids, actions in (
            ([123], [6]),
            ([123], [8]),
            ([123], [9]),
            ([123], [11]),
            ([123], [99]),
            ([123], [None]),
            ([123], ["Quarantine"]),
            ([123], [False]),
            ([123], None),
            (None, [2]),
            ([123, 456], [2]),
            ([123, 123], [2, 3]),
            ([0], [2]),
            (["unknown"], [2]),
        ):
            with self.subTest(ids=ids, actions=actions):
                evidence = fixture()
                for phase in ("before", "after"):
                    prefs = evidence[phase]["preferences"]["data"]
                    prefs["ThreatIDDefaultAction_Ids"] = ids
                    prefs["ThreatIDDefaultAction_Actions"] = actions
                self.assertEqual(security.evaluate(evidence)["status"], "incomplete")
        for key in ("ThreatIDDefaultAction_Ids", "ThreatIDDefaultAction_Actions"):
            evidence = fixture()
            for phase in ("before", "after"):
                del evidence[phase]["preferences"]["data"][key]
            self.assertEqual(security.evaluate(evidence)["status"], "incomplete")
        for action in (0, 1, 2, 3, 10):
            evidence = fixture()
            for phase in ("before", "after"):
                prefs = evidence[phase]["preferences"]["data"]
                for key in security.DEFAULT_ACTION_FIELDS:
                    prefs[key] = action
                prefs["ThreatIDDefaultAction_Ids"] = [123]
                prefs["ThreatIDDefaultAction_Actions"] = [action]
            self.assertEqual(security.evaluate(evidence)["status"], "scan_completed_no_detection")

    def test_captured_policy_change_requires_review_without_a_config_event(self):
        for key, value in (("MAPSReporting", 0), ("LowThreatDefaultAction", 2)):
            evidence = fixture()
            evidence["after"]["preferences"]["data"][key] = value
            report = security.evaluate(evidence)
            self.assertEqual(report["status"], "incomplete")
            self.assertIn(
                "Captured protection policy changed during this window", report["reasons"]
            )

    def test_empty_host_metadata_does_not_establish_complete_evidence(self):
        for value in ({"ok": True}, {"ok": True, "data": {}}, {"ok": True, "data": None}):
            evidence = fixture()
            evidence["after"]["host"] = value
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
            reports = json.loads((root / "matrix/matrix.json").read_text(encoding="utf-8"))
            self.assertEqual(reports[0]["status"], "incomplete")
            with (root / "matrix/matrix.csv").open(encoding="utf-8-sig", newline="") as stream:
                self.assertEqual(list(csv.DictReader(stream))[0]["expected_sha256"], DIGEST)
            with self.assertRaises(FileExistsError):
                security.summarize([source], root / "matrix")

    def test_matrix_csv_keeps_prefixed_cells_literal_and_json_unchanged(self):
        escaped = [
            "=1+1",
            "+1+1",
            "-1+1",
            "@SUM(1,1)",
            " =1+1",
            "\t=1+1",
            "\r=1+1",
            "\n=1+1",
            "\r\n=1+1",
            "\v=1+1",
            "\f=1+1",
            "\x00=1+1",
            "\x1f=1+1",
            "\x7f=1+1",
            "\x85=1+1",
            "\u00a0=1+1",
            "\ufeff=1+1",
            "\u200b=1+1",
            " leading label",
            "\t",
        ]
        ordinary = ["D-X64-PAYLOAD", "", "'=1+1", 'label,"quoted"\r\n=1+1']
        values = escaped + ordinary
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            sources, original_bytes, expected_reports = [], [], []
            for index, value in enumerate(values):
                source = root / f"evidence-{index}.json"
                evidence = fixture()
                evidence["request"]["case_id"] = value
                for phase in ("before", "after"):
                    evidence[phase]["host"]["data"].update(build=0, system_type=value)
                security.write_json(source, evidence)
                sources.append(source)
                original_bytes.append(source.read_bytes())
                expected_reports.append(
                    dict(security.evaluate(evidence), evidence_file=str(source))
                )

            output = root / "matrix"
            self.assertEqual(security.summarize(sources, output), 0)
            self.assertEqual(
                json.loads((output / "matrix.json").read_text(encoding="utf-8")), expected_reports
            )
            self.assertEqual([source.read_bytes() for source in sources], original_bytes)
            with (output / "matrix.csv").open(encoding="utf-8-sig", newline="") as stream:
                reader = csv.DictReader(stream)
                rows = list(reader)
                fields = set(reader.fieldnames or [])
            self.assertEqual(len(rows), len(values))
            for index, (value, row) in enumerate(zip(values, rows)):
                with self.subTest(value=value):
                    expected = "'" + value if index < len(escaped) else value
                    self.assertEqual(set(row), fields)
                    self.assertNotIn(None, row.values())
                    self.assertEqual(row["case_id"], expected)
                    self.assertEqual(row["host_type"], expected)
                    self.assertEqual(row["os_build"], "0")
                    self.assertEqual(row["expected_sha256"], DIGEST)
                    self.assertEqual(row["status"], "scan_completed_no_detection")

    def test_probe_timeout_and_unavailable_powershell_preserve_failure(self):
        with patch.object(
            security.subprocess, "run", side_effect=security.subprocess.TimeoutExpired("scan", 1)
        ):
            self.assertEqual(security.run_probe("scan", TARGET, START, 1)["state"], "timed_out")
        with patch.object(
            security.subprocess, "run", side_effect=FileNotFoundError("powershell.exe")
        ):
            self.assertEqual(security.run_probe("snapshot", TARGET, START, 1)["ok"], False)

    @unittest.skipUnless(
        shutil.which("powershell.exe") or shutil.which("pwsh"), "PowerShell required"
    )
    def test_probe_preserves_action_policy_and_rejects_unsupported_properties(self):
        powershell = shutil.which("powershell.exe") or shutil.which("pwsh")
        prefs = fixture()["before"]["preferences"]["data"] | {
            "SubmitSamplesConsent": 1,
            "PUAProtection": 1,
            "DisableBlockAtFirstSeen": False,
            "CloudBlockLevel": 0,
            "LowThreatDefaultAction": 6,
            "ThreatIDDefaultAction_Ids": [123, 456],
            "ThreatIDDefaultAction_Actions": [2, 6],
        }
        for missing in (None, "LowThreatDefaultAction", "ThreatIDDefaultAction_Ids"):
            with self.subTest(missing=missing):
                values = dict(prefs)
                if missing:
                    del values[missing]
                # All OS queries are mocked in a separate PowerShell process. No
                # real scan, file access, protection read or policy change occurs.
                command = "\n".join(
                    [
                        "$fakePreferences = ConvertFrom-Json '"
                        + json.dumps(values).replace("'", "''")
                        + "'",
                        "function Get-MpPreference { $fakePreferences }",
                        "function Get-MpComputerStatus { throw 'mock unavailable' }",
                        "function Get-MpThreatDetection { throw 'mock unavailable' }",
                        "function Get-CimInstance { throw 'mock unavailable' }",
                        "function Get-AuthenticodeSignature { throw 'mock unavailable' }",
                        "function Get-Content { throw 'mock unavailable' }",
                        "& '"
                        + str(security.PROBE).replace("'", "''")
                        + "' -Mode snapshot -Target 'C:\\mock.txt'",
                    ]
                )
                encoded = base64.b64encode(command.encode("utf-16-le")).decode("ascii")
                result = subprocess.run(
                    [
                        powershell,
                        "-NoLogo",
                        "-NoProfile",
                        "-NonInteractive",
                        "-EncodedCommand",
                        encoded,
                    ],
                    capture_output=True,
                    timeout=30,
                    check=True,
                )
                actual = json.loads(result.stdout.decode("utf-8-sig"))["preferences"]
                if missing:
                    self.assertFalse(actual["ok"])
                    self.assertIn(missing, actual["error"])
                else:
                    self.assertTrue(actual["ok"])
                    self.assertEqual(actual["data"], prefs)

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
                raw = json.loads((args.output / "evidence.json").read_text(encoding="utf-8"))
                report = json.loads((args.output / "report.json").read_text(encoding="utf-8"))
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
