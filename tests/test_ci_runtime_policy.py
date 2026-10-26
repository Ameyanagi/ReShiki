"""Fail closed before JavaScript actions, independently of setup-node's PATH."""

import ast
import json
import os
import re
import shutil
import subprocess
import unittest
from pathlib import Path
from textwrap import dedent

ROOT = Path(__file__).resolve().parents[1]
GUARD = ROOT / "scripts/check_actions_runtime.ps1"


def pinned_at_floor(value, floors):
    """Require an exact supported version, while allowing newer security patches."""
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", value):
        return False
    version = tuple(map(int, value.split(".")))
    return version[0] in floors and version >= floors[version[0]]


def status_condition(condition, guard_passed, later_failure):
    """Evaluate the small status-condition subset used by artifact steps."""
    expression = condition.removeprefix("${{").removesuffix("}}").strip()
    for function, value in {
        "always()": True,
        "failure()": later_failure,
        "success()": not later_failure,
        "cancelled()": False,
    }.items():
        expression = expression.replace(function, repr(value))
    expression = expression.replace(
        "steps.actions_runtime_guard.outcome", repr("success" if guard_passed else "failure")
    )
    expression = re.sub(r"hashFiles\('[^']+'\)", repr("created-artifact"), expression)
    expression = expression.replace("&&", "and").replace("||", "or")
    tree = ast.parse(expression, mode="eval")
    allowed = (
        ast.Expression,
        ast.BoolOp,
        ast.And,
        ast.Or,
        ast.Compare,
        ast.Eq,
        ast.NotEq,
        ast.Constant,
    )
    if any(not isinstance(node, allowed) for node in ast.walk(tree)):
        raise ValueError("Review the new artifact condition syntax")
    return eval(compile(tree, "artifact condition", "eval"), {"__builtins__": {}})


