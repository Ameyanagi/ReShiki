"""Signing policy and publication gates must fail closed for production artifacts."""

import os
import re
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[1]
RELEASE = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
NIGHTLY = (ROOT / ".github/workflows/nightly.yml").read_text(encoding="utf-8")


def job(text: str, name: str) -> str:
    return re.search(rf"^  {name}:\n(.*?)(?=^  \w+:|\Z)", text, re.M | re.S)[1]


def condition(text: str, name: str) -> str:
    match = re.search(r"^    if: (.*)\n((?:      .*\n)*)", job(text, name), re.M)
    return match[2].strip() if match[1] == ">-" else match[1]


def evaluate(expression: str, **context):
    """Evaluate the checked-in gates' boolean/string subset with supplied events."""
    expression = expression.strip().removeprefix("${{").removesuffix("}}").strip()
    expression = expression.replace("\n", " ").replace("&&", " and ").replace("||", " or ")
    return eval(expression, {"__builtins__": {}, "always": lambda: True}, context)


class WindowsSigningWorkflowTests(unittest.TestCase):
    def test_stable_tags_force_release_policy_and_callers_select_their_policy(self):
        enabled = re.search(r"^          SIGN_WINDOWS: (.*)$", RELEASE, re.M)[1]
        policy = re.search(r"^          SIGNING_POLICY: (.*)$", RELEASE, re.M)[1]
        cases = (
            ("push", "true", False, "test-signing", True, "release-signing"),
            ("push", "false", True, "test-signing", False, "release-signing"),
            ("workflow_dispatch", "true", False, "test-signing", False, "test-signing"),
            ("workflow_dispatch", "false", True, "test-signing", True, "test-signing"),
            ("workflow_dispatch", "false", True, "release-signing", True, "release-signing"),
            # Omitted reusable inputs receive the declared release-signing default.
            ("schedule", "false", True, "release-signing", True, "release-signing"),
            ("schedule", "false", True, "test-signing", True, "test-signing"),
            ("workflow_dispatch", "false", True, "", True, ""),
        )
        for event, flag, requested, chosen, expected_enabled, expected_policy in cases:
            with self.subTest(event=event, flag=flag, requested=requested, chosen=chosen):
                context = dict(
                    github=SimpleNamespace(event_name=event),
                    inputs=SimpleNamespace(sign_windows=requested, windows_signing_policy=chosen),
                    vars=SimpleNamespace(SIGNPATH_ENABLED=flag),
                )
                self.assertEqual(bool(evaluate(enabled, **context)), expected_enabled)
                self.assertEqual(evaluate(policy, **context), expected_policy)

    @unittest.skipUnless(os.name == "posix" and shutil.which("bash"), "selector executes on Ubuntu")
    def test_policy_selector_rejects_empty_or_unknown_inputs(self):
        selector = re.search(
            r"      - name: Select the Windows signing mode.*?        run: \|\n"
            r"((?:          [^\n]*\n)+)",
            RELEASE,
            re.S,
        )[1]
        selector = "\n".join(line.removeprefix("          ") for line in selector.splitlines())
        for policy in ("test-signing", "release-signing", "", "unknown"):
            with self.subTest(policy=policy), tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary) / "output"
                result = subprocess.run(
                    ["bash", "-e", "-o", "pipefail", "-c", selector],
                    env={
                        **os.environ,
                        "SIGNING_POLICY": policy,
                        "SIGN_WINDOWS": "true",
                        "GITHUB_OUTPUT": str(output),
                    },
                    capture_output=True,
                    text=True,
                )
                accepted = policy in ("test-signing", "release-signing")
                self.assertEqual(result.returncode == 0, accepted, result.stderr)
                if accepted:
                    self.assertEqual(output.read_text(), f"enabled=true\npolicy={policy}\n")
                else:
                    self.assertFalse(output.exists())

    def test_nightly_test_signatures_remain_ci_artifacts_and_production_takes_precedence(self):
        packages = job(NIGHTLY, "packages")
        requested = re.search(r"^      sign_windows: (.*)$", packages, re.M)[1]
        selected = re.search(r"^      windows_signing_policy: (.*)$", packages, re.M)[1]
        publication = re.search(r"^          WINDOWS_SIGNING: (.*)$", NIGHTLY, re.M)[1]
        description = re.search(r"^          WINDOWS_SIGNING_DESCRIPTION: (.*)$", NIGHTLY, re.M)[1]
        downloads = re.findall(r"^        if: (.*)$", job(NIGHTLY, "publish"), re.M)
        self.assertEqual(len(downloads), 2)
        for production, testing, enabled, policy, published in (
            ("false", "false", False, "test-signing", "unsigned"),
            ("false", "true", True, "test-signing", "unsigned"),
            ("true", "false", True, "release-signing", "required"),
            ("true", "true", True, "release-signing", "required"),
        ):
            with self.subTest(production=production, testing=testing):
                variables = SimpleNamespace(
                    SIGNPATH_NIGHTLY_ENABLED=production, SIGNPATH_NIGHTLY_TEST_ENABLED=testing
                )
                self.assertEqual(evaluate(requested, vars=variables), enabled)
                self.assertEqual(evaluate(selected, vars=variables), policy)
                needs = SimpleNamespace(
                    packages=SimpleNamespace(
                        outputs=SimpleNamespace(
                            windows_signing_enabled=str(enabled).lower(),
                            windows_signing_policy=policy,
                        )
                    )
                )
                self.assertEqual(evaluate(publication, needs=needs), published)
                signed = published == "required"
                self.assertEqual(evaluate(downloads[0], needs=needs), signed)
                self.assertEqual(evaluate(downloads[1], needs=needs), not signed)
                self.assertEqual(
                    evaluate(description, needs=needs),
                    "Windows application executables and installers are signed through SignPath."
                    if signed
                    else "Windows packages remain unsigned.",
                )

    def test_stable_publication_accepts_disabled_signing_but_blocks_signing_failure(self):
        gate = condition(RELEASE, "publish")
        for enabled, signing_result, expected in (
            ("true", "success", True),
            ("false", "skipped", True),
            ("true", "failure", False),
            ("true", "cancelled", False),
            ("true", "skipped", False),
            ("false", "failure", False),
        ):
            needs = SimpleNamespace(
                validate=SimpleNamespace(
                    result="success", outputs=SimpleNamespace(windows_signing_enabled=enabled)
                ),
                build=SimpleNamespace(result="success"),
                sign=SimpleNamespace(result="success"),
                sign_windows=SimpleNamespace(result=signing_result),
                reference=SimpleNamespace(result="success"),
            )
            with self.subTest(enabled=enabled, signing=signing_result):
                self.assertEqual(
                    evaluate(gate, github=SimpleNamespace(event_name="push"), needs=needs), expected
                )
            if expected:
                for required in ("validate", "build", "sign", "reference"):
                    getattr(needs, required).result = "failure"
                    self.assertFalse(
                        evaluate(gate, github=SimpleNamespace(event_name="push"), needs=needs)
                    )
                    getattr(needs, required).result = "success"
                self.assertFalse(
                    evaluate(
                        gate, github=SimpleNamespace(event_name="workflow_dispatch"), needs=needs
                    )
                )

    def test_nightly_publication_requires_a_successful_main_build(self):
        gate = condition(NIGHTLY, "publish")
        for ref, result, expected in (
            ("refs/heads/main", "success", True),
            ("refs/heads/main", "failure", False),
            ("refs/heads/main", "skipped", False),
            ("refs/heads/feature", "success", False),
        ):
            with self.subTest(ref=ref, result=result):
                self.assertEqual(
                    evaluate(
                        gate,
                        github=SimpleNamespace(ref=ref),
                        needs=SimpleNamespace(packages=SimpleNamespace(result=result)),
                    ),
                    expected,
                )

    @unittest.skipUnless(os.name == "posix", "production source guard executes on Ubuntu")
    @unittest.skipUnless(shutil.which("git") and shutil.which("bash"), "requires Git and Bash")
    def test_production_source_guard_checks_ref_and_actual_main_ancestry(self):
        guard = re.search(
            r"      - name: Require production signing.*?        run: \|\n"
            r"((?:          [^\n]*\n)+)",
            RELEASE,
            re.S,
        )[1]
        guard = "\n".join(line.removeprefix("          ") for line in guard.splitlines())
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            origin = root / "origin"
            origin.mkdir()

            def git(*arguments, cwd=origin):
                return subprocess.run(
                    ["git", *arguments], cwd=cwd, check=True, capture_output=True, text=True
                )

            git("init", "--initial-branch=main")
            git("config", "user.name", "Workflow test")
            git("config", "user.email", "workflow-test@example.invalid")
            git("commit", "--allow-empty", "-m", "main base")
            git("tag", "v1.2.3")
            git("branch", "feature")
            git("checkout", "-b", "unmerged")
            git("commit", "--allow-empty", "-m", "unmerged change")
            git("tag", "v9.9.9")
            git("checkout", "main")
            git("commit", "--allow-empty", "-m", "main change")
            for index, (ref, branch, expected) in enumerate(
                (
                    ("refs/heads/main", "main", 0),
                    ("refs/tags/v1.2.3", "v1.2.3", 0),
                    # Even a branch whose HEAD is already on main cannot release-sign.
                    ("refs/heads/feature", "feature", 1),
                    ("refs/tags/v9.9.9", "v9.9.9", 1),
                )
            ):
                checkout = root / f"checkout-{index}"
                git(
                    "clone",
                    "--no-local",
                    "--depth",
                    "1",
                    "--branch",
                    branch,
                    str(origin),
                    str(checkout),
                )
                with self.subTest(ref=ref):
                    result = subprocess.run(
                        ["bash", "-e", "-o", "pipefail", "-c", guard],
                        cwd=checkout,
                        env={**os.environ, "GITHUB_REF": ref},
                        capture_output=True,
                        text=True,
                    )
                    self.assertEqual(result.returncode, expected, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
