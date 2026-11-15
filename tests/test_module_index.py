"""docs/modules.md lists every module file with the summary its source gives."""

import tempfile
import unittest
from pathlib import Path

from scripts.module_index import PAGE, page, summary


class ModuleIndexTests(unittest.TestCase):
    def test_the_committed_page_matches_the_source_tree(self):
        self.assertEqual(
            PAGE.read_text(encoding="utf-8"), page(), "run python3 scripts/module_index.py"
        )

    def test_summary_is_the_first_sentence_as_plain_markdown(self):
        cases = {
            "//! Undo and redo. More text.\nuse x;\n": "Undo and redo.",
            "#![forbid(unsafe_code)]\n//! Wrapped over\n//! two lines.\n//! Second.\n": (
                "Wrapped over two lines."
            ),
            "//! Serves [`ToolHost`](a::b) for Cp* and Vec<u8>, not `Vec<u8>`\n": (
                "Serves `ToolHost` for Cp\\* and Vec\\<u8>, not `Vec<u8>`"
            ),
            "use x;\n//! Not a leading comment.\n": "",
        }
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "module.rs"
            for source, expected in cases.items():
                path.write_text(source, encoding="utf-8")
                self.assertEqual(summary(path), expected)


if __name__ == "__main__":
    unittest.main()
