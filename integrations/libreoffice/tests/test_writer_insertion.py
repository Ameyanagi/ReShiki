"""Writer insertion contracts with fake hosts; no desktop or subprocess access.

Run with Python UNO bindings. These tests do not establish native Writer
selection, layout, or save/reopen acceptance.
"""

import base64
import importlib.util
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

SOURCE = Path(__file__).parents[1] / "extension" / "reshiki.py"
SPEC = importlib.util.spec_from_file_location("reshiki_writer_extension", SOURCE)
extension = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(extension)

GUIDANCE = (
    "Place a text cursor in the Writer document, then choose ReShiki → Paste ReShiki Drawing."
)
PACKET = {
    "version": 1,
    "native": base64.b64encode(b'{"version":18,"atoms":[],"bonds":[]}').decode("ascii"),
    "png": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=",
    "extent": [100, 200],
}


class Text:
    def __init__(self, name):
        self.name = name
        self.String = "Existing text and selection must survive."
        self.sibling = object()
        self.contents = [self.sibling]
        self.position = SimpleNamespace(getText=lambda: self)
        self.insertions, self.removals = [], []

    def insertTextContent(self, position, content, absorb):
        if position.getText() is not self:
            raise RuntimeError("text interface and cursor not related")
        if absorb:
            raise AssertionError("Existing text must not be replaced")
        self.insertions.append((position, content, absorb))
        self.contents.append(content)

    def removeTextContent(self, content):
        self.removals.append(content)
        self.contents.remove(content)


class Storage:
    def __init__(self):
        self.entries = {"ExistingDrawing": b"existing native data"}
        self.removed = []

    def hasByName(self, name):
        return name in self.entries

    def removeElement(self, name):
        self.removed.append(name)
        del self.entries[name]


class WriterHost:
    def __init__(self, owner_name="body"):
        self.owner = Text(owner_name)
        self.body = self.owner if owner_name == "body" else Text("body")
        if self.body is not self.owner:
            self.body.insertTextContent = Mock(side_effect=AssertionError("Wrong text owner"))
            self.body.removeTextContent = Mock(side_effect=AssertionError("Wrong rollback owner"))
        self.view = SimpleNamespace(
            getText=Mock(return_value=self.owner),
            getStart=Mock(return_value=self.owner.position),
        )
        self.image = SimpleNamespace(Name="Image1", data=b"static image", anchor=object())
        self.storage = Storage()
        self.object = SimpleNamespace()
        self.document = SimpleNamespace(
            supportsService=lambda name: name == "com.sun.star.text.TextDocument",
            Text=self.body,
            GraphicObjects=(self.image,),
            CurrentController=SimpleNamespace(getViewCursor=Mock(return_value=self.view)),
            getDocumentStorage=Mock(return_value=self.storage),
            createInstance=Mock(return_value=self.object),
            setModified=Mock(),
        )

    def select_image(self):
        self.view.getText.side_effect = extension.UnoRuntimeException("no text selection", None)


