"""Collect exact-file Defender evidence on Windows; evaluate/matrix it on any OS.

No binary execution, sample upload, definition update or security-policy change.
Defender's existing remediation/cloud policy still applies when --scan is used.
"""

import argparse
import csv
import hashlib
import json
import ntpath
import re
import subprocess
import sys
import time
import unicodedata
import xml.etree.ElementTree as ET
from datetime import datetime, timezone
from pathlib import Path

PROBE = Path(__file__).with_name("windows_security_probe.ps1")
PROVIDER = "Microsoft-Windows-Windows Defender"
DETECTION_EVENTS = {1006, 1007, 1008, 1015, 1116, 1117, 1118, 1119}
PROTECTION_REVIEW_EVENTS = {3002, 3007, 5001, 5004, 5007, 5008, 5010, 5012, 5101}
HISTORY_REVIEW_EVENTS = {1013, 1014}
DEFAULT_ACTION_FIELDS = (
    "UnknownThreatDefaultAction",
    "LowThreatDefaultAction",
    "ModerateThreatDefaultAction",
    "HighThreatDefaultAction",
    "SevereThreatDefaultAction",
)
# Default (security intelligence), Clean, Quarantine, Remove, Block. Allow (6)
# suppresses detection events; user-defined/no-action/unknown policies need review.
REMEDIATING_ACTIONS = {0, 1, 2, 3, 10}
ACTIVE_FIELDS = (
    "AMServiceEnabled",
    "AntivirusEnabled",
    "RealTimeProtectionEnabled",
    "BehaviorMonitorEnabled",
    "OnAccessProtectionEnabled",
    "IoavProtectionEnabled",
)
VERSION_FIELDS = ("AMProductVersion", "AMEngineVersion", "AntivirusSignatureVersion")


def utc_now():
    return datetime.now(timezone.utc).isoformat()


def timestamp(value):
    parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.tzinfo is None:
        raise ValueError("UTC timestamps must include a timezone")
    return parsed


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def fingerprint(path):
    try:
        with path.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
            stream.seek(0)
            header = stream.read(64)
            machine = None
            if len(header) == 64 and header[:2] == b"MZ":
                stream.seek(int.from_bytes(header[60:64], "little"))
                pe = stream.read(6)
                if len(pe) == 6 and pe[:4] == b"PE\0\0":
                    machine = f"0x{int.from_bytes(pe[4:6], 'little'):04x}"
        return {"ok": True, "sha256": digest, "size": path.stat().st_size, "pe_machine": machine}
    except OSError as error:
        return {"ok": False, "error": str(error)}


def data(snapshot, name):
    section = snapshot.get(name, {})
    return section.get("data") if section.get("ok") is True else None


def objects(value):
    return value if isinstance(value, list) else ([] if value is None else [value])


def normalized(path):
    return ntpath.normcase(ntpath.normpath(path.strip()))


def scan_resource(path):
    # Native Windows event 1000 uses file:_ even for an exact-file custom scan.
    path = path.strip()
    return normalized(path[6:] if path.casefold().startswith("file:_") else path)


def resource_matches(value, target):
    # Defender resources can include file:_ / containerfile:_ prefixes and members.
    # A same-prefix sibling (app.exe.old) must not match app.exe.
    value = str(value).replace("/", "\\").casefold()
    pattern = r"(?:^|[;|]|file:_|containerfile:_|webfile:_|\s)" + re.escape(normalized(target))
    return re.search(pattern + r"(?=$|[;|\r\n]|->)", value) is not None


def parse_event(xml):
    ns = {"e": "http://schemas.microsoft.com/win/2004/08/events/event"}
    root = ET.fromstring(xml)
    system = root.find("e:System", ns)
    if system is None:
        raise ValueError("Event system metadata missing")
    provider = system.find("e:Provider", ns)
    created = system.find("e:TimeCreated", ns)
    event_id = system.findtext("e:EventID", namespaces=ns)
    if provider is None or provider.get("Name") != PROVIDER:
        raise ValueError("Unexpected event provider")
    if created is None or not created.get("SystemTime") or not event_id:
        raise ValueError("Event ID/time missing")
    return {
        "id": int(event_id),
        "utc": timestamp(created.get("SystemTime")),
        "fields": {
            item.get("Name"): item.text or "" for item in root.findall("e:EventData/e:Data", ns)
        },
    }


