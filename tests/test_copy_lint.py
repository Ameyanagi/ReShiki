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

    def test_measures_status_without_placeholders(self):
        fits = "x" * 60
        self.assertEqual(problems(f'self.status = format!("{fits}{{name}}");'), [])
        self.assertEqual(len(problems(f'self.status =\n    "{fits}x".into();')), 1)

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
