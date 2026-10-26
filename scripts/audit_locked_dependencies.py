"""Query current OSV records for the three locked graphs; retain every raw match.

Only exact, dated dispositions in dependency_advisory_policy.json are permitted.
This does not scan Actions bundles, embedded native libraries or the Nix closure.
"""

import argparse
import hashlib
import http.client
import json
import re
import shutil
import time
import tomllib
import urllib.error
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

from check_dependency_security_scope import bun_json, check
from download_tls import verified_context

POLICY = Path(__file__).with_name("dependency_advisory_policy.json")
LOCKS = ("Cargo.lock", "bun.lock", "uv.lock", "flake.lock")
OSV = "https://api.osv.dev/v1"
MAX_BYTES = 8 * 1024 * 1024
ATTEMPTS = 3
DEADLINE_SECONDS = 240
ID = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,127}")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def identity(query):
    return query["package"]["ecosystem"], query["package"]["name"], query["version"]


def inventory(project):
    records, workspaces = [], []
    for package in tomllib.loads((project / "Cargo.lock").read_text())["package"]:
        records.append(
            {
                "package": {"ecosystem": "crates.io", "name": package["name"]},
                "version": package["version"],
                "lockfile": "Cargo.lock",
                "source": package.get("source", "workspace-or-path"),
                "checksum": package.get("checksum"),
            }
        )
    bun = bun_json((project / "bun.lock").read_text())
    for key, entry in bun["packages"].items():
        require(
            isinstance(entry, list) and entry and isinstance(entry[0], str), "Invalid Bun entry"
        )
        name, separator, version = entry[0].rpartition("@")
        require(separator and name, f"Unversioned Bun entry: {key}")
        if version.startswith("workspace:"):
            relative = version.removeprefix("workspace:")
            require(
                bun["workspaces"].get(relative, {}).get("name") == name == key,
                f"Unverified Bun workspace alias: {key}",
            )
            workspaces.append({"key": key, "resolved": entry[0]})
            continue
        require(
            re.fullmatch(r"\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?", version),
            f"Unsupported Bun version: {key}",
        )
        records.append(
            {
                "package": {"ecosystem": "npm", "name": name},
                "version": version,
                "lockfile": "bun.lock",
                "lock_key": key,
                "integrity": entry[-1],
            }
        )
    for package in tomllib.loads((project / "uv.lock").read_text())["package"]:
        records.append(
            {
                "package": {"ecosystem": "PyPI", "name": package["name"]},
                "version": package["version"],
                "lockfile": "uv.lock",
                "source": package.get("source"),
            }
        )
    require(records, "Empty locked inventory")
    unique = {}
    for record in records:
        require(
            all(isinstance(value, str) and value for value in identity(record)),
            "Invalid package identity",
        )
        unique[identity(record)] = {"package": record["package"], "version": record["version"]}
    return records, workspaces, list(unique.values())


class Client:
    def __init__(self, prefix):
        self.prefix = prefix
        self.deadline = time.monotonic() + DEADLINE_SECONDS

    def path(self, suffix):
        return Path(str(self.prefix) + "-" + suffix)

    def fetch(self, label, url, payload=None):
        write_json(self.path(label + "-request.json"), {"url": url, "body": payload})
        data = json.dumps(payload).encode() if payload is not None else None
        request = urllib.request.Request(
            url,
            data=data,
            headers={"Content-Type": "application/json", "User-Agent": "ReShiki-locked-audit"},
        )
        for attempt in range(1, ATTEMPTS + 1):
            remaining = self.deadline - time.monotonic()
            require(remaining > 0, "Advisory scan exceeded its bounded deadline")
            try:
                with urllib.request.urlopen(
                    request, timeout=min(20, remaining), context=verified_context()
                ) as response:
                    body = response.read(MAX_BYTES + 1)
                    self.path(f"{label}-response-{attempt}.raw").write_bytes(body[:MAX_BYTES])
                    require(response.status == 200, f"Unexpected HTTP status: {response.status}")
                    require(response.geturl() == url, "Unexpected advisory endpoint redirect")
                    require(len(body) <= MAX_BYTES, "Advisory response exceeds byte limit")
                    length = response.headers.get("Content-Length")
                    require(length is None or int(length) == len(body), "Partial advisory response")
                    return body
            except (urllib.error.URLError, OSError, http.client.HTTPException) as error:
                write_json(self.path(f"{label}-error-{attempt}.json"), {"error": str(error)})
                if isinstance(error, urllib.error.HTTPError):
                    self.path(f"{label}-response-{attempt}.raw").write_bytes(error.read(MAX_BYTES))
                    if error.code not in {429, 500, 502, 503, 504}:
                        raise
                if attempt == ATTEMPTS:
                    raise
                time.sleep(min(attempt, max(0, self.deadline - time.monotonic())))
        raise ValueError("Advisory request did not complete")

    def json(self, label, url, payload=None):
        result = json.loads(self.fetch(label, url, payload))
        require(isinstance(result, dict), "Advisory response must be an object")
        return result


