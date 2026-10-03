"""Copy completion UI tests; no desktop, subprocess, or clipboard is accessed."""

import importlib.util
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

SOURCE = Path(__file__).parents[1] / "extension" / "reshiki.py"
SPEC = importlib.util.spec_from_file_location("reshiki_copy_extension", SOURCE)
extension = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(extension)


class Controller:
    def __init__(self):
        self.present = False
        self.messages = []

    def hasInfobar(self, name):
        return self.present

    def appendInfobar(self, name, title, detail, kind, buttons, close):
        assert not self.present
        self.present = True
        self.messages.append((title, detail, kind))

    def updateInfobar(self, name, title, detail, kind):
        assert self.present
        self.messages.append((title, detail, kind))


class CopyFeedbackTests(unittest.TestCase):
    def setUp(self):
        self.controller = Controller()
        self.handler = extension.Handler(None)
        self.handler.frame = SimpleNamespace(getController=lambda: self.controller)
        self.jobs, self.callbacks = [], []

        def thread(*, target, daemon):
            self.assertTrue(daemon)
            return SimpleNamespace(start=lambda: self.jobs.append(target))

        self.enterContext(patch.object(extension.threading, "Thread", side_effect=thread))
        self.enterContext(
            patch.object(
                extension, "post", side_effect=lambda ctx, action: self.callbacks.append(action)
            )
        )
        self.errors = self.enterContext(patch.object(extension, "show_error"))

    def test_ready_is_posted_only_after_worker_success_and_duplicate_copy_waits(self):
        def worker(program, mode, raw):
            self.assertEqual((program, mode, raw), ("editor", "--libreoffice-copy", b"native"))
            self.assertEqual(len(self.controller.messages), 1)
            self.assertTrue(self.handler.copying)
            self.assertFalse(self.callbacks)

        with patch.object(extension, "worker", side_effect=worker) as work:
            self.handler.copy("editor", b"native")
            self.assertEqual(self.controller.messages[0][0], "Copying editable drawing…")
            self.assertTrue(self.handler.copying)
            self.assertFalse(self.callbacks)
            self.handler.copy("editor", b"other drawing")
            self.assertEqual(len(self.jobs), 1)
            self.jobs.pop()()
            work.assert_called_once()
            self.assertEqual(len(self.controller.messages), 1)
            self.assertTrue(self.handler.copying)
            self.callbacks.pop()()
        self.assertFalse(self.handler.copying)
        self.assertEqual(
            self.controller.messages[-1],
            ("Editable drawing copied", "Ready to paste in ReShiki.", extension.INFO),
        )
        self.errors.assert_not_called()

    def test_menu_dispatch_captures_selected_native_before_background_copy(self):
        embedded = extension.Embedded(None, (b"selected native", b"PNG", (100, 200)))
        self.controller.getModel = lambda: "original document"
        with (
            patch.object(extension, "selected", return_value=embedded) as selected,
            patch.object(extension, "executable", return_value="editor"),
            patch.object(extension, "worker") as work,
        ):
            self.handler.dispatch(SimpleNamespace(Path="copy"), ())
            selected.assert_called_once_with("original document")
            embedded.native = b"subsequent edit"
            work.assert_not_called()
            self.jobs.pop()()
            work.assert_called_once_with("editor", "--libreoffice-copy", b"selected native")
            self.callbacks.pop()()
        self.assertEqual(self.controller.messages[-1][0], "Editable drawing copied")

    def test_failure_posts_existing_error_without_claiming_readiness_and_allows_retry(self):
        with patch.object(extension, "worker", side_effect=RuntimeError("clipboard denied")):
            self.handler.copy("editor", b"native")
            self.jobs.pop()()
            self.errors.assert_not_called()
            self.assertEqual(len(self.controller.messages), 1)
            self.callbacks.pop()()
        self.assertFalse(self.handler.copying)
        self.assertEqual(self.controller.messages[-1][0], "Drawing was not copied")
        self.assertEqual(self.controller.messages[-1][2], extension.DANGER)
        self.assertFalse(
            any("Ready to paste" in detail for _, detail, _ in self.controller.messages)
        )
        self.errors.assert_called_once_with(None, "clipboard denied")
        self.handler.copy("editor", b"retry")
        self.assertEqual(len(self.jobs), 1)

    def test_thread_start_failure_clears_copying_and_uses_existing_error(self):
        with patch.object(extension.threading, "Thread", side_effect=RuntimeError("no thread")):
            self.handler.copy("editor", b"native")
        self.assertFalse(self.handler.copying)
        self.assertEqual(self.controller.messages[-1][0], "Drawing was not copied")
        self.errors.assert_called_once_with(None, "no thread")

    def test_infobar_closed_during_copy_is_recreated_on_completion(self):
        with patch.object(extension, "worker"):
            self.handler.copy("editor", b"native")
            self.controller.present = False
            self.jobs.pop()()
            self.callbacks.pop()()
        self.assertTrue(self.controller.present)
        self.assertEqual(self.controller.messages[-1][0], "Editable drawing copied")

    def test_completion_uses_original_controller_after_frame_changes(self):
        with patch.object(extension, "worker"):
            self.handler.copy("editor", b"native")
            replacement = Controller()
            self.handler.frame.getController = lambda: replacement
            self.jobs.pop()()
            self.callbacks.pop()()
        self.assertEqual(self.controller.messages[-1][0], "Editable drawing copied")
        self.assertFalse(replacement.messages)

    def test_missing_infobar_uses_status_then_ends_it_on_callback_thread(self):
        indicator = Mock()
        self.handler.frame = SimpleNamespace(
            getController=lambda: object(), createStatusIndicator=lambda: indicator
        )
        with patch.object(extension, "worker"), patch.object(extension.threading, "Timer") as timer:
            self.handler.copy("editor", b"native")
            indicator.start.assert_called_once_with(
                "Copying editable drawing… Wait for confirmation before pasting.", 1
            )
            self.jobs.pop()()
            self.callbacks.pop()()
            indicator.setText.assert_called_once_with(
                "Editable drawing copied Ready to paste in ReShiki."
            )
            indicator.end.assert_not_called()
            timer.assert_called_once()
            delay, clear = timer.call_args.args
            self.assertEqual(delay, 4)
            self.assertTrue(timer.return_value.daemon)
            timer.return_value.start.assert_called_once()
            clear()
            indicator.end.assert_not_called()
            self.callbacks.pop()()
            indicator.end.assert_called_once()
        self.errors.assert_not_called()

    def test_unavailable_feedback_does_not_fail_clipboard_success(self):
        self.handler.frame = SimpleNamespace(getController=lambda: object())
        with patch.object(extension, "worker") as work:
            self.handler.copy("editor", b"native")
            self.jobs.pop()()
            self.callbacks.pop()()
        work.assert_called_once_with("editor", "--libreoffice-copy", b"native")
        self.assertFalse(self.handler.copying)
        self.errors.assert_not_called()

    def test_disposed_feedback_does_not_hide_clipboard_failure(self):
        with patch.object(extension, "worker", side_effect=RuntimeError("write failed")):
            self.handler.copy("editor", b"native")
            self.controller.updateInfobar = Mock(side_effect=RuntimeError("disposed"))
            self.jobs.pop()()
            self.callbacks.pop()()
        self.assertFalse(self.handler.copying)
        self.errors.assert_called_once_with(None, "write failed")


if __name__ == "__main__":
    unittest.main()
