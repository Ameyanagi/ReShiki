"""Offline protocol, disposition and CI regressions for the current OSV scan."""

import contextlib
import copy
import io
import json
import shutil
import sys
import tempfile
import unittest
import urllib.error
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import audit_locked_dependencies as audit

FIXTURE = json.loads((ROOT / "tests/fixtures/dependency-advisory-responses.json").read_text())
POLICY = json.loads(audit.POLICY.read_text())


class Reply(io.BytesIO):
    status = 200

    def __init__(self, url, body):
        super().__init__(body)
        self.url = url
        self.headers = {"Content-Length": str(len(body))}

    def geturl(self):
        return self.url


class FakeOSV:
    def __init__(self, new_id=False):
        self.entries = copy.deepcopy(POLICY["exceptions"])
        self.advisories = copy.deepcopy(FIXTURE["advisories"])
        if new_id:
            old = copy.deepcopy(self.entries[0])
            old["id"] = "RUSTSEC-2099-9999"
            self.entries.append(old)
            metadata = copy.deepcopy(self.advisories[POLICY["exceptions"][0]["id"]])
            metadata["id"] = old["id"]
            metadata["aliases"] = [POLICY["exceptions"][0]["id"]]
            self.advisories[old["id"]] = metadata
        self.calls = 0

    def __call__(self, request, *, timeout, context):
        if not context or not 0 < timeout <= 20:
            raise AssertionError("Missing TLS context or timeout bound")
        self.calls += 1
        url = request.full_url
        if url.endswith("/querybatch"):
            results = []
            for query in json.loads(request.data)["queries"]:
                found = [
                    {"id": entry["id"], "modified": self.advisories[entry["id"]]["modified"]}
                    for entry in self.entries
                    if audit.identity(entry) == audit.identity(query)
                ]
                results.append({"vulns": found} if found else {})
            body = json.dumps({"results": results}).encode()
        elif "/vulns/" in url:
            body = json.dumps(self.advisories[url.rsplit("/", 1)[-1]]).encode()
        else:
            identifier = url.rsplit("/", 1)[-1].removesuffix(".md")
            body = FIXTURE["rustsec"][identifier].encode()
        return Reply(url, body)


def project_copy(destination):
    for name in audit.LOCKS:
        shutil.copyfile(ROOT / name, destination / name)
    (destination / "website").mkdir()
    shutil.copyfile(ROOT / "website/astro.config.mjs", destination / "website/astro.config.mjs")


class LockedDependencyAuditTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.records, _, _ = audit.inventory(ROOT)

    def classify(self, entry, *, metadata=None, scope_errors=(), records=None, raw=None):
        return audit.classify(
            {"package": entry["package"], "version": entry["version"]},
            metadata or FIXTURE["advisories"][entry["id"]],
            self.records if records is None else records,
            POLICY,
            scope_errors,
            raw if raw is not None else FIXTURE["rustsec"].get(entry["id"], "").encode(),
        )

    def test_current_eight_matches_stay_visible_with_exact_dispositions(self):
        with (
            tempfile.TemporaryDirectory() as temporary,
            patch.object(audit.urllib.request, "urlopen", side_effect=FakeOSV()),
        ):
            prefix = Path(temporary) / "current"
            report = audit.audit(ROOT, prefix)
            self.assertEqual(report["raw_advisory_matches"], 8)
            self.assertEqual(
                len(json.loads(Path(str(prefix) + "-raw-matches.json").read_text())), 8
            )
            self.assertEqual(report["scope_errors"], [])
            dispositions = [entry["disposition"] for entry in report["findings"]]
            self.assertEqual(dispositions.count("reviewed-maintenance-notice"), 6)
            self.assertEqual(dispositions.count("reviewed-scoped-exposure"), 2)
            for name in audit.LOCKS:
                self.assertEqual(
                    Path(str(prefix) + "-" + name).read_bytes(), (ROOT / name).read_bytes()
                )
            self.assertTrue(Path(str(prefix) + "-batch-00-response-1.raw").is_file())

    def test_new_canonical_id_is_blocking_even_with_known_alias(self):
        with (
            tempfile.TemporaryDirectory() as temporary,
            patch.object(audit.urllib.request, "urlopen", side_effect=FakeOSV(new_id=True)),
        ):
            report = audit.audit(ROOT, Path(temporary) / "new")
            self.assertEqual(report["raw_advisory_matches"], 9)
            blocked = [entry for entry in report["findings"] if entry["disposition"] == "blocking"]
            self.assertEqual([entry["id"] for entry in blocked], ["RUSTSEC-2099-9999"])

    def test_known_dispositions_reject_changed_alias_version_revision_and_source(self):
        for entry in POLICY["exceptions"]:
            metadata = copy.deepcopy(FIXTURE["advisories"][entry["id"]])
            metadata["aliases"] = [*metadata.get("aliases", []), "CVE-2099-9999"]
            self.assertEqual(self.classify(entry, metadata=metadata)["disposition"], "blocking")
            metadata = copy.deepcopy(FIXTURE["advisories"][entry["id"]])
            metadata["modified"] = "2099-01-01T00:00:00Z"
            self.assertEqual(self.classify(entry, metadata=metadata)["disposition"], "blocking")
            changed = {**entry, "version": "999.0.0"}
            self.assertEqual(self.classify(changed, metadata=metadata)["disposition"], "blocking")
            records = copy.deepcopy(self.records)
            for record in records:
                if audit.identity(record) == audit.identity(entry):
                    record.update({key: "changed" for key in entry["locked_source"]})
            self.assertEqual(self.classify(entry, records=records)["disposition"], "blocking")

    def test_maintenance_requires_current_rustsec_and_osv_unmaintained_metadata(self):
        for entry in POLICY["exceptions"]:
            if entry["kind"] != "maintenance":
                continue
            metadata = copy.deepcopy(FIXTURE["advisories"][entry["id"]])
            metadata["affected"][0]["database_specific"]["informational"] = "unsound"
            self.assertEqual(self.classify(entry, metadata=metadata)["disposition"], "blocking")
            raw = (
                FIXTURE["rustsec"][entry["id"]]
                .replace('informational = "unmaintained"', 'informational = "unsound"')
                .encode()
            )
            self.assertEqual(self.classify(entry, raw=raw)["disposition"], "blocking")
            with self.assertRaisesRegex(ValueError, "RustSec metadata"):
                self.classify(entry, raw=b"not a RustSec record")

    def test_real_scope_guard_mutation_blocks_lru_and_http_cache_dispositions(self):
        with tempfile.TemporaryDirectory() as temporary:
            project = Path(temporary)
            project_copy(project)
            config = project / "website/astro.config.mjs"
            config.write_text(config.read_text().replace('output: "static"', 'output: "server"'))
            errors = audit.check(project)
            self.assertTrue(errors)
            for entry in POLICY["exceptions"]:
                if entry["kind"] == "dated-scope":
                    self.assertEqual(
                        self.classify(entry, scope_errors=errors)["disposition"], "blocking"
                    )

    def test_partial_paginated_malformed_and_error_batch_responses_fail_closed(self):
        for response in [
            {},
            {"results": []},
            {"results": [None]},
            {"results": [{"error": "unavailable"}]},
            {"results": [{"next_page_token": "more"}]},
            {"results": [{"vulns": {}}]},
            {"results": [{"vulns": [{"id": "../../outside", "modified": "2026-01-01T00:00:00Z"}]}]},
        ]:
            with self.subTest(response=response), self.assertRaises(ValueError):
                audit.batch_results(response, 1)

    def test_partial_full_advisory_never_reuses_a_known_disposition(self):
        entry = POLICY["exceptions"][0]
        record = copy.deepcopy(FIXTURE["advisories"][entry["id"]])
        record["affected"] = [{"package": entry["package"]}]
        with self.assertRaisesRegex(ValueError, "Incomplete affected-version"):
            audit.full_advisory(record, entry["id"], entry["modified"])
        record = copy.deepcopy(FIXTURE["advisories"][entry["id"]])
        record["aliases"] = None
        with self.assertRaisesRegex(ValueError, "Malformed advisory aliases"):
            audit.full_advisory(record, entry["id"], entry["modified"])

    def test_api_errors_retry_only_transient_failures_and_preserve_receipts(self):
        with tempfile.TemporaryDirectory() as temporary:
            client = audit.Client(Path(temporary) / "network")
            with (
                patch.object(
                    audit.urllib.request, "urlopen", side_effect=urllib.error.URLError("offline")
                ) as opened,
                patch.object(audit.time, "sleep"),
                self.assertRaises(urllib.error.URLError),
            ):
                client.json("failed", audit.OSV + "/querybatch", {"queries": []})
            self.assertEqual(opened.call_count, 3)
            self.assertTrue(client.path("failed-error-3.json").is_file())
            for status, expected in [(400, 1), (429, 3), (503, 3)]:
                with (
                    patch.object(
                        audit.urllib.request,
                        "urlopen",
                        side_effect=urllib.error.HTTPError(
                            audit.OSV, status, "failed", {}, io.BytesIO(b"api error")
                        ),
                    ) as opened,
                    patch.object(audit.time, "sleep"),
                    self.assertRaises(urllib.error.HTTPError),
                ):
                    client.json("http", audit.OSV + "/querybatch")
                self.assertEqual(opened.call_count, expected)

    def test_transport_partial_body_and_malformed_json_do_not_become_empty_findings(self):
        with tempfile.TemporaryDirectory() as temporary:
            client = audit.Client(Path(temporary) / "malformed")
            response = Reply(audit.OSV, b"{}")
            response.headers["Content-Length"] = "99"
            with (
                patch.object(audit.urllib.request, "urlopen", return_value=response),
                self.assertRaisesRegex(ValueError, "Partial"),
            ):
                client.json("partial", audit.OSV)
            with (
                patch.object(
                    audit.urllib.request, "urlopen", return_value=Reply(audit.OSV, b"not JSON")
                ),
                self.assertRaises(json.JSONDecodeError),
            ):
                client.json("invalid", audit.OSV)
            self.assertTrue(client.path("invalid-response-1.raw").is_file())

    def test_failure_exit_writes_a_report_and_new_advisory_exit_is_nonzero(self):
        with tempfile.TemporaryDirectory() as temporary:
            prefix = Path(temporary) / "failure"
            with (
                patch.object(
                    sys, "argv", ["audit", "--project", str(ROOT), "--output-prefix", str(prefix)]
                ),
                patch.object(audit, "audit", side_effect=ValueError("partial API response")),
            ):
                with contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(audit.main(), 1)
            self.assertTrue(Path(str(prefix) + "-failure.json").is_file())
            with (
                patch.object(
                    sys, "argv", ["audit", "--project", str(ROOT), "--output-prefix", str(prefix)]
                ),
                patch.object(audit.urllib.request, "urlopen", side_effect=FakeOSV(new_id=True)),
            ):
                with contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(audit.main(), 1)

    def test_ci_has_one_fresh_scan_and_dependency_for_no_project_download_tests(self):
        text = (ROOT / ".github/workflows/checks.yml").read_text()
        self.assertEqual(text.count("python scripts/audit_locked_dependencies.py"), 1)
        block = text.split("  dependency-advisories:\n", 1)[1].split("\n  web:", 1)[0]
        self.assertNotIn("matrix:", block)
        self.assertLess(block.index("- *actions_runtime"), block.index("uv sync --locked"))
        self.assertLess(
            block.index("uv sync --locked"), block.index("audit_locked_dependencies.py")
        )
        self.assertIn("steps.actions_runtime_guard.outcome == 'success'", block)
        self.assertIn("hashFiles('artifacts/dependency-advisories/*') != ''", block)
        for workflow in (ROOT / ".github/workflows").glob("*.yml"):
            for line in workflow.read_text().splitlines():
                if "uv run --no-project" in line and "tests.test_inchi_distribution" in line:
                    self.assertIn("--with certifi==2026.10.23", line)


if __name__ == "__main__":
    unittest.main()