def batch_results(response, count):
    require(set(response) == {"results"}, "Malformed OSV batch envelope")
    answers = response["results"]
    require(isinstance(answers, list) and len(answers) == count, "Partial OSV batch response")
    for answer in answers:
        require(
            isinstance(answer, dict) and set(answer) <= {"vulns", "next_page_token"},
            "Malformed OSV package response",
        )
        require(not answer.get("next_page_token"), "Incomplete paginated OSV response")
        vulns = answer.get("vulns", [])
        require(isinstance(vulns, list), "Malformed OSV advisory list")
        seen = set()
        for vuln in vulns:
            require(isinstance(vuln, dict), "Malformed OSV advisory")
            identifier = vuln.get("id")
            require(
                isinstance(identifier, str) and ID.fullmatch(identifier), "Invalid OSV advisory ID"
            )
            require(identifier not in seen, "Duplicate OSV advisory ID")
            require(isinstance(vuln.get("modified"), str), "Missing OSV advisory revision")
            datetime.fromisoformat(vuln["modified"].replace("Z", "+00:00"))
            seen.add(identifier)
    return answers


def full_advisory(advisory, identifier, revision):
    require(advisory.get("id") == identifier, "Full OSV advisory identity mismatch")
    require(isinstance(advisory.get("modified"), str), "Missing full OSV revision")
    require(
        datetime.fromisoformat(advisory["modified"].replace("Z", "+00:00"))
        == datetime.fromisoformat(revision.replace("Z", "+00:00")),
        "OSV advisory changed during scan; retry a fresh scan",
    )
    affected = advisory.get("affected")
    require(isinstance(affected, list) and affected, "Missing full OSV affected-package metadata")
    for entry in affected:
        require(isinstance(entry, dict), "Malformed full OSV affected entry")
        package = entry.get("package")
        require(
            isinstance(package, dict)
            and all(
                isinstance(package.get(key), str) and package[key] for key in ["ecosystem", "name"]
            ),
            "Incomplete affected package identity",
        )
        ranges, versions = entry.get("ranges", []), entry.get("versions", [])
        require(
            isinstance(ranges, list) and isinstance(versions, list) and (ranges or versions),
            "Incomplete affected-version metadata",
        )
        require(
            all(isinstance(version, str) and version for version in versions),
            "Malformed affected versions",
        )
        for interval in ranges:
            require(
                isinstance(interval, dict)
                and interval.get("type") in {"SEMVER", "ECOSYSTEM", "GIT"},
                "Malformed affected range",
            )
            events = interval.get("events")
            require(isinstance(events, list) and events, "Incomplete affected range events")
            for event in events:
                require(
                    isinstance(event, dict)
                    and len(event) == 1
                    and set(event) <= {"introduced", "fixed", "last_affected", "limit"}
                    and all(isinstance(value, str) and value for value in event.values()),
                    "Malformed affected range event",
                )
    aliases = advisory.get("aliases", [])
    require(
        isinstance(aliases, list)
        and all(isinstance(alias, str) and ID.fullmatch(alias) for alias in aliases)
        and len(aliases) == len(set(aliases)),
        "Malformed advisory aliases",
    )


def classify(query, advisory, records, policy, scope_errors, rustsec=None):
    identifier = advisory["id"]
    decision = {
        "query": query,
        "id": identifier,
        "disposition": "blocking",
        "reason": "New or changed advisory requires review",
    }
    match = next(
        (
            entry
            for entry in policy["exceptions"]
            if entry["id"] == identifier and identity(entry) == identity(query)
        ),
        None,
    )
    if match is None:
        return decision
    aliases = advisory.get("aliases", [])
    if not isinstance(aliases, list) or sorted(aliases) != sorted(match["aliases"]):
        decision["reason"] = "Advisory aliases changed; no alias-wide exception is permitted"
        return decision
    if advisory.get("modified") != match["modified"] or advisory.get("withdrawn"):
        decision["reason"] = "Advisory revision changed; reassess the dated disposition"
        return decision
    locked = [record for record in records if identity(record) == identity(query)]
    if not locked or any(
        any(record.get(key) != value for key, value in match["locked_source"].items())
        for record in locked
    ):
        decision["reason"] = "Reviewed locked source changed"
        return decision
    if scope_errors:
        decision["reason"] = "Dated dependency scope guard failed"
        return decision
    affected = [
        entry
        for entry in advisory.get("affected", [])
        if all(entry.get("package", {}).get(k) == v for k, v in query["package"].items())
    ]
    if not affected:
        decision["reason"] = "Full advisory metadata does not identify the queried package"
        return decision
    if match["kind"] == "maintenance":
        # OSV's affected package may also include a purl, so compare the two
        # identity fields without discarding any advisory from the raw report.
        if (
            not affected
            or any(
                entry.get("database_specific", {}).get("informational") != "unmaintained"
                or entry.get("database_specific", {}).get("categories")
                or entry.get("database_specific", {}).get("cvss")
                for entry in affected
            )
            or advisory.get("severity")
        ):
            decision["reason"] = "The advisory is no longer a maintenance-only notice"
            return decision
        if rustsec is None:
            raise ValueError("Missing fresh RustSec maintenance metadata")
        metadata = re.match(r"\A```toml\n(.*?)\n```", rustsec.decode(), re.DOTALL)
        if metadata is None:
            raise ValueError("Malformed RustSec metadata")
        source = tomllib.loads(metadata[1])["advisory"]
        if (
            (
                source.get("id"),
                source.get("package"),
                source.get("informational"),
                source.get("url"),
            )
            != (identifier, query["package"]["name"], "unmaintained", match["assessment_url"])
            or source.get("cvss")
            or source.get("categories")
            or hashlib.sha256(rustsec).hexdigest() != match["rustsec_sha256"]
        ):
            decision["reason"] = "Current RustSec metadata/source assessment changed"
            return decision
        decision["disposition"] = "reviewed-maintenance-notice"
    else:
        require(match["kind"] == "dated-scope", "Unsupported advisory disposition")
        decision["disposition"] = "reviewed-scoped-exposure"
    decision["reason"] = match["reason"]
    return decision