def events_in(evidence):
    section = evidence.get("events", {})
    if section.get("ok") is not True or section.get("data", {}).get("truncated") is not False:
        raise ValueError("Operational event log unavailable or truncated")
    start = timestamp(evidence["request"]["since_utc"])
    end = timestamp(evidence["finished_utc"])
    events = [parse_event(xml) for xml in section["data"]["records"]]
    if any(not start <= event["utc"] <= end for event in events):
        raise ValueError("Operational event outside the recorded observation window")
    return events


def completed_scan(evidence, events):
    scan = evidence.get("scan", {})
    if not scan.get("started_utc"):
        return None
    start = timestamp(scan["started_utc"])
    starts = [
        event
        for event in events
        if event["id"] == 1000
        and event["utc"] >= start
        and scan_resource(event["fields"].get("Scan Resources", ""))
        == normalized(evidence["request"]["target"])
    ]
    if len(starts) != 1:
        return None
    first = starts[0]
    scan_id = first["fields"].get("Scan ID")
    related = [event for event in events if scan_id and event["fields"].get("Scan ID") == scan_id]
    if any(event["id"] in {1002, 1005} for event in related):
        return None
    if any(event["id"] == 1001 and event["utc"] >= first["utc"] for event in related):
        return scan_id
    return None


def protection_problems(snapshot):
    problems = []
    status = data(snapshot, "status") or {}
    if status.get("AMRunningMode") != "Normal" or any(
        status.get(key) is not True for key in ACTIVE_FIELDS
    ):
        problems.append("Defender normal active protection is not fully established")
    if any(not status.get(key) for key in (*VERSION_FIELDS, "AntivirusSignatureLastUpdated")):
        problems.append("Engine/platform/definition metadata is incomplete")
    if status.get("DefenderSignaturesOutOfDate") is not False:
        problems.append("Current definitions are not established")
    prefs = data(snapshot, "preferences")
    if not isinstance(prefs, dict):
        problems.append("Protection policy could not be read")
    else:
        for key in (
            "DisableRealtimeMonitoring",
            "DisableBehaviorMonitoring",
            "DisableIOAVProtection",
            "DisableArchiveScanning",
        ):
            if prefs.get(key) is not False:
                problems.append(f"{key} is enabled or unknown")
        for key in ("ExclusionPath", "ExclusionExtension", "ExclusionProcess"):
            if key not in prefs or prefs[key]:
                problems.append(f"{key} requires review; no exclusion matching is assumed")
        for key in DEFAULT_ACTION_FIELDS:
            action = prefs.get(key)
            if type(action) is not int or action not in REMEDIATING_ACTIONS:
                problems.append(f"{key} is non-remediating, user-defined or unknown")
        ids_key, actions_key = "ThreatIDDefaultAction_Ids", "ThreatIDDefaultAction_Actions"
        if ids_key not in prefs or actions_key not in prefs:
            problems.append("Threat-specific action policy could not be read")
        else:
            ids, actions = objects(prefs[ids_key]), objects(prefs[actions_key])
            if (
                len(ids) != len(actions)
                or any(type(threat_id) is not int or threat_id <= 0 for threat_id in ids)
                or len(set(map(str, ids))) != len(ids)
            ):
                problems.append("Threat-specific action policy pairs are incomplete or ambiguous")
            if any(
                type(action) is not int or action not in REMEDIATING_ACTIONS for action in actions
            ):
                problems.append(
                    "Threat-specific action is non-remediating, user-defined or unknown"
                )
    return problems


