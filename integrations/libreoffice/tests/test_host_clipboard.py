"""Linux host clipboard contracts, without accessing a desktop or clipboard.

The fake host queues publication before the next UNO callback, like GTK VCL.
Its readback proves only retained host data. Actual Wayland compositor and
external-consumer behavior requires separate native desktop acceptance.
"""

import base64
import importlib.util
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

import uno

SOURCE = Path(__file__).parents[1] / "extension" / "reshiki.py"
SPEC = importlib.util.spec_from_file_location("reshiki_host_clipboard_extension", SOURCE)
extension = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(extension)

NATIVE = b'{"version":18,"atoms":[],"bonds":[]}'
OTHER = b'{"version":18,"atoms":[],"bonds":[],"annotations":[]}'
PNG = base64.b64decode(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
)


def packet(native=NATIVE, png=PNG):
    return {
        "version": 1,
        "native": base64.b64encode(native).decode("ascii"),
        "png": base64.b64encode(png).decode("ascii"),
        "extent": [100, 200],
    }


class Offered:
    def __init__(self, values):
        self.values = values
        self.requests = []

    def getTransferDataFlavors(self):
        return tuple(extension.flavor(mime) for mime in self.values)

    def getTransferData(self, requested):
        self.requests.append(requested.MimeType)
        return self.values[requested.MimeType]


class Controller:
    def __init__(self, document):
        self.document = document
        self.messages = []

    def getModel(self):
        return self.document

    def hasInfobar(self, name):
        return bool(self.messages)

    def appendInfobar(self, name, title, detail, kind, buttons, close):
        self.messages.append((title, detail, kind))

    def updateInfobar(self, name, title, detail, kind):
        self.messages.append((title, detail, kind))


class TransferableTests(unittest.TestCase):
    def test_only_linux_uses_host_clipboard(self):
        for platform, expected in (("linux", True), ("darwin", False), ("win32", False)):
            with self.subTest(platform=platform), patch.object(extension.sys, "platform", platform):
                self.assertEqual(extension.uses_host_clipboard(), expected)

    def test_offer_is_immutable_and_rejects_wrong_flavor_or_type(self):
        native, png = bytearray(NATIVE), bytearray(PNG)
        drawing = extension.ClipboardDrawing(native, png)
        native[:] = OTHER
        png[:] = b"later picture"
        self.assertEqual(
            drawing.getTransferData(extension.flavor(extension.NATIVE_MIME)).value, NATIVE
        )
        self.assertEqual(drawing.getTransferData(extension.flavor("image/png")).value, PNG)
        self.assertEqual(
            [value.MimeType for value in drawing.getTransferDataFlavors()],
            [extension.NATIVE_MIME, "image/png"],
        )
        unsupported = extension.flavor(extension.NATIVE_MIME)
        unsupported.DataType = uno.getTypeByName("string")
        for requested in (unsupported, extension.flavor("text/plain")):
            self.assertFalse(drawing.isDataFlavorSupported(requested))
            with self.assertRaises(extension.UnsupportedFlavorException):
                drawing.getTransferData(requested)

    def test_native_read_prefers_canonical_bytes_and_does_not_fetch_preview(self):
        offered = Offered(
            {
                "image/png": uno.ByteSequence(PNG),
                "application/x-moruno-drawing+json": uno.ByteSequence(OTHER),
                extension.NATIVE_MIME: uno.ByteSequence(NATIVE),
            }
        )
        clipboard = SimpleNamespace(getContents=lambda: offered)
        with patch.object(extension, "service", return_value=clipboard):
            self.assertEqual(extension.read_host_native(None), NATIVE)
        self.assertEqual(offered.requests, [extension.NATIVE_MIME])

    def test_native_read_accepts_exposed_legacy_mime(self):
        offered = Offered({"application/x-moruno-drawing+json": uno.ByteSequence(NATIVE)})
        with patch.object(
            extension, "service", return_value=SimpleNamespace(getContents=lambda: offered)
        ):
            self.assertEqual(extension.read_host_native(None), NATIVE)

    def test_empty_image_only_wrong_type_and_oversized_data_are_rejected(self):
        wrong_type = Offered({extension.NATIVE_MIME: uno.ByteSequence(NATIVE)})
        wrong = extension.flavor(extension.NATIVE_MIME)
        wrong.DataType = uno.getTypeByName("string")
        wrong_type.getTransferDataFlavors = lambda: (wrong,)
        cases = (
            None,
            Offered({"image/png": uno.ByteSequence(PNG)}),
            Offered({extension.NATIVE_MIME: uno.ByteSequence(b"")}),
            Offered({extension.NATIVE_MIME: "not binary"}),
            Offered({extension.NATIVE_MIME: uno.ByteSequence(NATIVE + b" ")}),
            wrong_type,
        )
        with patch.object(extension, "LIMIT", len(NATIVE)):
            for contents in cases:
                with (
                    self.subTest(contents=contents),
                    patch.object(
                        extension,
                        "service",
                        return_value=SimpleNamespace(getContents=lambda: contents),
                    ),
                ):
                    with self.assertRaises(ValueError):
                        extension.read_host_native(None)
        self.assertFalse(wrong_type.requests)

    def test_offer_enforces_combined_budget_before_publication(self):
        with patch.object(extension, "LIMIT", len(NATIVE) + len(PNG)):
            extension.ClipboardDrawing(NATIVE, PNG)
        with patch.object(extension, "LIMIT", len(NATIVE) + len(PNG) - 1):
            with self.assertRaisesRegex(ValueError, "Combined"):
                extension.ClipboardDrawing(NATIVE, PNG)