def audit(project, prefix, policy_path=POLICY):
    project, prefix = project.resolve(), prefix.resolve()
    prefix.parent.mkdir(parents=True, exist_ok=True)
    client = Client(prefix)
    for name in LOCKS:
        shutil.copyfile(project / name, client.path(name))
    records, workspaces, queries = inventory(project)
    write_json(
        client.path("inventory.json"),
        {
            "time_utc": datetime.now(timezone.utc).isoformat(),
            "records": records,
            "workspace_aliases": workspaces,
            "lock_sha256": {
                name: hashlib.sha256((project / name).read_bytes()).hexdigest() for name in LOCKS
            },
        },
    )
    scope_errors = check(project)
    write_json(client.path("dated-scope.json"), {"errors": scope_errors})
    policy = json.loads(policy_path.read_text())
    require(
        policy["schema"] == 1 and len(policy["exceptions"]) == 8,
        "Unexpected exception policy; review exact dispositions",
    )
    matches = []
    for index, start in enumerate(range(0, len(queries), 200)):
        batch = queries[start : start + 200]
        response = client.json(f"batch-{index:02}", OSV + "/querybatch", {"queries": batch})
        for query, answer in zip(batch, batch_results(response, len(batch)), strict=True):
            matches.extend({"query": query, "result": item} for item in answer.get("vulns", []))
    write_json(client.path("raw-matches.json"), matches)
    decisions = []
    for item in matches:
        identifier = item["result"]["id"]
        advisory = client.json(identifier, OSV + "/vulns/" + identifier)
        full_advisory(advisory, identifier, item["result"]["modified"])
        exception = next(
            (
                entry
                for entry in policy["exceptions"]
                if entry["id"] == identifier and identity(entry) == identity(item["query"])
            ),
            None,
        )
        raw = None
        if exception and exception["kind"] == "maintenance":
            url = f"https://raw.githubusercontent.com/RustSec/advisory-db/main/crates/{item['query']['package']['name']}/{identifier}.md"
            raw = client.fetch(identifier + "-rustsec", url)
        decisions.append(classify(item["query"], advisory, records, policy, scope_errors, raw))
    result = {
        "time_utc": datetime.now(timezone.utc).isoformat(),
        "queried_unique_versions": len(queries),
        "raw_advisory_matches": len(matches),
        "scope_errors": scope_errors,
        "findings": decisions,
        "limits": [
            "Only locked Cargo/npm/PyPI name/version queries; not Actions bundles, embedded native libraries, OS packages or Nix closure.",
            "Workspace/path/git packages use declared names and versions; fork differences need separate assessment.",
            "Reviewed maintenance and scoped matches remain visible and are not repaired-version claims.",
            "Socket timeouts, capped retries and a between-request budget are bounded; the CI job timeout is the final wall-clock limit.",
        ],
    }
    write_json(client.path("summary.json"), result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project", type=Path, required=True)
    parser.add_argument("--output-prefix", type=Path, required=True)
    args = parser.parse_args()
    try:
        result = audit(args.project, args.output_prefix)
    except Exception as error:
        args.output_prefix.parent.mkdir(parents=True, exist_ok=True)
        write_json(
            Path(str(args.output_prefix) + "-failure.json"),
            {"error": f"{type(error).__name__}: {error}"},
        )
        print(f"Advisory scan failed closed: {error}")
        return 1
    blocking = len(result["scope_errors"]) + sum(
        row["disposition"] == "blocking" for row in result["findings"]
    )
    print(
        json.dumps(
            {
                "raw_advisory_matches": result["raw_advisory_matches"],
                "blocking_findings": blocking,
                "findings": result["findings"],
            },
            indent=2,
        )
    )
    return int(blocking != 0)


if __name__ == "__main__":
    raise SystemExit(main())
