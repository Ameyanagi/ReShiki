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
    AXTB_SOURCE,
    BASELINE,
    ROOT,
    SERDE_JSON_FORBIDDEN,
    WEBPKI_ROOT_CERTS_CDLA_REVIEW,
    allowlist_problems,
    copyleft,
    layout,
    license_allowed,
    reviewed_certificate_data_license,
    selected_mit_license,
    verify,
)

SERVER = "schemars,server,transport-async-rw,uuid"
SERDE_JSON = "alloc,default,float_roundtrip,std"
RMCP_TREE = "rmcp v3.5.1\nserde_derive v1.0.229 (proc-macro)\ntokio v1.53.1\ntokio v1.53.1 (*)\n"
REVISION = "1234567890" * 4
FORK_QUERY = f"{AXTB_SOURCE}?rev={REVISION}"
FORK_TREE_SOURCE = f"{FORK_QUERY}#{REVISION[:8]}"
FORK_LOCK_SOURCE = f"git+{FORK_QUERY}#{REVISION}"
FORK_NAMES = (
    "axtb",
    "axtb-base",
    "axtb-d4",
    "axtb-driver",
    "axtb-gfnff",
    "axtb-gxtb",
    "axtb-solvation",
)
FORK_TREE = "".join(
    f"{name} v0.2.0 ({FORK_TREE_SOURCE})|LGPL-3.0-or-later\n" for name in FORK_NAMES
)
DEFAULT_LICENSE_TREE = (
    "reshiki v0.17.1 (/repo (test))|MIT OR Apache-2.0\n"
    "pastey v0.2.3 (proc-macro)|MIT OR Apache-2.0 (*)\n"
)
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


def cargo(
    production,
    *,
    with_dev=None,
    rmcp=RMCP_TREE,
    default_licenses=DEFAULT_LICENSE_TREE,
    xtb_licenses=FORK_TREE,
    extra_xtb_packages=(),
):
    """Answer like cargo: dev-only features appear only when dev edges are requested."""

    def run(root, *command):
        if command[1] == "metadata":
            assert command[-2:] == ("--features", "xtb")
            packages = [
                {
                    **package(name, "0.2.0", "LGPL-3.0-or-later"),
                    "source": FORK_LOCK_SOURCE,
                    "manifest_path": str(root / "fork/crates/driver/Cargo.toml"),
                }
                for name in FORK_NAMES
            ]
            return json.dumps({"packages": [*packages, *extra_xtb_packages]})
        if command[-1] == "{p}|{l}":
            assert command[command.index("-e") + 1] == "normal,build"
            if "--workspace" in command:
                assert "--features" not in command
                return default_licenses
            assert command[3:7] == ("-p", "reshiki", "--features", "xtb")
            return xtb_licenses
        if command[command.index("-p") + 1] == "rmcp":
            return rmcp
        edges = command[command.index("-e") + 1].split(",")
        return with_dev if with_dev and "dev" in edges else production

    return run


def package(name, version, license):
    return {"name": name, "version": version, "license": license}


