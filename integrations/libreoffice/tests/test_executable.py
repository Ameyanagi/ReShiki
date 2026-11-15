"""Which ReShiki executable the extension starts, without a running desktop.

Run with LibreOffice's UNO Python, alongside test_session.py.
"""

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

SOURCE = Path(__file__).parents[1] / "extension" / "reshiki.py"
SPEC = importlib.util.spec_from_file_location("reshiki_executable_extension", SOURCE)
extension = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(extension)


class ExecutableTests(unittest.TestCase):
    def test_the_chosen_app_then_the_installed_app_come_before_path(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            installed, on_path, chosen = (
                root / name for name in ("installed", "on-path", "chosen")
            )
            for program in (installed, on_path, chosen):
                program.write_bytes(b"")
            settings = root / "reshiki-integration.json"
            with (
                patch.object(extension, "settings_path", return_value=settings),
                patch.object(extension, "installed_executables", return_value=[str(installed)]),
                patch.object(extension.shutil, "which", return_value=str(on_path)),
            ):
                # A program named reshiki earlier on PATH does not replace the
                # installed application.
                self.assertEqual(extension.executable(None), str(installed.resolve()))
                installed.unlink()
                self.assertEqual(extension.executable(None), str(on_path.resolve()))
                settings.write_text(json.dumps({"executable": str(chosen)}), encoding="utf-8")
                self.assertEqual(extension.executable(None), str(chosen))
                chosen.unlink()
                with self.assertRaisesRegex(RuntimeError, "has moved"):
                    extension.executable(None)


if __name__ == "__main__":
    unittest.main()
