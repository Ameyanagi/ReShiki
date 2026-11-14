"""A pull request's job summary lists the fixture rows it changes."""

import gzip
import subprocess
import tempfile
import unittest
from pathlib import Path

from scripts.baseline_summary import ROW_LIMIT, clip, quoted, row_changes, summary

BASELINE = Path(__file__).parent / "fixtures/s2n/baseline.tsv"


def git(root, *arguments):
    return subprocess.run(
        ["git", "-C", str(root), *arguments], check=True, capture_output=True, text=True
    ).stdout.strip()


class BaselineSummaryTests(unittest.TestCase):
    def test_keyed_rows_report_old_and_new_values_and_other_rows_as_added_or_removed(self):
        old = "smiles\tname\nCC\tethane\nCCC\tpropane\nC\tmethane\n"
        new = "smiles\tname\nCC\tEthane\nC\tmethane\nCCCC\tbutane\n"
        self.assertEqual(
            row_changes(old, new, keyed=True),
            ["- CC\tethane", "+ CC\tEthane", "- CCC\tpropane", "+ CCCC\tbutane"],
        )
        # Without a key column, a repeated line counts once for each copy.
        self.assertEqual(
            row_changes("a\nb\nb\n", "b\nc\n", keyed=False),
            ["- a", "- b", "+ c"],
        )
        self.assertEqual(row_changes("a\nb\n", "b\na\n", keyed=False), [])

    def test_rows_that_share_a_first_column_are_matched_by_as_many_columns_as_tell_them_apart(
        self,
    ):
        # The naming baseline has four columns and is keyed on the first two.
        old = BASELINE.read_text(encoding="utf-8")
        row = "generated\tacid\t21\t21\n"
        self.assertIn(row, old)
        self.assertEqual(
            row_changes(old, old.replace(row, "generated\tacid\t20\t21\n"), keyed=True),
            ["- generated\tacid\t21\t21", "+ generated\tacid\t20\t21"],
        )
        self.assertEqual(
            row_changes(old, old.replace(row, ""), keyed=True), ["- generated\tacid\t21\t21"]
        )
        self.assertEqual(
            row_changes(old.replace(row, ""), old, keyed=True), ["+ generated\tacid\t21\t21"]
        )
        # A file with a row that appears twice has no key: its rows are counted.
        self.assertEqual(
            row_changes("a\t1\na\t1\nb\t2\n", "a\t1\nb\t3\n", keyed=True),
            ["- a\t1", "- b\t2", "+ b\t3"],
        )

    def test_summary_covers_text_gzip_binary_added_and_removed_fixtures_only(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fixtures = root / "tests/fixtures"
            (fixtures / "s2n").mkdir(parents=True)
            (root / "src").mkdir()
            git(root, "init", "-q")
            git(root, "config", "user.email", "test@example.invalid")
            git(root, "config", "user.name", "Test")
            (fixtures / "s2n/baseline.tsv").write_text(
                "check\tcorrect\nacid\t21\nalcohol\t20\n", encoding="utf-8"
            )
            (fixtures / "capture.json.gz").write_bytes(gzip.compress(b'{"a": 1}\n{"b": 2}\n'))
            (fixtures / "drawing.cdx").write_bytes(b"\x00\x01binary")
            (fixtures / "removed.txt").write_text("gone\n", encoding="utf-8")
            (fixtures / "long.txt").write_text("", encoding="utf-8")
            (root / "src/lib.rs").write_text("// old\n", encoding="utf-8")
            git(root, "add", ".")
            git(root, "commit", "-q", "-m", "base")
            base = git(root, "rev-parse", "HEAD")

            (fixtures / "s2n/baseline.tsv").write_text(
                "check\tcorrect\nacid\t21\nalcohol\t21\n", encoding="utf-8"
            )
            (fixtures / "capture.json.gz").write_bytes(gzip.compress(b'{"a": 1}\n{"b": 3}\n'))
            (fixtures / "drawing.cdx").write_bytes(b"\x00\x01binary, longer")
            (fixtures / "removed.txt").unlink()
            (fixtures / "added.tsv").write_text("key\tvalue\nx\t1\n", encoding="utf-8")
            (fixtures / "long.txt").write_text(
                "".join(f"row {index}\n" for index in range(ROW_LIMIT + 5)), encoding="utf-8"
            )
            (root / "src/lib.rs").write_text("// new\n", encoding="utf-8")
            git(root, "add", "-A")
            git(root, "commit", "-q", "-m", "head")

            text = summary(base, "HEAD", root)
            self.assertEqual(summary("HEAD", "HEAD", root), "")

        self.assertIn("6 changed fixture files", text)
        self.assertNotIn("src/lib.rs", text)
        self.assertIn("`tests/fixtures/s2n/baseline.tsv`: 1 removed, 1 added", text)
        self.assertIn("- alcohol\t20\n+ alcohol\t21\n", text)
        self.assertIn("`tests/fixtures/capture.json.gz`: 1 removed, 1 added", text)
        self.assertIn('- {"b": 2}\n+ {"b": 3}\n', text)
        self.assertIn("`tests/fixtures/drawing.cdx`: binary, 8 to 16 bytes", text)
        self.assertIn("`tests/fixtures/removed.txt`: 1 removed, 0 added", text)
        self.assertIn("`tests/fixtures/added.tsv`: 0 removed, 2 added", text)
        self.assertIn(f"`tests/fixtures/long.txt`: 0 removed, {ROW_LIMIT + 5} added", text)
        self.assertIn(f"+ row {ROW_LIMIT - 1}\n", text)
        self.assertNotIn(f"+ row {ROW_LIMIT}\n", text)
        self.assertIn("5 more rows", text)

    def test_a_path_cannot_add_its_own_markdown_to_the_summary(self):
        self.assertEqual(quoted("tests/fixtures/a.tsv"), "`tests/fixtures/a.tsv`")
        self.assertEqual(quoted("a`\n# Fake\x1b\udc80"), "`` a`\\n# Fake\\x1b\\udc80 ``")
        self.assertEqual(quoted("``a```"), "```` ``a``` ````")

    def test_a_long_summary_is_cut_by_bytes_at_a_line_end_and_its_code_block_is_closed(self):
        text = "- `a`: 0 removed, 3 added\n\n```diff\n+ \u00e9\u00e9\u00e9\n+ \u65e5\u672c\u8a9e\n+ c\n```\n"
        self.assertEqual(clip(text, len(text.encode("utf-8"))), text)
        # The limit falls inside the second row, between the bytes of one character.
        limit = len(text[: text.index("\u672c")].encode("utf-8")) + 1
        clipped = clip(text, limit)
        self.assertEqual(
            clipped,
            "- `a`: 0 removed, 3 added\n\n```diff\n+ \u00e9\u00e9\u00e9\n```\n"
            "\nThe rest of the list is too long to show.\n",
        )
        # A cut between two files leaves no block open.
        self.assertEqual(
            clip(text + text, len(text.encode("utf-8")) + 5),
            text + "\nThe rest of the list is too long to show.\n",
        )


if __name__ == "__main__":
    unittest.main()