class AgentDependencyTests(unittest.TestCase):
    def audit(
        self, run, packages=(), locked_packages=(), *, source_revision=REVISION, stale_patches=()
    ):
        with tempfile.TemporaryDirectory() as temporary, patch("check_agent_dependencies.run", run):
            root = Path(temporary)
            (root / BASELINE).parent.mkdir(parents=True)
            (root / BASELINE).write_text(json.dumps(FIXTURE_BASELINE), encoding="utf-8")
            fork_packages = [
                {"name": name, "version": version, "source": FORK_LOCK_SOURCE}
                for name, version in (
                    ("axtb-driver", "0.2.0"),
                    ("gemm-common", "0.18.2"),
                    ("private-gemm-x86", "0.1.22"),
                )
                if name not in stale_patches
            ]
            lock = "version = 4\n" + "".join(
                "\n[[package]]\n"
                + "".join(f"{key} = {json.dumps(value)}\n" for key, value in package.items())
                for package in (*fork_packages, *locked_packages)
            )
            (root / "Cargo.lock").write_text(lock, encoding="utf-8")
            worker = root / "crates/xtb-worker/src/lib.rs"
            worker.parent.mkdir(parents=True)
            worker.write_text(f'pub const AXTB_REV: &str = "{REVISION}";\n', encoding="utf-8")
            notices = root / "licenses/axtb"
            notices.mkdir(parents=True)
            (notices / "SOURCE").write_text(
                "Fork:       https://github.com/Ameyanagi/axtb, branch reshiki\n"
                f"Revision:   {source_revision}\n",
                encoding="utf-8",
            )
            fork = root / "fork"
            (fork / "crates/driver").mkdir(parents=True)
            for name, source in (
                ("COPYING", fork / "crates/driver"),
                ("COPYING.LESSER", fork / "crates/driver"),
                ("NOTICE", fork / "crates/driver"),
                ("CHANGES-reshiki.md", fork),
            ):
                terms = f"Upstream {name}\r\n".encode()
                (source / name).write_bytes(terms)
                (notices / name).write_bytes(terms)
            verify(root, {"packages": list(packages)})

    def test_production_graph_within_the_baseline_passes(self):
        self.audit(cargo(tree(SERVER)), [package("serde_json", "1.0.151", "MIT OR Apache-2.0")])

    def test_copyleft_expressions(self):
        for expression, expected in (
            (None, False),
            ("", False),
            ("MIT OR (Apache-2.0 AND BSD-3-Clause)", False),
            ("LicenseRef-Not-GPL-3.0-only", False),
            ("GPL", True),
            ("GPL-2.0", True),
            ("LGPL-3.0-or-later", True),
            ("AGPL-3.0-only", True),
            ("GPL-3.0+ WITH Classpath-exception-2.0", True),
            ("LGPL-2.1", True),
            ("MIT/(Apache-2.0 AND LGPL-3.0-only)", False),
            # An OR with a non-GPL alternative is used under that alternative.
            ("Apache-2.0 OR GPL-2.0-only", False),
            ("Apache-2.0/GPL-2.0-only", False),
            ("GPL-2.0-only OR LGPL-3.0-only", True),
            ("MIT AND LGPL-2.1", True),
            ("MIT AND gpl-3.0-only", True),
            ("(Apache-2.0 OR GPL-2.0-only)", False),
            ("MIT OR (Apache-2.0 AND LGPL-3.0-only)", False),
            ("(MIT OR GPL-3.0-only) AND LGPL-2.1-or-later", True),
            ("GPL-2.0-only WITH Classpath-exception-2.0 OR MIT", False),
        ):
            with self.subTest(expression=expression):
                self.assertEqual(copyleft(expression), expected)

    def test_default_graph_rejects_copyleft_and_the_fork(self):
        default_licenses = DEFAULT_LICENSE_TREE + (
            "transitive v1.2.0|LGPL-2.1\n"
            # Cargo's order: the proc-macro marker comes before the source.
            f"other-name v0.2.0 (proc-macro) ({FORK_TREE_SOURCE})|MIT (*)\n"
        )
        with self.assertRaisesRegex(ValueError, "xTB rule 1") as raised:
            self.audit(cargo(tree(SERVER), default_licenses=default_licenses))
        for target in (*RELEASE_TARGETS, "wasm32-unknown-unknown"):
            self.assertIn(
                f"default graph for {target}: transitive@1.2.0 declares", str(raised.exception)
            )
            self.assertIn(
                f"default graph for {target}: other-name@0.2.0 comes from", str(raised.exception)
            )

    def test_xtb_graph_requires_exactly_the_pinned_fork_with_lgpl(self):
        self.audit(cargo(tree(SERVER), xtb_licenses=FORK_TREE))
        first, *remaining = FORK_TREE.splitlines(keepends=True)
        rest = "".join(remaining)
        wrong_rev = REVISION.replace("1", "2")
        for xtb_licenses, message in (
            (rest, "missing axtb fork packages: axtb"),
            (FORK_TREE + "extra v1.2.0|GPL-3.0-only\n", "unexpected copyleft package extra"),
            (first.replace(REVISION, wrong_rev) + rest, "axtb@0.2.0 has the wrong pinned source"),
            (
                first.replace("LGPL-3.0-or-later", "MIT") + rest,
                "axtb@0.2.0 must declare LGPL-3.0-or-later",
            ),
        ):
            with (
                self.subTest(message=message),
                self.assertRaisesRegex(ValueError, "xTB rule 2") as raised,
            ):
                self.audit(cargo(tree(SERVER), xtb_licenses=xtb_licenses))
            self.assertIn(message, str(raised.exception))

    def test_xtb_metadata_exempts_only_expected_names_from_pinned_fork(self):
        extra_xtb_packages = (
            {
                **package("axtb", "1.0.0", "LGPL-3.0-or-later"),
                "source": "git+https://example.test/axtb",
            },
            {**package("axtb-extra", "0.2.0", "LGPL-3.0-or-later"), "source": FORK_LOCK_SOURCE},
        )
        with self.assertRaisesRegex(ValueError, "xTB rule 3") as raised:
            self.audit(cargo(tree(SERVER), extra_xtb_packages=extra_xtb_packages))
        self.assertIn("axtb@1.0.0 has a license", str(raised.exception))
        self.assertIn("axtb-extra@0.2.0 has a license", str(raised.exception))
        self.assertNotIn("axtb-driver@0.2.0 has a license", str(raised.exception))

    def test_pin_consistency_requires_the_gemm_patches_at_the_pin(self):
        stale = {"name": "gemm-common", "version": "0.18.2", "source": "registry+x"}
        with self.assertRaisesRegex(ValueError, "Cargo.lock gemm-common source must be the fork"):
            self.audit(cargo(tree(SERVER)), locked_packages=[stale], stale_patches={"gemm-common"})

    def test_pin_consistency_rejects_a_different_source_revision(self):
        with self.assertRaisesRegex(
            ValueError, r"licenses/axtb/SOURCE must have one Revision line"
        ):
            self.audit(cargo(tree(SERVER)), source_revision="0" * 40)

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

    def test_certificate_data_review_matches_only_locked_provenance(self):
        reviewed = dict(WEBPKI_ROOT_CERTS_CDLA_REVIEW)
        self.assertFalse(license_allowed(reviewed["license"]))
        self.assertTrue(reviewed_certificate_data_license(reviewed, [reviewed]))
        self.assertFalse(reviewed_certificate_data_license(reviewed, []))
        self.audit(cargo(tree(SERVER)), [reviewed], [reviewed])
        with self.assertRaisesRegex(ValueError, "webpki-root-certs@1.0.10 has a license"):
            self.audit(cargo(tree(SERVER)), [reviewed], [{**reviewed, "checksum": "0" * 64}])
        for field, changed in (
            ("name", "different"),
            ("version", "1.0.11"),
            ("license", "CDLA-Permissive-2.0 OR GPL-3.0-only"),
            ("source", "git+https://example.test/webpki-root-certs"),
        ):
            with self.subTest(metadata_field=field):
                self.assertFalse(
                    reviewed_certificate_data_license({**reviewed, field: changed}, [reviewed])
                )
        for field, changed in (
            ("name", "different"),
            ("version", "1.0.11"),
            ("source", "registry+https://example.test/index"),
            ("checksum", "0" * 64),
        ):
            with self.subTest(lock_field=field):
                self.assertFalse(
                    reviewed_certificate_data_license(reviewed, [{**reviewed, field: changed}])
                )

    def test_certificate_data_review_record_matches_the_audit(self):
        record = json.loads(
            (ROOT / "licenses/rust/webpki-root-certs-1.0.10-cdla-review.json").read_text(
                encoding="utf-8"
            )
        )
        for field, value in WEBPKI_ROOT_CERTS_CDLA_REVIEW.items():
            self.assertEqual(record[field], value)
        self.assertEqual(set(record["upstream_texts"]), {"LICENSE"})

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
            "MIT AND (MIT OR Apache-2.0) AND Python-2.0",
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
            "CDLA-Permissive-1.0",
            "CDLA-Sharing-1.0",
            "CDLA-Permissive-2.0",
            "CDLA-Permissive-2.0 OR GPL-3.0-only",
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