class CIRuntimePolicyTests(unittest.TestCase):
    def test_every_action_job_starts_with_the_exact_pre_checkout_guard(self):
        canonical = GUARD.read_text(encoding="utf-8").rstrip("\n")
        workflows = ROOT / ".github/workflows"
        checked = 0
        for path in sorted(workflows.glob("*.yml")):
            text = path.read_text(encoding="utf-8")
            if not re.search(r"uses: [^\s]+@", text):
                continue  # The license job runs only a trusted inline Python check.
            match = re.search(
                r"      - &actions_runtime\n"
                r"        name: Require a patched GitHub Actions Node runtime\n"
                r"        id: actions_runtime_guard\n"
                r"        shell: pwsh\n"
                r"        env:\n"
                r"          RESHIKI_RUNNER_ENVIRONMENT: \$\{\{ runner.environment \}\}\n"
                r"          RESHIKI_RUNNER_OS: \$\{\{ runner.os \}\}\n"
                r"          RESHIKI_RUNNER_ARCH: \$\{\{ runner.arch \}\}\n"
                r"        run: \|\n((?:          [^\n]*\n|\n)+)",
                text,
            )
            self.assertIsNotNone(match, f"{path.name}: missing canonical guard")
            self.assertEqual(dedent(match[1]).rstrip("\n"), canonical, path.name)
            jobs = re.split(r"(?m)^  [\w-]+:\n", text.split("\njobs:\n", 1)[1])[1:]
            for job in jobs:
                if "    steps:\n" not in job:
                    continue  # Reusable workflows guard their actual execution jobs.
                self.assertNotIn("hashFiles(", job.split("    steps:\n", 1)[0], path.name)
                first = job.split("    steps:\n", 1)[1].splitlines()[0]
                self.assertIn(
                    first,
                    ["      - &actions_runtime", "      - *actions_runtime"],
                    f"{path.name}: an action can run before the runtime check",
                )
                checked += 1
        self.assertGreaterEqual(checked, 20, "Lost workflow/job coverage")

    def test_bootstrap_pins_and_pre_step_runtime_boundaries(self):
        policy = json.loads((ROOT / "scripts/actions_node_bootstrap.json").read_text())
        self.assertEqual(policy["node"], "24.22.1")
        self.assertEqual(
            policy["primary_signer_fingerprint"],
            "5BE8A3F6C8A5C01D106C0AD820B1A390B168D356",
        )
        self.assertEqual(len(policy["assets"]), 6)
        canonical = GUARD.read_text()
        for name, sha256 in policy["assets"].items():
            self.assertIn(f"@('{name}', '{sha256}')", canonical)
        for path in (ROOT / ".github/workflows").glob("*.yml"):
            text = path.read_text()
            self.assertNotRegex(text, r"ACTIONS_ALLOW_USE_UNSECURE_NODE_VERSION\s*:")
            self.assertNotRegex(
                text, r"ACTIONS_RUNNER_(?:FORCED_INTERNAL|FORCE_ACTIONS)_NODE_VERSION\s*:"
            )
            # Step expressions before the canonical first step may execute the
            # old runner's internal Node, even without a JavaScript action.
            before = text.split("      - &actions_runtime", 1)[0]
            self.assertNotIn("hashFiles(", before, path.name)
            self.assertNotRegex(text, r"(?m)^    container:", path.name)

    def test_status_steps_reject_guard_failure_but_retain_later_failure_artifacts(self):
        checked = 0
        for path in sorted((ROOT / ".github/workflows").glob("*.yml")):
            text = path.read_text(encoding="utf-8")
            for condition in re.findall(r"(?m)^        if: ([^\n]+)", text):
                if not re.search(r"\b(?:always|failure|success|cancelled)\(", condition):
                    continue
                self.assertFalse(status_condition(condition, False, True), path.name)
                self.assertFalse(status_condition(condition, False, False), path.name)
                self.assertTrue(status_condition(condition, True, True), path.name)
                if "hashFiles(" in condition:
                    self.assertLess(
                        condition.index("steps.actions_runtime_guard.outcome"),
                        condition.index("hashFiles("),
                    )
                checked += 1
        self.assertGreaterEqual(checked, 4)
        self.assertTrue(
            status_condition("always()", False, True), "Control must expose the old bypass"
        )

    def test_workflow_runtime_and_pages_pins(self):
        package = json.loads((ROOT / "package.json").read_text(encoding="utf-8"))
        self.assertTrue(package["packageManager"].startswith("bun@"))
        self.assertTrue(pinned_at_floor(package["packageManager"][4:], {1: (1, 4, 4)}))
        self.assertTrue(package["engines"]["bun"].startswith(">="))
        self.assertTrue(pinned_at_floor(package["engines"]["bun"][2:], {1: (1, 4, 4)}))
        self.assertLessEqual(
            tuple(map(int, package["engines"]["bun"][2:].split("."))),
            tuple(map(int, package["packageManager"][4:].split("."))),
        )
        for path in sorted((ROOT / ".github/workflows").glob("*.yml")):
            text = path.read_text(encoding="utf-8")
            for action in re.finditer(r"uses: (\S+)", text):
                value = action[1]
                if value.startswith("./"):
                    continue
                self.assertRegex(value, r"^[^@]+@[0-9a-f]{40}$", path.name)
                block = re.split(r"\n      - |\n  [\w-]+:\n", text[action.end() :], 1)[0]
                if value.startswith("astral-sh/setup-uv@"):
                    declared = re.search(r'(?m)^\s+version: "([^"]+)"$', block)
                    self.assertIsNotNone(declared, path.name)
                    self.assertTrue(pinned_at_floor(declared[1], {0: (0, 13, 5)}), path.name)
                if value.startswith("actions/setup-node@"):
                    declared = re.search(r'(?m)^\s+node-version: "([^"]+)"$', block)
                    self.assertIsNotNone(declared, path.name)
                    self.assertTrue(
                        pinned_at_floor(declared[1], {22: (22, 23, 4), 24: (24, 22, 1)}),
                        path.name,
                    )
                    self.assertIn("package-manager-cache: false", block)
                if value.startswith("oven-sh/setup-bun@"):
                    self.assertIn("bun-version-file: package.json", block, path.name)
        docs = (ROOT / ".github/workflows/docs.yml").read_text(encoding="utf-8")
        self.assertIn("actions/deploy-pages@368f82528645a54fb793d4d04e342629a3f51346", docs)
        self.assertIn(
            "actions/upload-pages-artifact@fc324d3547104276b827a68afc52ff2a11cc49c9", docs
        )

    def test_pin_policy_allows_future_patches_and_rejects_floating_or_old_versions(self):
        for value in ["22.23.4", "22.23.5", "24.22.1", "24.22.2", "24.23.0"]:
            self.assertTrue(pinned_at_floor(value, {22: (22, 23, 4), 24: (24, 22, 1)}))
        for value in [
            "22",
            "24.x",
            "lts/*",
            "^24.22.1",
            "24.22.1-rc.1",
            "22.23.3",
            "24.22.0",
            "20.20.2",
        ]:
            self.assertFalse(pinned_at_floor(value, {22: (22, 23, 4), 24: (24, 22, 1)}))
        self.assertTrue(pinned_at_floor("1.4.5", {1: (1, 4, 4)}))
        self.assertTrue(pinned_at_floor("0.13.6", {0: (0, 13, 5)}))

    def test_uv_python_requests_use_available_managed_patch_floor(self):
        for path in sorted((ROOT / ".github/workflows").glob("*.yml")):
            text = path.read_text(encoding="utf-8")
            if "astral-sh/setup-uv@" not in text:
                continue
            self.assertIn("\nenv:\n  UV_PYTHON_PREFERENCE: only-managed\n", text, path.name)
            requests = re.findall(r"(?m)^\s+(?:python-version|python): ([^\n]+)", text)
            requests += re.findall(r"--python ([^\s]+)", text)
            for value in requests:
                if value.startswith("${{"):
                    continue  # Matrices are checked through their literal entries above.
                match = re.fullmatch(
                    r"(?:cpython-)?(3\.12\.[0-9]+)(?:-windows-x86_64-none)?", value.strip('"')
                )
                self.assertIsNotNone(match, f"{path.name}: floating or unsupported Python {value}")
                self.assertGreaterEqual(tuple(map(int, match[1].split("."))), (3, 12, 15))

    def test_pinned_actions_have_audited_node24_manifests_and_no_pre_hooks(self):
        policy = json.loads(
            (ROOT / ".github/actions/runtime-manifests.json").read_text(encoding="utf-8")
        )["actions"]
        pending = set()
        for path in (ROOT / ".github/workflows").glob("*.yml"):
            pending.update(re.findall(r"uses:\s*([^\s#]+@[^\s#]+)", path.read_text()))
        visited = set()
        while pending:
            action = pending.pop()
            if action in visited:
                continue
            visited.add(action)
            self.assertIn(
                action, policy, f"Review the immutable action manifest before using {action}"
            )
            entry = policy[action]
            self.assertIn(entry["using"], {"node24", "composite"}, action)
            self.assertIsNone(entry["pre"], f"{action}: pre hooks can run before the runtime guard")
            self.assertRegex(entry["metadata_sha256"], r"^[0-9a-f]{64}$")
            repository, revision = action.split("@")
            self.assertEqual(
                entry["metadata_url"],
                f"https://github.com/{repository}/blob/{revision}/action.yml",
            )
            pending.update(entry["nested_uses"])
        self.assertEqual(set(policy), visited, "Remove stale action audits")

    def test_actual_powershell_discovery_and_version_functions(self):
        pwsh = shutil.which("pwsh")
        if not pwsh:
            if os.environ.get("CI"):
                self.fail("CI must execute the guard tests with PowerShell")
            self.skipTest("PowerShell is unavailable on this developer machine")
        result = subprocess.run(
            [pwsh, "-NoLogo", "-NoProfile", "-File", str(ROOT / "tests/actions_runtime_guard.ps1")],
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("PASS: real guard path/version functions", result.stdout)

    def test_actual_powershell_bootstrap_in_owned_sandbox(self):
        pwsh = shutil.which("pwsh")
        node = os.environ.get("RESHIKI_TEST_NODE") or shutil.which("node")
        if not pwsh or (not node and not os.environ.get("CI")):
            if os.environ.get("CI"):
                self.fail("CI must execute the bootstrap sandbox tests")
            self.skipTest("PowerShell and patched Node are required for the sandbox test")
        command = [
            pwsh,
            "-NoLogo",
            "-NoProfile",
            "-File",
            str(ROOT / "tests/actions_runtime_bootstrap.ps1"),
        ]
        if not os.environ.get("CI"):
            command.extend(["-NodeExecutable", str(Path(node).resolve())])
        if archive := os.environ.get("RESHIKI_TEST_NODE_ARCHIVE"):
            command.extend(["-OfficialArchive", archive])
        result = subprocess.run(command, capture_output=True, text=True, timeout=180, check=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("PASS: real bootstrap functions", result.stdout)


if __name__ == "__main__":
    unittest.main()
