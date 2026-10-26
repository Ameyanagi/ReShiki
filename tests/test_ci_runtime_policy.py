"""Keep the pre-checkout Node bootstrap and its official artifact pins consistent."""

import json
import re
import unittest
from pathlib import Path
from textwrap import dedent

ROOT = Path(__file__).resolve().parents[1]


class CIRuntimePolicyTests(unittest.TestCase):
    def test_every_action_job_starts_with_the_canonical_bootstrap(self):
        canonical = (ROOT / "scripts/check_actions_runtime.ps1").read_text().rstrip("\n")
        checked = 0
        for path in sorted((ROOT / ".github/workflows").glob("*.yml")):
            text = path.read_text()
            if not re.search(r"uses: [^\s]+@", text):
                continue
            self.assertIn("      - &actions_runtime\n", text, path.name)
            bootstrap = text.split("      - &actions_runtime\n", 1)[1]
            body = re.search(r"(?m)^        run: \|\n((?:          [^\n]*\n|\n)+)", bootstrap)
            self.assertIsNotNone(body, path.name)
            self.assertEqual(dedent(body[1]).rstrip("\n"), canonical, path.name)
            jobs = re.split(r"(?m)^  [\w-]+:\n", text.split("\njobs:\n", 1)[1])[1:]
            for job in jobs:
                if "    steps:\n" not in job:
                    continue
                first = job.split("    steps:\n", 1)[1].splitlines()[0]
                self.assertIn(
                    first,
                    ["      - &actions_runtime", "      - *actions_runtime"],
                    f"{path.name}: JavaScript actions must follow the bootstrap",
                )
                checked += 1
        self.assertGreaterEqual(checked, 20, "Lost workflow/job coverage")

    def test_bootstrap_uses_the_six_verified_official_node_artifacts(self):
        policy = json.loads((ROOT / "scripts/actions_node_bootstrap.json").read_text())
        self.assertEqual(policy["node"], "24.22.1")
        canonical = (ROOT / "scripts/check_actions_runtime.ps1").read_text()
        pins = dict(re.findall(r"@\('([^']+)', '([0-9a-f]{64})'\)", canonical))
        self.assertEqual(len(pins), 6)
        self.assertEqual(pins, policy["assets"])
        self.assertIn(f"https://nodejs.org/dist/v{policy['node']}/", canonical)


if __name__ == "__main__":
    unittest.main()
