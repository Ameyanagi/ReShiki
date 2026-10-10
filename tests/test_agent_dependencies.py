"""The agent dependency audit's parsers and decisions, on fixture cargo output."""

import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from build_inchi_helper import RELEASE_TARGETS
from check_agent_dependencies import (
    AHO_CORASICK_MIT_CHOICE,
    BASELINE,
    ROOT,
    SERDE_JSON_FORBIDDEN,
    allowlist_problems,
    layout,
    license_allowed,
    selected_mit_license,
    verify,
)

SERVER = "schemars,server,transport-async-rw,uuid"
SERDE_JSON = "alloc,default,float_roundtrip,std"
RMCP_TREE = "rmcp v3.5.1\nserde_derive v1.0.229 (proc-macro)\ntokio v1.53.1\ntokio v1.53.1 (*)\n"
FIXTURE_BASELINE = {
    "source_commit": "40c8d82a5ed213685a7eedd150a0b7adf6899846",
    "packages": ["serde_json@1.0.151"],
    "serde_json_features": {
        target: ["default", "float_roundtrip", "std"] for target in RELEASE_TARGETS
    },
    "rmcp_features": SERVER.split(","),
}


def tree(rmcp, serde_json=SERDE_JSON):
    """`cargo tree -p reshiki --prefix none --format '{p}|{f}'` output."""
    return (
        "reshiki v0.11.0 (/repo)|default\n"
        "reshiki-mcp v0.1.0 (/repo/crates/mcp)|\n"
        f"rmcp v3.5.1|{rmcp}\n"
        "pastey v0.2.3 (proc-macro)|\n"
        f"serde_json v1.0.151|{serde_json}\n"
        f"serde_json v1.0.151|{serde_json} (*)\n"
    )


def cargo(production, *, with_dev=None, rmcp=RMCP_TREE):
    """Answer like cargo: dev-only features appear only when dev edges are requested."""

    def run(root, *command):
        if command[command.index("-p") + 1] == "rmcp":
            return rmcp
        edges = command[command.index("-e") + 1].split(",")
        return with_dev if with_dev and "dev" in edges else production

    return run


def package(name, version, license):
    return {"name": name, "version": version, "license": license}


