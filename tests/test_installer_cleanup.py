"""Exercise Inno's deferred self-deletion without installing into a user account."""

import errno
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from installers import InstallerCheckDirectory


class CleanupTests(unittest.TestCase):
    @staticmethod
    def locked_file(code):
        error = PermissionError(errno.EACCES, "Access is denied", "Installed ReShiki/unins000.exe")
        error.winerror = code
        return error

    def test_context_exit_retries_transient_windows_locks_then_removes_directory(self):
        original_cleanup = tempfile.TemporaryDirectory.cleanup
        for code in (5, 32, 33):
            with self.subTest(winerror=code):
                directory = InstallerCheckDirectory()
                root = Path(directory.name)
                uninstaller = root / "Installed ReShiki/unins000.exe"
                uninstaller.parent.mkdir()
                uninstaller.touch()
                attempts = 0
                elapsed = 0.0
                waits = []

                def cleanup(instance):
                    nonlocal attempts
                    self.assertIs(instance, directory)
                    attempts += 1
                    if attempts <= 2:
                        raise self.locked_file(code)
                    original_cleanup(instance)

                def sleep(seconds):
                    nonlocal elapsed
                    elapsed += seconds
                    waits.append(seconds)
                    if len(waits) == 2:
                        # The clone finishes self-deletion between cleanup attempts.
                        uninstaller.unlink()

                try:
                    with (
                        patch.object(tempfile.TemporaryDirectory, "cleanup", cleanup),
                        patch("installers.time.monotonic", side_effect=lambda: elapsed),
                        patch("installers.time.sleep", side_effect=sleep),
                    ):
                        with directory as temporary:
                            self.assertEqual(Path(temporary), root)
                            self.assertTrue(uninstaller.is_file())
                    self.assertEqual(attempts, 3)
                    self.assertEqual(waits, [0.05, 0.1])
                    self.assertFalse(root.exists())
                finally:
                    original_cleanup(directory)

    def test_persistent_lock_preserves_os_error_and_stops_at_deadline(self):
        directory = InstallerCheckDirectory()
        error = self.locked_file(5)
        elapsed = 0.0
        waits = []

        def sleep(seconds):
            nonlocal elapsed
            elapsed += seconds
            waits.append(seconds)

        try:
            with (
                patch.object(tempfile.TemporaryDirectory, "cleanup", side_effect=error) as cleanup,
                patch("installers.time.monotonic", side_effect=lambda: elapsed),
                patch("installers.time.sleep", side_effect=sleep),
            ):
                with self.assertRaises(PermissionError) as caught:
                    with directory:
                        pass
            self.assertIs(caught.exception, error)
            self.assertEqual(caught.exception.winerror, 5)
            self.assertEqual(caught.exception.filename, "Installed ReShiki/unins000.exe")
            self.assertIn("30 seconds", " ".join(caught.exception.__notes__))
            self.assertAlmostEqual(elapsed, 30.0)
            self.assertLessEqual(max(waits), 0.25)
            self.assertLess(cleanup.call_count, 125)
        finally:
            directory.cleanup()

    def test_unrelated_errors_are_not_retried(self):
        for error in (OSError(errno.EIO, "I/O failure"), self.locked_file(87)):
            with self.subTest(error=error):
                directory = InstallerCheckDirectory()
                try:
                    with (
                        patch.object(
                            tempfile.TemporaryDirectory, "cleanup", side_effect=error
                        ) as cleanup,
                        patch("installers.time.sleep") as sleep,
                    ):
                        with self.assertRaises(OSError) as caught:
                            directory.cleanup()
                    self.assertIs(caught.exception, error)
                    self.assertEqual(cleanup.call_count, 1)
                    sleep.assert_not_called()
                finally:
                    directory.cleanup()

    def test_successful_cleanup_does_not_delay_or_suppress_verification_failure(self):
        directory = InstallerCheckDirectory()
        verification_error = ValueError("Uninstaller modified user data")
        with patch("installers.time.sleep") as sleep:
            with self.assertRaises(ValueError) as caught:
                with directory:
                    raise verification_error
        self.assertIs(caught.exception, verification_error)
        self.assertFalse(Path(directory.name).exists())
        sleep.assert_not_called()

    def test_persistent_cleanup_failure_retains_the_original_verification_error(self):
        directory = InstallerCheckDirectory()
        verification_error = ValueError("Uninstaller left the application executable behind")
        cleanup_error = self.locked_file(5)
        try:
            with (
                patch.object(tempfile.TemporaryDirectory, "cleanup", side_effect=cleanup_error),
                patch("installers.time.monotonic", side_effect=[0.0, 30.0]),
                patch("installers.time.sleep") as sleep,
            ):
                with self.assertRaises(PermissionError) as caught:
                    with directory:
                        raise verification_error
            self.assertIs(caught.exception, cleanup_error)
            self.assertIs(caught.exception.__context__, verification_error)
            self.assertFalse(caught.exception.__suppress_context__)
            sleep.assert_not_called()
        finally:
            directory.cleanup()


if __name__ == "__main__":
    unittest.main()
