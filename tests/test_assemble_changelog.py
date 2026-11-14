"""The unreleased changes page is assembled from one fragment per change."""

import contextlib
import io
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import assemble_changelog

HEAD = "# Unreleased changes\n\nSee [ReShiki 0.15.0](changes-0.15.md).\n"


class AssembleChangelogTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.docs = Path(temporary.name)
        self.fragments = self.docs / "changes/unreleased"
        self.fragments.mkdir(parents=True)
        (self.docs / "changes-unreleased.md").write_text(HEAD, encoding="utf-8")
        (self.fragments / "README.md").write_text("# Fragments\n", encoding="utf-8")
        for name, value in (("ROOT", self.docs), ("DOCS", self.docs)):
            self.enterContext(patch.object(assemble_changelog, name, value))
        self.enterContext(patch.object(assemble_changelog, "FRAGMENTS", self.fragments))

    def fragment(self, name, front, body="- A change.\n"):
        (self.fragments / f"{name}.md").write_text(f"---\n{front}---\n{body}", encoding="utf-8")

    def test_page_lists_fragments_by_section_after_the_head(self):
        self.fragment("wave1-fix-b", "section: Fixes\nissues: [1151, 1152]\n", "- Fix B.\n")
        self.fragment("wave1-fix-a", "section: Fixes\nissues: []\n", "- Fix A.\n- More A.\n")
        self.fragment("wave1-dev", "section: Developer\nissues: []\n", "- Tooling.\n")
        self.fragment(
            "wave1-feature",
            "section: Changes\nissues: [7]\ncredit: [@someone]\n",
            "- A feature that\n  wraps.\n",
        )
        issues = "https://github.com/Ameyanagi/ReShiki/issues/"
        self.assertEqual(
            assemble_changelog.assemble(),
            HEAD
            + "\n## Changes\n\n"
            + f"- A feature that\n  wraps. ([#7]({issues}7); thanks to @someone)\n"
            + "\n## Fixes\n\n"
            + "- Fix A.\n- More A.\n"
            + f"- Fix B. ([#1151]({issues}1151); [#1152]({issues}1152))\n"
            + "\n## Developer\n\n- Tooling.\n",
        )

    def test_page_without_fragments_is_the_head(self):
        self.assertEqual(assemble_changelog.assemble(), HEAD)

    def test_front_matter_comments_are_ignored(self):
        self.fragment("commented", "section: Fixes        # or: Changes\nissues: [3]  # fixed\n")
        self.assertEqual(assemble_changelog.fragments()[0][1:4], ("Fixes", [3], []))

    def test_invalid_fragments_are_rejected(self):
        cases = {
            "section": "section: Features\nissues: []\n",
            "issues": "section: Fixes\n",
            "number": "section: Fixes\nissues: [one]\n",
            "unknown": "section: Fixes\nissues: []\npr: 12\n",
            "credit": "section: Fixes\nissues: []\ncredit: [someone]\n",
        }
        for name, front in cases.items():
            with self.subTest(name):
                self.fragment(name, front)
                with self.assertRaises(ValueError):
                    assemble_changelog.fragments()
                (self.fragments / f"{name}.md").unlink()
        for name, text in (
            ("prose", "---\nsection: Fixes\nissues: []\n---\nProse.\n"),
            ("bare", "- No front matter.\n"),
        ):
            with self.subTest(name):
                (self.fragments / f"{name}.md").write_text(text, encoding="utf-8")
                with self.assertRaises(ValueError):
                    assemble_changelog.fragments()
                (self.fragments / f"{name}.md").unlink()

    def test_check_reports_an_invalid_fragment(self):
        self.fragment("bad", "section: Nope\nissues: []\n")
        with (
            patch("sys.argv", ["assemble_changelog.py", "--check"]),
            contextlib.redirect_stderr(io.StringIO()) as error,
        ):
            self.assertEqual(assemble_changelog.main(), 1)
        self.assertIn("bad.md: section must be one of", error.getvalue())

    def test_page_is_written_as_utf8_whatever_the_console_encoding(self):
        self.fragment("arrow", "section: Fixes\nissues: []\n", "- A \u2192 B.\n")
        # A Windows console code page without the arrow.
        stdout = io.TextIOWrapper(io.BytesIO(), encoding="cp1252")
        with patch("sys.argv", ["assemble_changelog.py"]), patch("sys.stdout", stdout):
            self.assertEqual(assemble_changelog.main(), 0)
        stdout.flush()
        self.assertIn("- A \u2192 B.".encode(), stdout.buffer.getvalue())

    def test_release_folds_fragments_into_notes_and_contributors(self):
        (self.docs / "changes-0.16.md").write_text("# ReShiki 0.16.0\n\nIntro.\n", encoding="utf-8")
        (self.docs / "contributors.md").write_text("# Contributors\n", encoding="utf-8")
        self.fragment(
            "fix", "section: Fixes\nissues: [5]\ncredit: [@a, @b]\n", "- Fix it\n  well.\n"
        )
        self.fragment("dev", "section: Developer\nissues: []\n", "- Tooling.\n")
        with contextlib.redirect_stdout(io.StringIO()):
            assemble_changelog.release("0.16.0")
        notes = (self.docs / "changes-0.16.md").read_text(encoding="utf-8")
        self.assertTrue(notes.startswith("# ReShiki 0.16.0\n\nIntro.\n\n## Fixes\n\n- Fix it\n"))
        self.assertTrue(notes.endswith("## Developer\n\n- Tooling.\n"))
        self.assertEqual(
            (self.docs / "contributors.md").read_text(encoding="utf-8"),
            "# Contributors\n\n@a, @b — Fix it well. In [ReShiki 0.16.0](changes-0.16.md).\n",
        )
        self.assertEqual([p.name for p in self.fragments.iterdir()], ["README.md"])

    def test_release_of_a_patch_version_creates_its_notes(self):
        self.fragment("fix", "section: Fixes\nissues: []\n", "- Fix.\n")
        with contextlib.redirect_stdout(io.StringIO()):
            assemble_changelog.release("0.16.1")
        self.assertEqual(
            (self.docs / "changes-0.16.1.md").read_text(encoding="utf-8"),
            "# ReShiki 0.16.1\n\n## Fixes\n\n- Fix.\n",
        )


if __name__ == "__main__":
    unittest.main()
