import unittest
from pathlib import Path

from scripts import copy_lint

APP = Path("src/app/example.rs")


def problems(text: str, path: Path = APP) -> list[str]:
    return [message for _, message in copy_lint.problems(path, text)]


class CopyLintTest(unittest.TestCase):
    def test_accepts_plain_app_text(self):
        self.assertEqual(
            problems(
                'self.status = "3D projection applied".into();\n'
                'section("Atom labels");\n'
                'command("Align center", Message::Fit);\n'
            ),
            [],
        )

    def test_reports_each_rule_with_its_line(self):
        found = list(
            copy_lint.problems(
                APP,
                'let a = "Centre charge in ring";\n'
                'let b = format!("Imported {count} template(s)");\n'
                'let c = "Drag · Escape cancels";\n'
                'self.status = "Cleanup applied · Undo restores the original layout".into();\n'
                'section("ATOM LABELS");\n',
            )
        )
        self.assertEqual([line for line, _ in found], [1, 2, 3, 4, 5])
        self.assertIn("US English", found[0][1])
        self.assertIn("real plural", found[1][1])
        self.assertIn("Esc", found[2][1])
        self.assertIn("Undo", found[3][1])
        self.assertIn("sentence case", found[4][1])

    def test_counts_each_placeholder_as_a_short_word(self):
        self.assertEqual(problems(f'self.status = format!("{"x" * 55}{{name}}");'), [])
        self.assertEqual(len(problems(f'self.status = format!("{"x" * 56}{{name}}");')), 1)
        self.assertEqual(len(problems(f'self.status =\n    "{"x" * 61}".into();')), 1)
        # Only the first line shows in the status bar.
        self.assertEqual(problems(f'self.status = format!("Copied\\n{"x" * 70}");'), [])

    def test_measures_constant_and_conditional_statuses(self):
        long = "x" * 61
        found = problems(
            f'const LONG: &str =\n    "{long}";\n'
            "app.status = LONG.into();\n"
            f'self.status = if ok {{ "Done".into() }} else {{ "{long}".into() }};\n'
            f'self.status = match n {{ 0 => return, n => format!("{{n}}{long}") }};\n'
        )
        self.assertEqual(len(found), 3)
        shared = list(copy_lint.problems(APP, "self.status = OTHER.into();", {"OTHER": long}))
        self.assertEqual([line for line, _ in shared], [1])

    def test_joins_concat_constants(self):
        found = problems(
            "const LONG: &str = concat!(\n"
            f'    "{"x" * 40}",\n'
            f'    "{"y" * 21}\\n",\n'
            '    "Tooltip text",\n'
            ");\n"
            "self.status = LONG.into();\n"
        )
        self.assertEqual(len(found), 1)
        self.assertIn("(61)", found[0])

    def test_reads_multiline_raw_and_char_literals(self):
        found = list(
            copy_lint.problems(
                APP,
                "let quote = '\"';\n"
                'let tip = "R/S: stereocenter.\\n\\\n    r/s: pseudoasymmetric centre.";\n'
                'let raw = r#"Colour "inside" raw"#;\n'
                'let groups = "abs: as drawn.\\nCentres flip together.";\n',
            )
        )
        self.assertEqual([line for line, _ in found], [2, 4, 5])
        self.assertIn("centre", found[0][1])
        self.assertIn("Colour", found[1][1])
        self.assertIn("Centres", found[2][1])

    def test_accepts_us_words_that_share_a_stem(self):
        self.assertEqual(
            problems(
                'self.status = "Cancellation requested".into();\n'
                'let a = "Analysis ready · 3 analyses";\n'
            ),
            [],
        )
        self.assertEqual(len(problems('let a = "Cancelled";\nlet b = "Cancelling";')), 2)

    def test_skips_comments_identifiers_and_test_code(self):
        self.assertEqual(
            problems(
                '// "Centre" in a comment\n'
                "Cleanup::Symmetrise { upright }\n"
                '#[cfg(test)]\nmod tests {\n    let s = "Undo restores it";\n}\n'
            ),
            [],
        )
        self.assertTrue(copy_lint.is_test_file(Path("src/app/context_menu/tests.rs")))
        self.assertTrue(copy_lint.is_test_file(Path("src/app/workspace/model_face_qa.rs")))
        self.assertFalse(copy_lint.is_test_file(Path("src/app/context_menu.rs")))


if __name__ == "__main__":
    unittest.main()
