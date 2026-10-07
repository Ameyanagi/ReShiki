"""The agent API reaches the filesystem only through reshiki_agent::access.

Two fences keep it that way: each agent crate's clippy.toml disallows direct
filesystem calls, and a text scan of the agent, MCP server and launch sources
flags filesystem, network and updater tokens outside the access module.
"""

import re
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

FS_FUNCTIONS = (
    "read",
    "read_to_string",
    "write",
    "copy",
    "rename",
    "remove_file",
    "remove_dir",
    "remove_dir_all",
    "create_dir",
    "create_dir_all",
    "hard_link",
    "canonicalize",
    "metadata",
    "symlink_metadata",
    "read_dir",
    "read_link",
    "set_permissions",
)
REQUIRED_METHODS = frozenset(
    [f"std::fs::{name}" for name in (*FS_FUNCTIONS, "exists")]
    + [f"tokio::fs::{name}" for name in (*FS_FUNCTIONS, "try_exists")]
    + ["cap_std::ambient_authority"]
)
REQUIRED_TYPES = frozenset(
    ["std::fs::File", "std::fs::OpenOptions", "tokio::fs::File", "tokio::fs::OpenOptions"]
)

TOKENS = (
    "std::fs",
    "tokio::fs",
    "File::open",
    "File::create",
    "OpenOptions",
    "NamedTempFile",
    "ambient_authority",
    "write_atomic",
    "data_directory(",
    "reqwest",
)
UPDATES = re.compile(r"updates::(?!CURRENT_VERSION\b)")


def fenced_crates() -> list[Path]:
    """crates/agent always; crates/mcp once it exists."""
    crates = [ROOT / "crates" / "agent"]
    if (ROOT / "crates" / "mcp").is_dir():
        crates.append(ROOT / "crates" / "mcp")
    return crates


def disallowed(config: dict, key: str) -> dict[str, str]:
    """Entry paths and their reasons; plain string entries have no reason."""
    entries = {}
    for entry in config.get(key, []):
        if isinstance(entry, str):
            entries[entry] = ""
        else:
            entries[entry["path"]] = entry.get("reason", "")
    return entries


def is_test_file(path: Path) -> bool:
    if path.name == "tests.rs" or path.stem.endswith("_tests"):
        return True
    return any(part == "tests" or part.endswith("_tests") for part in path.parent.parts)


def scanned_sources() -> list[Path]:
    """Agent sources outside the access module, MCP and launch sources; no tests."""
    agent = ROOT / "crates" / "agent" / "src"
    access = agent / "access"
    sources = [
        path
        for path in agent.rglob("*.rs")
        if path != agent / "access.rs" and access not in path.parents
    ]
    sources += (ROOT / "crates" / "mcp" / "src").rglob("*.rs")
    launch = ROOT / "src" / "launch.rs"
    if launch.is_file():
        sources.append(launch)
    sources += (ROOT / "src" / "launch").rglob("*.rs")
    return sorted(path for path in sources if not is_test_file(path.relative_to(ROOT)))


def violations(text: str) -> list[str]:
    """`line: token` for each fenced token in Rust source text."""
    found = []
    for number, line in enumerate(text.splitlines(), start=1):
        found += [f"{number}: {token}" for token in TOKENS if token in line]
        found += [f"{number}: {match.group(0)}" for match in UPDATES.finditer(line)]
    return found


class ClippyFenceTests(unittest.TestCase):
    def test_each_agent_crate_disallows_direct_filesystem_access(self):
        for crate in fenced_crates():
            with self.subTest(crate=crate.name):
                path = crate / "clippy.toml"
                self.assertTrue(path.is_file(), f"{path.relative_to(ROOT)} is missing")
                config = tomllib.loads(path.read_text(encoding="utf-8"))
                methods = disallowed(config, "disallowed-methods")
                types = disallowed(config, "disallowed-types")
                self.assertEqual(REQUIRED_METHODS - methods.keys(), set())
                self.assertEqual(REQUIRED_TYPES - types.keys(), set())
                unexplained = [name for name, reason in {**methods, **types}.items() if not reason]
                self.assertEqual(unexplained, [], "every entry needs a reason")


class SourceFenceTests(unittest.TestCase):
    def test_agent_mcp_and_launch_sources_use_no_fenced_tokens(self):
        sources = scanned_sources()
        self.assertIn(ROOT / "crates" / "agent" / "src" / "lib.rs", sources)
        found = {
            str(path.relative_to(ROOT)): hits
            for path in sources
            if (hits := violations(path.read_text(encoding="utf-8")))
        }
        self.assertEqual(found, {})

    def test_the_scanner_flags_fenced_tokens(self):
        self.assertEqual(
            violations('let _ = tokio::fs::read("x").await;'),
            ["1: tokio::fs"],
        )
        self.assertEqual(violations("use crate::updates::check;"), ["1: updates::"])
        self.assertEqual(violations("let v = updates::CURRENT_VERSION;"), [])

    def test_test_files_and_the_access_module_are_not_scanned(self):
        sources = scanned_sources()
        agent = ROOT / "crates" / "agent" / "src"
        self.assertNotIn(agent / "access.rs", sources)
        self.assertFalse([path for path in sources if (agent / "access") in path.parents])
        self.assertNotIn(agent / "canvas_tools" / "tests.rs", sources)
        self.assertNotIn(agent / "review" / "overlap_tests.rs", sources)


if __name__ == "__main__":
    unittest.main()