class WriterInsertionTests(unittest.TestCase):
    def setUp(self):
        self.embedded = self.enterContext(patch.object(extension, "Embedded"))
        self.write = self.embedded.return_value._write

        def write(storage, entry):
            storage.entries[entry] = b"new native data"

        self.write.side_effect = write

    def assert_untouched(self, host):
        host.document.getDocumentStorage.assert_not_called()
        host.document.createInstance.assert_not_called()
        host.document.setModified.assert_not_called()
        self.embedded.assert_not_called()
        self.assertEqual(host.storage.entries, {"ExistingDrawing": b"existing native data"})
        self.assertEqual(host.owner.contents, [host.owner.sibling])
        self.assertEqual(host.document.GraphicObjects, (host.image,))
        self.assertEqual(host.image.data, b"static image")

    def dispatch(self, host):
        handler = extension.Handler(None)
        handler.frame = SimpleNamespace(
            getController=lambda: SimpleNamespace(getModel=lambda: host.document)
        )
        self.jobs, self.callbacks = [], []
        self.enterContext(patch.object(extension, "executable", return_value="editor"))
        self.enterContext(patch.object(extension, "uses_host_clipboard", return_value=False))
        self.worker = self.enterContext(patch.object(extension, "worker", return_value=PACKET))
        self.errors = self.enterContext(patch.object(extension, "show_error"))
        self.thread = self.enterContext(
            patch.object(
                extension.threading,
                "Thread",
                side_effect=lambda *, target, daemon: SimpleNamespace(
                    start=lambda: self.jobs.append(target)
                ),
            )
        )
        self.enterContext(
            patch.object(
                extension, "post", side_effect=lambda ctx, callback: self.callbacks.append(callback)
            )
        )
        handler.dispatch(SimpleNamespace(Path="paste"), ())

    def test_insertion_uses_body_cell_frame_or_header_owner_and_stable_range(self):
        for name in ("body", "table-cell", "text-frame", "header"):
            with self.subTest(owner=name):
                host = WriterHost(name)
                original_text = host.owner.String
                inserted = extension.insert(None, host.document, PACKET)
                self.assertIs(inserted, host.object)
                self.assertEqual(host.owner.insertions, [(host.owner.position, host.object, False)])
                self.assertEqual(host.owner.contents, [host.owner.sibling, host.object])
                self.assertEqual(host.owner.String, original_text)
                self.assertEqual(host.object.AnchorType.value, "AS_CHARACTER")
                self.assertEqual((host.object.Width, host.object.Height), (100, 200))
                self.assertIn(host.object.StreamName, host.storage.entries)
                host.document.setModified.assert_called_once_with(True)
                if host.body is not host.owner:
                    host.body.insertTextContent.assert_not_called()
                    host.body.removeTextContent.assert_not_called()

    def test_rollback_uses_saved_owner_and_preserves_existing_entries(self):
        host = WriterHost("table-cell")
        failure = RuntimeError("marking modified failed")

        def fail(modified):
            # A later UI target must not become the rollback owner.
            host.view.getText.return_value = Text("different owner")
            raise failure

        host.document.setModified.side_effect = fail
        with self.assertRaises(RuntimeError) as caught:
            extension.insert(None, host.document, PACKET)
        self.assertIs(caught.exception, failure)
        self.assertEqual(host.owner.removals, [host.object])
        self.assertEqual(host.owner.contents, [host.owner.sibling])
        self.assertEqual(host.storage.removed, [host.object.StreamName])
        self.assertEqual(host.storage.entries, {"ExistingDrawing": b"existing native data"})
        host.body.removeTextContent.assert_not_called()

    def test_selected_image_rejects_direct_insert_before_storage_or_object_creation(self):
        host = WriterHost()
        host.select_image()
        with self.assertRaises(ValueError) as caught:
            extension.insert(None, host.document, PACKET)
        self.assertEqual(str(caught.exception), GUIDANCE)
        self.assert_untouched(host)

    def test_selected_image_preflight_starts_no_renderer(self):
        host = WriterHost()
        host.select_image()
        self.dispatch(host)
        self.thread.assert_not_called()
        self.worker.assert_not_called()
        self.assertFalse(self.jobs)
        self.assertFalse(self.callbacks)
        self.errors.assert_called_once()
        self.assertEqual(str(self.errors.call_args.args[1]), GUIDANCE)
        self.assert_untouched(host)

    def test_selection_changed_to_image_while_rendering_is_rechecked_before_storage(self):
        host = WriterHost("header")
        self.dispatch(host)
        self.assertEqual(len(self.jobs), 1)
        self.jobs.pop()()
        self.worker.assert_called_once_with("editor", "--libreoffice-clipboard")
        host.select_image()
        self.assertEqual(len(self.callbacks), 1)
        self.callbacks.pop()()
        self.errors.assert_called_once()
        self.assertEqual(str(self.errors.call_args.args[1]), GUIDANCE)
        self.assert_untouched(host)

    def test_selection_changed_to_another_text_owner_is_reacquired_after_rendering(self):
        host = WriterHost("header")
        self.dispatch(host)
        self.jobs.pop()()
        current = Text("table-cell")
        host.view.getText.return_value = current
        host.view.getStart.return_value = current.position
        self.callbacks.pop()()
        self.assertEqual(current.insertions, [(current.position, host.object, False)])
        self.assertFalse(host.owner.insertions)
        self.errors.assert_not_called()

    def test_unrelated_range_runtime_errors_keep_their_identity(self):
        for method in ("getText", "getStart"):
            with self.subTest(method=method):
                host = WriterHost()
                failure = extension.UnoRuntimeException("disposed text view", None)
                getattr(host.view, method).side_effect = failure
                with self.assertRaises(extension.UnoRuntimeException) as caught:
                    extension.insert(None, host.document, PACKET)
                self.assertIs(caught.exception, failure)
                self.assert_untouched(host)

    def test_missing_owner_or_position_is_rejected_before_storage(self):
        for method in ("getText", "getStart"):
            with self.subTest(method=method):
                host = WriterHost()
                getattr(host.view, method).return_value = None
                with self.assertRaises(ValueError) as caught:
                    extension.insert(None, host.document, PACKET)
                self.assertEqual(str(caught.exception), GUIDANCE)
                self.assert_untouched(host)

    def test_storage_runtime_error_is_not_reworded_as_selection_guidance(self):
        host = WriterHost()
        failure = extension.UnoRuntimeException("storage unavailable", None)
        host.document.getDocumentStorage.side_effect = failure
        with self.assertRaises(extension.UnoRuntimeException) as caught:
            extension.insert(None, host.document, PACKET)
        self.assertIs(caught.exception, failure)
        self.embedded.assert_not_called()
        host.document.createInstance.assert_not_called()

    def test_insertion_runtime_error_retains_identity_and_cleans_new_storage(self):
        host = WriterHost("text-frame")
        failure = extension.UnoRuntimeException("frame is protected", None)
        host.owner.insertTextContent = Mock(side_effect=failure)
        with self.assertRaises(extension.UnoRuntimeException) as caught:
            extension.insert(None, host.document, PACKET)
        self.assertIs(caught.exception, failure)
        self.assertEqual(host.storage.entries, {"ExistingDrawing": b"existing native data"})
        self.assertEqual(host.storage.removed, [host.object.StreamName])
        self.assertFalse(host.owner.removals)
        host.document.setModified.assert_not_called()

    def test_calc_and_impress_still_insert_on_their_own_pages(self):
        for kind in ("sheet.SpreadsheetDocument", "presentation.PresentationDocument"):
            with self.subTest(kind=kind):
                page = SimpleNamespace(add=Mock(), remove=Mock())
                controller = SimpleNamespace(
                    getActiveSheet=Mock(return_value=SimpleNamespace(DrawPage=page)),
                    getCurrentPage=Mock(return_value=page),
                    getViewCursor=Mock(side_effect=AssertionError("Not a Writer document")),
                )
                shape, storage = SimpleNamespace(), Storage()
                document = SimpleNamespace(
                    supportsService=lambda name: name == "com.sun.star." + kind,
                    CurrentController=controller,
                    createInstance=Mock(return_value=shape),
                    getDocumentStorage=Mock(return_value=storage),
                    setModified=Mock(),
                )
                self.assertIs(extension.insert(None, document, PACKET), shape)
                document.createInstance.assert_called_once_with("com.sun.star.drawing.OLE2Shape")
                page.add.assert_called_once_with(shape)
                self.assertIn(shape.PersistName, storage.entries)
                self.assertEqual((shape.Size.Width, shape.Size.Height), (100, 200))
                controller.getViewCursor.assert_not_called()
                if kind == "sheet.SpreadsheetDocument":
                    controller.getActiveSheet.assert_called_once_with()
                    controller.getCurrentPage.assert_not_called()
                else:
                    controller.getCurrentPage.assert_called_once_with()
                    controller.getActiveSheet.assert_not_called()


if __name__ == "__main__":
    unittest.main()