def evaluate(evidence) -> dict:
    if evidence.get("schema_version") != 1:
        raise ValueError("Unsupported security-evidence schema")
    request = evidence["request"]
    expected = request["expected_sha256"]
    if not re.fullmatch(r"[0-9a-f]{64}", expected):
        raise ValueError("Expected SHA256 must be 64 lowercase hexadecimal characters")
    before, after = evidence.get("before", {}), evidence.get("after", {})
    reasons, detected = [], False
    try:
        events = events_in(evidence)
    except (KeyError, ValueError, AttributeError, TypeError, ET.ParseError) as error:
        events = []
        reasons.append(f"Event evidence incomplete: {error}")
    for event in events:
        if event["id"] in DETECTION_EVENTS:
            if any(
                resource_matches(value, request["target"]) for value in event["fields"].values()
            ):
                detected = True
            else:
                reasons.append("An unattributed threat event in this window requires review")
        if event["id"] in PROTECTION_REVIEW_EVENTS:
            reasons.append(
                f"Protection failure, recovery or configuration event {event['id']} requires review"
            )
        if event["id"] in HISTORY_REVIEW_EVENTS:
            reasons.append(f"Threat history deletion event {event['id']} requires review")
    old_threats = {json.dumps(item, sort_keys=True) for item in objects(data(before, "threats"))}
    new_threats = {json.dumps(item, sort_keys=True) for item in objects(data(after, "threats"))}
    if old_threats - new_threats:
        reasons.append("Prior threat records disappeared or changed during this window")
    # A later history purge must not erase target detection evidence already captured.
    for item in objects(data(before, "threats")) + objects(data(after, "threats")):
        if any(
            resource_matches(value, request["target"]) for value in objects(item.get("Resources"))
        ):
            detected = True
        elif json.dumps(item, sort_keys=True) not in old_threats:
            reasons.append("A new/changed unattributed threat record requires review")
    for name, snapshot in (("before", before), ("after", after)):
        reasons.extend(f"{name}: {problem}" for problem in protection_problems(snapshot))
        if snapshot.get("threats", {}).get("ok") is not True:
            reasons.append(f"{name}: threat history unavailable")
        host = data(snapshot, "host")
        if not isinstance(host, dict) or not host:
            reasons.append(f"{name}: Windows host metadata unavailable")
        file = evidence.get(f"file_{name}", {})
        if file.get("ok") is not True or file.get("sha256") != expected:
            reasons.append(
                f"{name}: exact expected file is missing, unreadable or has a different hash"
            )
    versions = [
        {key: (data(snapshot, "status") or {}).get(key) for key in VERSION_FIELDS}
        for snapshot in (before, after)
    ]
    if versions[0] != versions[1]:
        reasons.append("Engine/platform/definitions changed; rerun to qualify one version")
    if data(before, "preferences") != data(after, "preferences"):
        reasons.append("Captured protection policy changed during this window")
    scan_id = completed_scan(evidence, events)
    if (
        evidence.get("scan", {}).get("state") != "returned"
        or evidence["scan"].get("exit_code") != 0
    ):
        reasons.append("Requested custom scan did not return successfully (or was not requested)")
    if not scan_id:
        reasons.append("No unique exact-target start/completion event pair with matching Scan ID")
    if detected:
        reasons.insert(
            0,
            "Threat evidence references the target path; retain raw records for time/hash attribution",
        )
    return {
        "case_id": request["case_id"],
        "release_tag": request["release_tag"],
        "source_commit": request["source_commit"],
        "kind": request["kind"],
        "phase": request["phase"],
        "target_architecture": request["architecture"],
        "target": request["target"],
        "expected_sha256": expected,
        "container_sha256": request.get("container_sha256"),
        "observed_sha256": evidence.get("file_before", {}).get("sha256"),
        "pe_machine": evidence.get("file_before", {}).get("pe_machine"),
        "status": "detection_recorded"
        if detected
        else ("incomplete" if reasons else "scan_completed_no_detection"),
        "reasons": list(dict.fromkeys(reasons)),
        "scan_id": scan_id,
        "versions_before": versions[0],
        "versions_after": versions[1],
        "host": data(before, "host"),
        "signature": data(after, "signature"),
        "started_utc": request["since_utc"],
        "finished_utc": evidence["finished_utc"],
        "scope": "Exact-file static scan only; browser, install, native execution, other vendors and reputation are not tested.",
    }


