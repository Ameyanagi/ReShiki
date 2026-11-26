"""Keep the xTB extra pinned to the corresponding worker wheel."""

import re
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def workspace_version():
    with (ROOT / "Cargo.toml").open("rb") as file:
        return tomllib.load(file)["workspace"]["package"]["version"]


class PythonXtbPinTests(unittest.TestCase):
    def test_application_and_wheel_versions_match(self):
        with (ROOT / "Cargo.toml").open("rb") as file:
            manifest = tomllib.load(file)
        self.assertEqual(
            manifest["package"]["version"], manifest["workspace"]["package"]["version"]
        )

    def test_extra_pins_matching_worker_version(self):
        with (ROOT / "crates/python/pyproject.toml").open("rb") as file:
            dependencies = tomllib.load(file)["project"]["optional-dependencies"]["xtb"]

        # The stable version is identical in Cargo and PEP 440. Cargo release
        # candidate versions such as 1.2.3-rc.1 are published as 1.2.3rc1.
        version = re.sub(r"-(alpha|beta|rc)\.?([0-9]+)$", self._prerelease, workspace_version())
        requirements = [
            dependency.split(";", 1)[0].strip()
            for dependency in dependencies
            if dependency.startswith("reshiki-xtb")
        ]
        self.assertEqual(requirements, [f"reshiki-xtb=={version}"])

    def test_readme_links_to_corresponding_source(self):
        readme = (ROOT / "crates/python-xtb/README.md").read_text(encoding="utf-8")
        self.assertIn(
            f"(https://github.com/Ameyanagi/ReShiki/releases/tag/v{workspace_version()})",
            readme,
        )

    @staticmethod
    def _prerelease(match):
        stage = {"alpha": "a", "beta": "b", "rc": "rc"}[match[1]]
        return f"{stage}{int(match[2])}"


if __name__ == "__main__":
    unittest.main()
