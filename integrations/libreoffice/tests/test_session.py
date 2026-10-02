"""Run with a Python interpreter that provides LibreOffice's uno/unohelper.

These tests exercise real session methods with controlled host/subprocess
boundaries; they do not require a desktop or a running LibreOffice instance.
"""

import importlib.util
import tempfile
import threading
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

SOURCE = Path(__file__).parents[1] / "extension" / "reshiki.py"
SPEC = importlib.util.spec_from_file_location("reshiki_extension", SOURCE)
extension = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(extension)


class SessionTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)
        self.old = (b'{"version":15}', b"old PNG", (100, 200))
        self.new = (b'{"version":15,"title":"new"}', b"new PNG", (300, 400))
        self.object = extension.Embedded(None, self.old)
        self.object.parent, self.object.entry = "old storage", "Object 1"
        self.session = {"directory": self.root, "accepted": self.old[0], "error": None}
        self.object.session = self.session

    def tearDown(self):
        self.directory.cleanup()

    def test_final_atomic_save_between_read_and_exit_is_accepted(self):
        path = self.root / "drawing.rsk"
        path.write_bytes(self.old[0])
        owner = self

        class Process:
            exited = False

            def poll(self):
                return 0 if self.exited else None

        process = Process()

        class RacingSource:
            def __enter__(self):
                return self

            def __exit__(self, *args):
                pass

            def read(self, limit):
                raw = path.read_bytes()
                # Atomic save+exit just after returning the previous file's bytes.
                path.write_bytes(owner.new[0])
                process.exited = True
                return raw

        class RacingPath:
            def open(self, mode):
                return RacingSource()

        self.session.update(path=RacingPath(), process=process)
        accepted = []

        def accept(session, data, done):
            accepted.append(data)
            session["accepted"] = data[0]
            done.set()

        with (
            patch.object(extension, "post", side_effect=lambda ctx, action: action()),
            patch.object(extension, "worker", return_value={}),
            patch.object(extension, "packet", return_value=self.new),
            patch.object(extension.time, "sleep"),
            patch.object(self.object, "_accept", side_effect=accept),
        ):
            self.object._watch(self.session, "unused executable")
        self.assertEqual(accepted, [self.new])
        self.assertEqual(self.session["accepted"], self.new[0])
        self.assertIsNone(self.object.session)

    def test_host_failure_after_persistence_rolls_back_storage_and_acceptance(self):
        stored = []
        owner = self.object

        class Client:
            def saveObject(self):
                owner._write(owner.parent, owner.entry)
                raise RuntimeError("host rejected after storage commit")

        self.object.client = Client()
        done = threading.Event()
        with patch.object(
            self.object, "_write", side_effect=lambda *args: stored.append(self.object.native)
        ):
            self.object._accept(self.session, self.new, done)
        self.assertEqual(stored, [self.new[0], self.old[0]])
        self.assertEqual((self.object.native, self.object.png, self.object.extent), self.old)
        self.assertEqual(self.session["accepted"], self.old[0])
        self.assertIn("host rejected", self.session["error"])
        self.assertTrue(done.is_set())
        self.assertFalse((self.root / "accepted.json").exists())

    def test_saveback_waits_for_save_as_destination_acceptance(self):
        stored = []
        owner = self.object

        class Client:
            def saveObject(self):
                owner.storeOwn()

            def getComponent(self):
                return None

        self.object.client = Client()
        done = threading.Event()
        with (
            patch.object(self.object, "_queue_replacement"),
            patch.object(
                self.object,
                "_write",
                side_effect=lambda parent, name: stored.append((parent, self.object.native)),
            ),
            patch.object(extension, "post", side_effect=lambda ctx, action: action()),
        ):
            self.object.storeAsEntry("new storage", "Object 1", (), ())
            self.object._accept(self.session, self.new, done)
            self.assertFalse(done.is_set())
            self.assertEqual(stored, [("new storage", self.old[0])])
            with self.assertRaises(extension.WrongStateException):
                self.object.storeOwn()
            self.object.saveCompleted(True)
        self.assertTrue(done.is_set())
        self.assertEqual(stored[-1], ("new storage", self.new[0]))
        self.assertEqual(self.session["accepted"], self.new[0])
        self.assertIsNone(self.session["error"])

    def test_stale_session_cannot_update_another_object(self):
        self.object.session = {"token": "replacement"}
        done = threading.Event()
        self.object._accept(self.session, self.new, done)
        self.assertEqual(self.object.native, self.old[0])
        self.assertEqual(self.session["accepted"], self.old[0])
        self.assertTrue(done.is_set())
        self.assertIn("no longer available", self.session["error"])

    def test_no_init_completes_host_save_as_before_accepting_deferred_edit(self):
        for target in (
            ("new storage", "New Object"),
            ("old storage", "Object 1"),
            ("third storage", "Other Object"),
        ):
            with self.subTest(target=target):
                obj = extension.Embedded(None, self.old)
                obj.parent, obj.entry = "old storage", "Object 1"
                session = {"directory": self.root, "accepted": self.old[0], "error": None}
                obj.session = session
                stored = []

                class Client:
                    def saveObject(self):
                        obj.storeOwn()

                    def getComponent(self):
                        return None

                obj.client = Client()
                done = threading.Event()
                with (
                    patch.object(obj, "_queue_replacement"),
                    patch.object(
                        obj,
                        "_write",
                        side_effect=lambda parent, name: stored.append((parent, name, obj.native)),
                    ),
                    patch.object(extension, "post", side_effect=lambda ctx, action: action()),
                ):
                    obj.storeAsEntry("new storage", "New Object", (), ())
                    obj._accept(session, self.new, done)
                    self.assertFalse(done.is_set())
                    obj.setPersistentEntry(*target, 2, (), ())
                self.assertTrue(done.is_set())
                self.assertEqual((obj.parent, obj.entry), target)
                self.assertEqual(stored[-1], (*target, self.new[0]))
                self.assertEqual(session["accepted"], self.new[0])
                self.assertIsNone(session["error"])
                self.assertIsNone(obj.pending)

    def test_host_rejection_after_no_init_keeps_saved_native_and_does_not_acknowledge(self):
        stored = {}
        owner = self.object

        class Client:
            def saveObject(self):
                owner.storeOwn()
                raise RuntimeError("host rejected after Save As")

        self.object.client = Client()
        done = threading.Event()
        with (
            patch.object(self.object, "_queue_replacement"),
            patch.object(
                self.object,
                "_write",
                side_effect=lambda parent, name: stored.__setitem__((parent, name), owner.native),
            ),
            patch.object(extension, "post", side_effect=lambda ctx, action: action()),
        ):
            self.object.storeAsEntry("new storage", "New Object", (), ())
            self.object._accept(self.session, self.new, done)
            self.object.setPersistentEntry("new storage", "New Object", 2, (), ())
        self.assertTrue(done.is_set())
        self.assertEqual(stored, {("new storage", "New Object"): self.old[0]})
        self.assertEqual((self.object.native, self.object.png, self.object.extent), self.old)
        self.assertEqual(self.session["accepted"], self.old[0])
        self.assertIn("host rejected", self.session["error"])
        self.assertFalse((self.root / "accepted.json").exists())
        self.assertIsNone(self.object.pending)

    def test_host_frame_rounding_does_not_change_intrinsic_extent(self):
        for _ in range(20):
            width, height = self.object.extent
            self.object.setVisualAreaSize(1, extension.size(width + 1, height + 1))
        self.assertEqual(self.object.extent, self.old[2])
        self.assertEqual(self.object.native, self.old[0])

    def test_persistence_writes_fallback_preview_to_the_requested_container(self):
        ctx = extension.uno.getComponentContext()
        factory = extension.service(ctx, "com.sun.star.embed.StorageFactory")
        old_parent, new_parent = factory.createInstance(), factory.createInstance()

        def replacement(parent, name):
            images = parent.openStorageElement("ObjectReplacements", 3)
            try:
                stream = images.openStreamElement(name, 3)
                source = stream.getInputStream()
                try:
                    self.assertEqual(stream.getPropertyValue("MediaType"), "image/png")
                    self.assertTrue(stream.getPropertyValue("UseCommonStoragePasswordEncryption"))
                    return source.readBytes(None, extension.LIMIT)[1].value
                finally:
                    source.closeInput()
                    stream.dispose()
            finally:
                images.dispose()

        try:
            obj = extension.Embedded(ctx, self.old)
            obj.parent, obj.entry = old_parent, "ReShiki-first"
            obj.storeToEntry(old_parent, obj.entry, (), ())
            old_parent.commit()
            untouched = extension.Embedded(ctx, (self.old[0], b"other preview", self.old[2]))
            untouched.parent, untouched.entry = old_parent, "ReShiki-other"
            untouched.storeToEntry(old_parent, untouched.entry, (), ())
            old_parent.commit()
            obj.native, obj.png, obj.extent = self.new
            obj.storeAsEntry(new_parent, "ReShiki-renamed", (), ())
            new_parent.commit()
            self.assertEqual(replacement(new_parent, "ReShiki-renamed"), self.new[1])
            self.assertEqual(replacement(old_parent, "ReShiki-first"), self.old[1])
            self.assertEqual(replacement(old_parent, "ReShiki-other"), b"other preview")
            obj.saveCompleted(True)
            self.assertEqual((obj.parent, obj.entry), (new_parent, "ReShiki-renamed"))
        finally:
            old_parent.dispose()
            new_parent.dispose()

    def test_host_image_lock_is_respected_until_destination_precommit(self):
        ctx = extension.uno.getComponentContext()
        parent = extension.service(ctx, "com.sun.star.embed.StorageFactory").createInstance()
        images = parent.openStorageElement("ObjectReplacements", 7)
        obj = extension.Embedded(ctx, self.old)
        obj.parent, obj.entry = parent, "ReShiki-first"
        try:
            obj.storeOwn()
            obj.storeAsEntry(parent, obj.entry, (), ())
            self.assertEqual(len(obj.replacements), 1)
            # The queued replacement is immutable even if later work changes PNG.
            obj.png = self.new[1]
            images.dispose()
            images = None
            parent.commit()
            self.assertEqual(obj.replacements, [])
            images = parent.openStorageElement("ObjectReplacements", 3)
            stream = images.openStreamElement(obj.entry, 3)
            source = stream.getInputStream()
            try:
                self.assertEqual(source.readBytes(None, extension.LIMIT)[1].value, self.old[1])
            finally:
                source.closeInput()
                stream.dispose()
            obj.saveCompleted(True)
        finally:
            if images is not None:
                images.dispose()
            parent.dispose()

    def test_failed_cache_precommit_aborts_and_revert_removes_listener(self):
        ctx = extension.uno.getComponentContext()
        parent = extension.service(ctx, "com.sun.star.embed.StorageFactory").createInstance()
        images = parent.openStorageElement("ObjectReplacements", 7)
        obj = extension.Embedded(ctx, self.old)
        try:
            obj.storeAsEntry(parent, "ReShiki-first", (), ())
            with self.assertRaises(extension.IOException):
                parent.commit()
            self.assertEqual(len(obj.replacements), 1)
            images.dispose()
            images = None
            parent.revert()
            self.assertEqual(obj.replacements, [])
            obj.saveCompleted(False)
            self.assertFalse(parent.hasByName("ReShiki-first"))
        finally:
            if images is not None:
                images.dispose()
            parent.dispose()

    def test_rejected_save_as_and_disposed_destination_cancel_cached_preview(self):
        ctx = extension.uno.getComponentContext()
        factory = extension.service(ctx, "com.sun.star.embed.StorageFactory")
        for rejected in (False, True):
            with self.subTest(rejected=rejected):
                parent = factory.createInstance()
                obj = extension.Embedded(ctx, self.old)
                obj.storeAsEntry(parent, "ReShiki-first", (), ())
                if rejected:
                    obj.saveCompleted(False)
                    parent.commit()
                    self.assertFalse(parent.hasByName("ObjectReplacements"))
                parent.dispose()
                self.assertEqual(obj.replacements, [])

    def presentation_host(self, dimensions, location="slide"):
        class Shapes:
            def __init__(self, *values):
                self.values = values

            def getCount(self):
                return len(self.values)

            def getByIndex(self, index):
                return self.values[index]

        frame = SimpleNamespace(PersistName=self.object.entry, Size=extension.size(*dimensions))
        other = SimpleNamespace(PersistName="Other object", Size=extension.size(400, 500))
        other_page, target_page = Shapes(other), Shapes(Shapes(frame))
        pages, masters, handout = Shapes(other_page), Shapes(), Shapes()
        if location == "slide":
            pages.values += (target_page,)
        elif location == "notes":
            other_page.getNotesPage = lambda: target_page
        elif location == "master":
            masters.values = (target_page,)
        elif location == "master notes":
            master_page = Shapes()
            master_page.getNotesPage = lambda: target_page
            masters.values = (master_page,)
        elif location == "handout":
            handout = target_page
        # Notes-page references need not be followed recursively.
        target_page.getNotesPage = lambda: target_page
        host = SimpleNamespace(
            supportsService=lambda name: name == "com.sun.star.presentation.PresentationDocument",
            getDrawPages=lambda: pages,
            getMasterPages=lambda: masters,
            getHandoutMasterPage=lambda: handout,
            setModified=lambda value: None,
        )
        self.object.client = SimpleNamespace(
            saveObject=self.object.storeOwn, getComponent=lambda: host
        )
        return frame, other

    def test_impress_saveback_resizes_only_its_frame_and_preserves_user_scale(self):
        for dimensions, expected in (((100, 200), (300, 400)), ((200, 100), (600, 200))):
            with self.subTest(dimensions=dimensions):
                self.object.native, self.object.png, self.object.extent = self.old
                frame, other = self.presentation_host(dimensions)
                done = threading.Event()
                with patch.object(self.object, "_write"):
                    self.object._accept(self.session, self.new, done)
                self.assertTrue(done.is_set())
                self.assertIsNone(self.session["error"])
                self.assertEqual((frame.Size.Width, frame.Size.Height), expected)
                self.assertEqual((other.Size.Width, other.Size.Height), (400, 500))
                self.assertEqual(self.session["accepted"], self.new[0])

    def test_impress_frame_lookup_includes_notes_and_all_master_containers(self):
        for location in ("notes", "master", "master notes", "handout"):
            with self.subTest(location=location):
                self.object.native, self.object.png, self.object.extent = self.old
                frame, other = self.presentation_host((200, 100), location)
                with patch.object(self.object, "_write"):
                    self.object._accept(self.session, self.new, threading.Event())
                self.assertIsNone(self.session["error"])
                self.assertEqual((frame.Size.Width, frame.Size.Height), (600, 200))
                self.assertEqual((other.Size.Width, other.Size.Height), (400, 500))

    def test_failed_acceptance_restores_the_impress_frame_and_stored_drawing(self):
        frame, other = self.presentation_host((200, 100))
        stored = []
        done = threading.Event()
        with (
            patch.object(
                self.object, "_write", side_effect=lambda *args: stored.append(self.object.native)
            ),
            patch.object(Path, "write_text", side_effect=OSError("acknowledgement rejected")),
        ):
            self.object._accept(self.session, self.new, done)
        self.assertTrue(done.is_set())
        self.assertIn("acknowledgement rejected", self.session["error"])
        self.assertEqual(stored, [self.new[0], self.old[0]])
        self.assertEqual((frame.Size.Width, frame.Size.Height), (200, 100))
        self.assertEqual((other.Size.Width, other.Size.Height), (400, 500))
        self.assertEqual(self.session["accepted"], self.old[0])

    def test_external_editor_never_advertises_inplace_ui_states(self):
        self.assertEqual(self.object.getReachableStates(), (0, 1, 2))
        for unsupported in (3, 4):
            with self.assertRaises(extension.IOException):
                self.object.changeState(unsupported)

    def test_external_editor_activation_and_finish_notify_state_visibility_and_repaint(self):
        obj = self.object
        obj.session = None
        obj.state = 1
        events = []

        class Listener:
            def changingState(self, event, old, new):
                events.append(("changing", old, new, obj.getCurrentState()))

            def stateChanged(self, event, old, new):
                events.append(("changed", old, new, obj.getCurrentState()))

            def notifyEvent(self, event):
                events.append((event.EventName, obj.getCurrentState()))

        class Client:
            def visibilityChanged(self, visible):
                events.append(("visible", visible, obj.getCurrentState()))

        obj.client = Client()
        listener = Listener()
        obj.addStateChangeListener(listener)
        obj.addEventListener(listener)
        directory = self.root / "edit"
        directory.mkdir()
        with (
            patch.object(extension, "executable", return_value="/test/editor"),
            patch.object(extension.tempfile, "mkdtemp", return_value=str(directory)),
            patch.object(extension.subprocess, "Popen") as launch,
            patch.object(extension.threading, "Thread"),
        ):
            obj.doVerb(0)
        self.assertEqual(
            launch.call_args.args[0],
            ["/test/editor", "--open", str(directory / "drawing.rsk"), "--libreoffice-edit"],
        )
        self.assertEqual(obj.getCurrentState(), 2)
        self.assertEqual(
            events,
            [
                ("changing", 1, 2, 1),
                ("visible", True, 2),
                ("changed", 1, 2, 2),
                ("OnVisAreaChanged", 2),
            ],
        )
        events.clear()
        obj._finish(obj.session)
        self.assertEqual(obj.getCurrentState(), 1)
        self.assertEqual(
            events,
            [
                ("changing", 2, 1, 2),
                ("visible", False, 1),
                ("changed", 2, 1, 1),
                ("OnVisAreaChanged", 1),
            ],
        )
        self.assertIsNone(obj.session)
        self.assertFalse(directory.exists())

    def test_failed_editor_launch_does_not_report_an_active_window(self):
        self.object.session = None
        self.object.state = 1
        directory = self.root / "failed-edit"
        directory.mkdir()
        with (
            patch.object(extension, "executable", return_value="/missing/editor"),
            patch.object(extension.tempfile, "mkdtemp", return_value=str(directory)),
            patch.object(extension.subprocess, "Popen", side_effect=OSError("launch failed")),
            patch.object(extension, "show_error") as show,
        ):
            self.object.doVerb(0)
        self.assertEqual(self.object.getCurrentState(), 1)
        self.assertIsNone(self.object.session)
        self.assertIn("launch failed", str(show.call_args.args[1]))

    def test_advisory_uno_failures_do_not_abandon_editor_or_skip_other_listeners(self):
        for stage, error_type in (
            ("changing", extension.WrongStateException),
            ("changing", extension.UnoRuntimeException),
            ("changed", extension.UnoRuntimeException),
            ("visible", extension.WrongStateException),
            ("visible", extension.UnoRuntimeException),
        ):
            with self.subTest(stage=stage, error_type=error_type):
                obj = extension.Embedded(None, self.old)
                obj.state = extension.RUNNING
                events = []

                def notify(kind):
                    if kind == stage:
                        raise error_type("advisory notification failed", obj)

                class FailingListener:
                    def changingState(self, *args):
                        notify("changing")

                    def stateChanged(self, *args):
                        notify("changed")

                class Listener:
                    def changingState(self, event, old, new):
                        events.append(("changing", old, new))

                    def stateChanged(self, event, old, new):
                        events.append(("changed", old, new))

                    def notifyEvent(self, event):
                        events.append((event.EventName, obj.getCurrentState()))

                class Client:
                    def visibilityChanged(self, visible):
                        notify("visible")

                listener = Listener()
                obj.client = Client()
                obj.addStateChangeListener(FailingListener())
                obj.addStateChangeListener(listener)
                obj.addEventListener(listener)
                directory = self.root / ("advisory-" + stage + "-" + error_type.__name__)
                directory.mkdir()
                with (
                    patch.object(extension, "executable", return_value="/test/editor"),
                    patch.object(extension.tempfile, "mkdtemp", return_value=str(directory)),
                    patch.object(extension.subprocess, "Popen"),
                    patch.object(extension.threading, "Thread") as thread,
                    patch.object(extension, "show_error") as show,
                ):
                    obj.doVerb(0)
                    thread.return_value.start.assert_called_once_with()
                    self.assertIs(thread.call_args.kwargs["args"][0], obj.session)
                    self.assertEqual(obj.getCurrentState(), extension.ACTIVE)
                    self.assertEqual(obj.session["accepted"], self.old[0])
                    self.assertFalse((directory / "accepted.json").exists())
                    obj._finish(obj.session)
                    show.assert_not_called()
                self.assertEqual(obj.getCurrentState(), extension.RUNNING)
                self.assertIsNone(obj.session)
                self.assertFalse(directory.exists())
                self.assertEqual(obj.native, self.old[0])
                self.assertEqual(
                    events,
                    [
                        ("changing", 1, 2),
                        ("changed", 1, 2),
                        ("OnVisAreaChanged", 2),
                        ("changing", 2, 1),
                        ("changed", 2, 1),
                        ("OnVisAreaChanged", 1),
                    ],
                )

    def test_unexpected_activation_notification_failure_still_starts_watcher(self):
        obj = self.object
        obj.session = None
        directory = self.root / "unexpected-edit"
        directory.mkdir()
        with (
            patch.object(extension, "executable", return_value="/test/editor"),
            patch.object(extension.tempfile, "mkdtemp", return_value=str(directory)),
            patch.object(extension.subprocess, "Popen"),
            patch.object(extension.threading, "Thread") as thread,
            patch.object(obj, "_set_state", side_effect=RuntimeError("unexpected notice")),
            patch.object(extension, "show_error") as show,
        ):
            obj.doVerb(0)
        thread.return_value.start.assert_called_once_with()
        self.assertIs(thread.call_args.kwargs["args"][0], obj.session)
        self.assertEqual(obj.session["accepted"], self.old[0])
        self.assertTrue((directory / "drawing.rsk").exists())
        self.assertIn("unexpected notice", str(show.call_args.args[1]))

    def test_advisory_failure_preserves_rejected_save_and_recovery_file(self):
        obj = self.object
        obj.state = extension.ACTIVE
        path = self.root / "drawing.rsk"
        path.write_bytes(self.new[0])
        self.session["path"] = path

        class Client:
            def saveObject(self):
                raise RuntimeError("host rejected saved drawing")

            def visibilityChanged(self, visible):
                raise extension.WrongStateException("visibility notice failed", obj)

        obj.client = Client()
        done = threading.Event()
        with patch.object(obj, "_write"), patch.object(extension, "show_error") as show:
            obj._accept(self.session, self.new, done)
            obj._finish(self.session)
        self.assertTrue(done.is_set())
        self.assertIsNone(obj.session)
        self.assertEqual(obj.getCurrentState(), extension.RUNNING)
        self.assertEqual(obj.native, self.old[0])
        self.assertEqual(self.session["accepted"], self.old[0])
        self.assertEqual(path.read_bytes(), self.new[0])
        self.assertFalse((self.root / "accepted.json").exists())
        self.assertIn("host rejected saved drawing", str(show.call_args.args[1]))

    def test_unexpected_finish_notification_failure_leaves_no_active_session(self):
        obj = self.object
        obj.state = extension.ACTIVE
        directory = self.root / "finished-edit"
        directory.mkdir()
        self.session["directory"] = directory

        class Listener:
            def changingState(self, *args):
                raise RuntimeError("unexpected finish notice")

        obj.addStateChangeListener(Listener())
        with self.assertRaisesRegex(RuntimeError, "unexpected finish notice"):
            obj._finish(self.session)
        self.assertIsNone(obj.session)
        self.assertEqual(obj.getCurrentState(), extension.RUNNING)
        self.assertFalse(directory.exists())


if __name__ == "__main__":
    unittest.main()