def run_probe(mode, target, since, timeout):
    command = [
        "powershell.exe",
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-File",
        str(PROBE),
        "-Mode",
        mode,
        "-Target",
        target,
        "-Since",
        since,
    ]
    try:
        completed = subprocess.run(command, capture_output=True, timeout=timeout, check=False)
        output = completed.stdout.decode("utf-8-sig", errors="replace")
        error = completed.stderr.decode("utf-8-sig", errors="replace")
        if mode == "scan":
            return {
                "state": "returned",
                "exit_code": completed.returncode,
                "stdout": output,
                "stderr": error,
            }
        if completed.returncode:
            return {"ok": False, "error": error, "exit_code": completed.returncode}
        return json.loads(output)
    except subprocess.TimeoutExpired:
        return {
            "ok": False,
            "state": "timed_out",
            "error": "Collector wait expired; Defender's service scan may continue. No scan cancellation or policy change was issued.",
        }
    except (OSError, ValueError) as error:
        return {"ok": False, "state": "error", "error": str(error)}


def collect(args):
    if sys.platform != "win32":
        raise ValueError(
            "Collection requires native Windows with Windows PowerShell and Defender; summarize works on any OS"
        )
    args.output.mkdir(parents=True, exist_ok=False)
    target = str(args.target.resolve())
    started = utc_now()
    since = args.since or started
    if timestamp(since) > timestamp(started):
        raise ValueError("--since cannot be in the future")
    evidence = {
        "schema_version": 1,
        "collector": {
            "python_version": sys.version,
            "script_sha256": fingerprint(Path(__file__)).get("sha256"),
            "probe_sha256": fingerprint(PROBE).get("sha256"),
        },
        "request": {
            "case_id": args.case_id,
            "release_tag": args.release_tag,
            "source_commit": args.source_commit,
            "target": target,
            "expected_sha256": args.sha256.lower(),
            "architecture": args.architecture,
            "kind": args.kind,
            "phase": args.phase,
            "source_url": args.source_url,
            "container_sha256": args.container_sha256,
            "since_utc": since,
        },
        "scan": {"state": "not_requested"},
        "finished_utc": started,
    }

    def checkpoint():
        evidence["finished_utc"] = utc_now()
        write_json(args.output / "evidence.json", evidence)

    checkpoint()
    evidence["before"] = run_probe("snapshot", target, since, 60)
    evidence["file_before"] = fingerprint(Path(target))
    checkpoint()
    if args.scan:
        if evidence["file_before"].get("sha256") != args.sha256.lower() or protection_problems(
            evidence["before"]
        ):
            evidence["scan"] = {
                "state": "skipped",
                "reason": "Hash or protection prerequisites not established",
            }
        else:
            scan_start = utc_now()
            deadline = time.monotonic() + args.timeout
            evidence["scan"] = run_probe("scan", target, since, args.timeout)
            evidence["scan"]["started_utc"] = scan_start
            while True:
                evidence["events"] = run_probe("events", target, since, 60)
                checkpoint()
                if (
                    evidence["events"].get("ok") is not True
                    or evidence["scan"].get("exit_code") != 0
                ):
                    break
                try:
                    if completed_scan(evidence, events_in(evidence)):
                        break
                except (KeyError, ValueError, AttributeError, TypeError, ET.ParseError):
                    break
                if time.monotonic() >= deadline:
                    evidence["scan"]["state"] = "completion_not_observed"
                    break
                time.sleep(2)
    evidence["after"] = run_probe("snapshot", target, since, 60)
    evidence["file_after"] = fingerprint(Path(target))
    evidence["events"] = run_probe("events", target, since, 60)
    checkpoint()
    report = evaluate(evidence)
    write_json(args.output / "report.json", report)
    print(f"{report['case_id']}: {report['status']} ({args.output})")
    return 0 if report["status"] == "scan_completed_no_detection" else 1