class HostClipboardTests(unittest.TestCase):
    def setUp(self):
        self.enterContext(patch.object(extension, "uses_host_clipboard", return_value=True))
        self.document, self.window = object(), object()
        self.active = self.window
        self.controller = Controller(self.document)
        self.frame = SimpleNamespace(
            getController=lambda: self.controller, getContainerWindow=lambda: self.window
        )
        self.handler = extension.Handler(None)
        self.handler.frame = self.frame
        self.old = Offered({extension.NATIVE_MIME: uno.ByteSequence(OTHER)})
        self.contents, self.published = self.old, self.old
        self.writes, self.jobs, self.callbacks = [], [], []
        self.in_worker = False

        def contents():
            self.assertFalse(self.in_worker, "A helper thread must not read the host clipboard")
            return self.contents

        def publish(value, owner):
            self.assertFalse(self.in_worker, "A helper thread must not publish the host clipboard")
            self.assertIsNone(owner)
            self.writes.append(value)
            self.contents = value
            # GTK VCL's setContents retains data now and queues OS publication.
            self.callbacks.append(lambda: setattr(self, "published", value))

        self.clipboard = SimpleNamespace(
            getContents=contents, setContents=Mock(side_effect=publish)
        )

        def service(ctx, name):
            self.assertFalse(self.in_worker, "Background work must not access host services")
            if name == "com.sun.star.datatransfer.clipboard.SystemClipboard":
                return self.clipboard
            if name == "com.sun.star.awt.Toolkit":
                return SimpleNamespace(getActiveTopWindow=lambda: self.active)
            raise AssertionError("Unexpected host service: " + name)

        self.enterContext(patch.object(extension, "service", side_effect=service))
        self.enterContext(
            patch.object(
                extension, "post", side_effect=lambda ctx, callback: self.callbacks.append(callback)
            )
        )
        self.enterContext(
            patch.object(
                extension.threading,
                "Thread",
                side_effect=lambda *, target, daemon: SimpleNamespace(
                    start=lambda: self.jobs.append(target)
                ),
            )
        )
        self.errors = self.enterContext(patch.object(extension, "show_error"))
        self.worker = self.enterContext(patch.object(extension, "worker", return_value=packet()))
        self.enterContext(patch.object(extension, "executable", return_value="editor"))
        self.insert = self.enterContext(patch.object(extension, "insert"))

    def run_worker(self):
        self.in_worker = True
        try:
            self.jobs.pop(0)()
        finally:
            self.in_worker = False

    def callback(self):
        self.callbacks.pop(0)()

    def assert_no_ready(self):
        self.assertFalse(
            any("Ready to paste" in message[1] for message in self.controller.messages)
        )

    def test_copy_waits_for_host_publication_and_later_retention_check(self):
        self.handler.copy("editor", NATIVE)
        self.handler.copy("editor", OTHER)
        self.assertEqual(len(self.jobs), 1)
        self.assertIs(self.contents, self.old)
        self.run_worker()
        self.worker.assert_called_once_with("editor", "--libreoffice-preview", NATIVE)
        self.assertFalse(self.writes)
        self.assert_no_ready()
        self.callback()
        self.assertEqual(len(self.writes), 1)
        self.assertIs(self.published, self.old)
        self.assertTrue(self.handler.copying)
        self.assert_no_ready()
        self.handler.copy("editor", OTHER)
        self.assertFalse(self.jobs)
        self.callback()  # Host OS-publication event precedes the verification callback.
        self.assertIs(self.published, self.writes[0])
        self.assert_no_ready()
        self.callback()
        self.assertEqual(self.controller.messages[-1][0], "Editable drawing copied")
        self.assertFalse(self.handler.copying)
        self.errors.assert_not_called()
        self.assertEqual(
            self.published.getTransferData(extension.flavor(extension.NATIVE_MIME)).value, NATIVE
        )
        self.assertEqual(self.published.getTransferData(extension.flavor("image/png")).value, PNG)

    def test_copy_freezes_selected_bytes_and_original_frame_before_background_work(self):
        embedded = extension.Embedded(None, (NATIVE, PNG, (100, 200)))
        with patch.object(extension, "selected", return_value=embedded):
            self.handler.dispatch(SimpleNamespace(Path="copy"), ())
        embedded.native = OTHER
        self.handler.frame = SimpleNamespace(getContainerWindow=lambda: object())
        self.run_worker()
        self.worker.assert_called_once_with("editor", "--libreoffice-preview", NATIVE)
        while self.callbacks:
            self.callback()
        self.assertEqual(self.controller.messages[-1][0], "Editable drawing copied")
        self.assertEqual(
            self.published.getTransferData(extension.flavor(extension.NATIVE_MIME)).value, NATIVE
        )

    def test_preparation_error_or_changed_native_leaves_previous_clipboard_untouched(self):
        for value in (RuntimeError("renderer failed"), packet(OTHER), {"version": 2}):
            with self.subTest(value=value):
                self.worker.side_effect = value if isinstance(value, Exception) else None
                self.worker.return_value = value
                self.handler.copy("editor", NATIVE)
                self.run_worker()
                self.callback()
                self.assertFalse(self.writes)
                self.assertIs(self.contents, self.old)
                self.assertFalse(self.handler.copying)
                self.assert_no_ready()

    def test_combined_limit_failure_does_not_replace_previous_clipboard(self):
        with patch.object(extension, "LIMIT", len(NATIVE) + len(PNG) - 1):
            self.handler.copy("editor", NATIVE)
            self.run_worker()
            self.callback()
        self.assertFalse(self.writes)
        self.assertIs(self.contents, self.old)
        self.assert_no_ready()
        self.assertIn("Combined", self.errors.call_args.args[1])

    def test_missing_or_other_active_window_refuses_publication(self):
        for active in (None, object()):
            with self.subTest(active=active):
                self.handler.copy("editor", NATIVE)
                self.run_worker()
                self.active = active
                self.callback()
                self.assertFalse(self.writes)
                self.assertIs(self.contents, self.old)
                self.assertFalse(self.handler.copying)
                self.assert_no_ready()
                self.assertIn("copy again", self.errors.call_args.args[1])

    def test_disposed_original_window_does_not_publish(self):
        self.handler.copy("editor", NATIVE)
        self.run_worker()
        self.frame.getContainerWindow = Mock(side_effect=RuntimeError("disposed"))
        self.callback()
        self.assertFalse(self.writes)
        self.assertIs(self.contents, self.old)
        self.assert_no_ready()
        self.errors.assert_called_once_with(None, "disposed")

    def test_publication_exception_never_reports_ready(self):
        self.clipboard.setContents.side_effect = RuntimeError("host refused publication")
        self.handler.copy("editor", NATIVE)
        self.run_worker()
        self.callback()
        self.assert_no_ready()
        self.assertFalse(self.handler.copying)
        self.assertIs(self.contents, self.old)
        self.errors.assert_called_once_with(None, "host refused publication")

    def test_replaced_clipboard_during_publication_is_not_success(self):
        self.handler.copy("editor", NATIVE)
        self.run_worker()
        self.callback()
        self.callback()
        self.contents = self.old
        self.callback()
        self.assert_no_ready()
        self.assertFalse(self.handler.copying)
        self.assertIn("clipboard changed", self.errors.call_args.args[1])

    def test_preview_mismatch_or_readback_failure_is_not_success(self):
        for replacement in (
            Offered(
                {
                    extension.NATIVE_MIME: uno.ByteSequence(NATIVE),
                    "image/png": uno.ByteSequence(b"wrong"),
                }
            ),
            Offered({"image/png": uno.ByteSequence(PNG)}),
            None,
        ):
            with self.subTest(replacement=replacement):
                self.handler.copy("editor", NATIVE)
                self.run_worker()
                self.callback()
                self.callback()
                self.contents = replacement
                self.callback()
                self.assert_no_ready()
                self.assertFalse(self.handler.copying)

    def test_paste_captures_clipboard_and_original_document_on_dispatch_thread(self):
        self.contents = Offered({extension.NATIVE_MIME: uno.ByteSequence(NATIVE)})
        self.handler.dispatch(SimpleNamespace(Path="paste"), ())
        self.contents = Offered({extension.NATIVE_MIME: uno.ByteSequence(OTHER)})
        self.controller.document = object()
        self.run_worker()
        self.worker.assert_called_once_with("editor", "--libreoffice-preview", NATIVE)
        self.insert.assert_not_called()
        self.callback()
        self.insert.assert_called_once_with(None, self.document, packet())
        self.errors.assert_not_called()
        self.assertFalse(self.writes)

    def test_image_only_clipboard_does_not_start_worker_or_insert(self):
        self.contents = Offered({"image/png": uno.ByteSequence(PNG)})
        self.handler.dispatch(SimpleNamespace(Path="paste"), ())
        self.assertFalse(self.jobs)
        self.worker.assert_not_called()
        self.insert.assert_not_called()
        self.errors.assert_called_once()
        self.assertFalse(self.writes)

    def test_paste_validation_failure_never_inserts_an_object(self):
        self.contents = Offered({extension.NATIVE_MIME: uno.ByteSequence(NATIVE)})
        for value in (RuntimeError("invalid native document"), packet(OTHER), {"version": 2}):
            with self.subTest(value=value):
                self.worker.side_effect = value if isinstance(value, Exception) else None
                self.worker.return_value = value
                self.handler.dispatch(SimpleNamespace(Path="paste"), ())
                self.run_worker()
                self.callback()
                self.insert.assert_not_called()
                self.assertFalse(self.writes)

    def test_non_linux_paste_retains_native_clipboard_worker_route(self):
        with patch.object(extension, "uses_host_clipboard", return_value=False):
            self.handler.dispatch(SimpleNamespace(Path="paste"), ())
            self.run_worker()
            self.callback()
        self.worker.assert_called_once_with("editor", "--libreoffice-clipboard")
        self.insert.assert_called_once_with(None, self.document, packet())
        self.assertFalse(self.writes)


if __name__ == "__main__":
    unittest.main()