class AgentDependencyTests(unittest.TestCase):
    def audit(self, run, packages=(), locked_packages=()):
        with tempfile.TemporaryDirectory() as temporary, patch("check_agent_dependencies.run", run):
            root = Path(temporary)
            (root / BASELINE).parent.mkdir(parents=True)
            (root / BASELINE).write_text(json.dumps(FIXTURE_BASELINE), encoding="utf-8")
            lock = "version = 4\n" + "".join(
                "\n[[package]]\n"
                + "".join(f"{key} = {json.dumps(value)}\n" for key, value in package.items())
                for package in locked_packages
            )
            (root / "Cargo.lock").write_text(lock, encoding="utf-8")
            verify(root, {"packages": list(packages)})

    def test_production_graph_within_the_baseline_passes(self):
        self.audit(cargo(tree(SERVER)), [package("serde_json", "1.0.151", "MIT OR Apache-2.0")])

    def test_preserve_order_flip_fails(self):
        flipped = tree(SERVER, "default,float_roundtrip,indexmap,preserve_order,std")
        with self.assertRaisesRegex(ValueError, "serde_json enables preserve_order") as raised:
            self.audit(cargo(flipped))
        for target in RELEASE_TARGETS:
            self.assertIn(f"preserve_order for {target}", str(raised.exception))
        self.assertIn("serde_json feature indexmap", str(raised.exception))

    def test_rmcp_client_on_a_dev_only_edge_passes(self):
        self.audit(cargo(tree(SERVER), with_dev=tree("client," + SERVER)))

    def test_rmcp_client_on_a_normal_edge_fails(self):
        with self.assertRaisesRegex(ValueError, "rmcp feature client for .* reviewed allowlist"):
            self.audit(cargo(tree("client," + SERVER)))

    def test_new_gpl_package_fails(self):
        packages = [
            package("serde_json", "1.0.151", "GPL-3.0-only"),
            package("copyleft", "1.0.0", "MIT OR GPL-3.0-only"),
        ]
        with self.assertRaisesRegex(ValueError, r"copyleft@1\.0\.0 has a license") as raised:
            self.audit(cargo(tree(SERVER)), packages)
        # Packages already on main are outside this gate.
        self.assertNotIn("serde_json@", str(raised.exception))

    def test_new_mit_package_passes(self):
        self.audit(cargo(tree(SERVER)), [package("permissive", "1.0.0", "MIT")])

    def test_exact_aho_corasick_mit_choice_matches_locked_provenance(self):
        choice = dict(AHO_CORASICK_MIT_CHOICE)
        self.assertFalse(license_allowed(choice["license"]))
        self.assertTrue(selected_mit_license(choice, [choice]))
        self.assertFalse(selected_mit_license(choice, []))
        self.audit(cargo(tree(SERVER)), [choice], [choice])
        with self.assertRaisesRegex(ValueError, "aho-corasick@1.1.5 has a license"):
            self.audit(cargo(tree(SERVER)), [choice], [{**choice, "checksum": "0" * 64}])
        for field, changed in (
            ("name", "different"),
            ("version", "1.1.6"),
            ("license", "Unlicense OR MIT OR GPL-3.0-only"),
            ("source", "git+https://example.test/aho-corasick"),
        ):
            with self.subTest(metadata_field=field):
                self.assertFalse(selected_mit_license({**choice, field: changed}, [choice]))
        for field, changed in (
            ("name", "different"),
            ("version", "1.1.6"),
            ("source", "registry+https://example.test/index"),
            ("checksum", "0" * 64),
        ):
            with self.subTest(lock_field=field):
                self.assertFalse(selected_mit_license(choice, [{**choice, field: changed}]))

    def test_selected_mit_choice_record_matches_the_audit(self):
        record = json.loads(
            (ROOT / "licenses/rust/aho-corasick-1.1.5-mit-choice.json").read_text(encoding="utf-8")
        )
        for field, value in AHO_CORASICK_MIT_CHOICE.items():
            self.assertEqual(record[field], value)
        self.assertEqual(record["selected_license"], "MIT")
        self.assertEqual(set(record["upstream_texts"]), {"LICENSE-MIT", "UNLICENSE", "COPYING"})

    def test_hyper_under_rmcp_fails(self):
        with self.assertRaisesRegex(ValueError, "rmcp depends on hyper for .* network-free"):
            self.audit(cargo(tree(SERVER), rmcp=RMCP_TREE + "hyper v1.7.0\n"))

    def test_license_expressions(self):
        for expression in (
            "MIT",
            "Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT",
            "MIT/Apache-2.0",
            "(MIT OR Apache-2.0) AND Unicode-3.0",
            "BSD-2-Clause OR BSD-3-Clause OR ISC OR Zlib",
        ):
            with self.subTest(expression=expression):
                self.assertTrue(license_allowed(expression))
        for expression in (
            None,
            "",
            "MPL-2.0",
            "MIT OR GPL-3.0-only",
            "Apache-2.0 WITH Classpath-exception-2.0",
            "MIT OR",
        ):
            with self.subTest(expression=expression):
                self.assertFalse(license_allowed(expression))

    def test_rmcp_allowlist_stays_server_only(self):
        self.assertEqual(allowlist_problems(set(SERVER.split(","))), [])
        self.assertEqual(
            allowlist_problems({"client", "macros", "transport-io", "auth", "server-side-http"}),
            [
                "the rmcp allowlist must contain server",
                "the rmcp allowlist must not contain auth",
                "the rmcp allowlist must not contain client",
                "the rmcp allowlist must not contain macros",
                "the rmcp allowlist must not contain server-side-http",
                "the rmcp allowlist must not contain transport-io",
            ],
        )

    def test_committed_baseline_is_reviewed_and_reproducible(self):
        text = (ROOT / BASELINE).read_text(encoding="utf-8")
        baseline = json.loads(text)
        self.assertEqual(allowlist_problems(set(baseline["rmcp_features"])), [])
        self.assertEqual(list(baseline["serde_json_features"]), list(RELEASE_TARGETS))
        for features in baseline["serde_json_features"].values():
            self.assertFalse(SERDE_JSON_FORBIDDEN & set(features))
        self.assertEqual("\n".join(layout(baseline)) + "\n", text)


if __name__ == "__main__":
    unittest.main()