def summarize(paths, output):
    reports = [
        dict(evaluate(json.loads(path.read_text(encoding="utf-8-sig"))), evidence_file=str(path))
        for path in paths
    ]
    output.mkdir(parents=True, exist_ok=False)
    write_json(output / "matrix.json", reports)
    fields = [
        "case_id",
        "release_tag",
        "kind",
        "phase",
        "target_architecture",
        "target",
        "expected_sha256",
        "observed_sha256",
        "pe_machine",
        "status",
        "scan_id",
        "os_build",
        "host_type",
        "engine_before",
        "engine_after",
        "definitions_before",
        "definitions_after",
        "evidence_file",
        "reasons",
    ]
    with (output / "matrix.csv").open("w", newline="", encoding="utf-8-sig") as stream:
        writer = csv.DictWriter(stream, fieldnames=fields, extrasaction="ignore")
        writer.writeheader()
        for report in reports:
            host = report.get("host")
            if not isinstance(host, dict):
                host = {}
            row = dict(
                report,
                reasons="; ".join(report["reasons"]),
                os_build=host.get("build"),
                host_type=host.get("system_type"),
            )
            for phase in ("before", "after"):
                versions = report[f"versions_{phase}"]
                if not isinstance(versions, dict):
                    raise ValueError("Missing version record")
                row[f"engine_{phase}"] = versions["AMEngineVersion"]
                row[f"definitions_{phase}"] = versions["AntivirusSignatureVersion"]
            # Importers may skip leading whitespace/control/format characters.
            # Prefix the original value; JSON evidence and evaluation stay exact.
            writer.writerow(
                {
                    key: (
                        "'" + value
                        if isinstance(value, str)
                        and value
                        and (
                            value[0] in "=+-@"
                            or value[0].isspace()
                            or unicodedata.category(value[0]) in {"Cc", "Cf"}
                        )
                        else value
                    )
                    for key, value in row.items()
                }
            )
    return (
        0
        if reports and all(report["status"] == "scan_completed_no_detection" for report in reports)
        else 1
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    scan = commands.add_parser(
        "collect", help="Save one Windows target's native evidence; --scan opts into a custom scan"
    )
    scan.add_argument("target", type=Path)
    for name in ("case-id", "release-tag", "source-commit", "sha256", "phase", "source-url"):
        scan.add_argument(f"--{name}", required=True)
    scan.add_argument(
        "--kind",
        choices=[
            "container",
            "payload",
            "installed_app",
            "uninstaller",
            "setup_component",
            "fixture",
        ],
        required=True,
    )
    scan.add_argument("--architecture", choices=["x64", "arm64"], required=True)
    scan.add_argument("--container-sha256")
    scan.add_argument(
        "--since", help="Timezone-bearing start of browser/download observation; default is now"
    )
    scan.add_argument("--scan", action="store_true")
    scan.add_argument(
        "--timeout", type=int, default=600, help="Scan/event wait budget in seconds, 1–1800"
    )
    scan.add_argument(
        "--output", type=Path, required=True, help="New evidence directory; never overwritten"
    )
    matrix = commands.add_parser(
        "summarize", help="Re-evaluate raw evidence into JSON/CSV on any OS"
    )
    matrix.add_argument("evidence", type=Path, nargs="+")
    matrix.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == "summarize":
            return summarize(args.evidence, args.output)
        if (
            not re.fullmatch(r"[0-9a-fA-F]{64}", args.sha256)
            or args.container_sha256
            and not re.fullmatch(r"[0-9a-fA-F]{64}", args.container_sha256)
            or not 1 <= args.timeout <= 1800
        ):
            raise ValueError("Supply a SHA256 digest and a scan timeout between 1 and 1800 seconds")
        return collect(args)
    except (OSError, ValueError, KeyError) as error:
        parser.exit(2, f"{error}\n")


if __name__ == "__main__":
    sys.exit(main())
