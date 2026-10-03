"""Controlled native-client resize callbacks; run with LibreOffice's UNO Python.

These tests model Writer's fresh scale-1 client and do not replace a native
save/close/reopen test of an actually scaled Writer embedded object.
"""

import tempfile
import threading
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

from test_session import extension


class WriterFrame:
    def __init__(self, owner, dimensions):
        self.owner, self.dimensions = owner, dimensions
        self.reads, self.writes = [], []
        self.on_size, self.on_name, self.on_write = None, None, None

    @property
    def Size(self):
        self.reads.append("Size")
        if self.on_size:
            self.on_size()
        return extension.size(*self.dimensions)

    @Size.setter
    def Size(self, value):
        self.writes.append((value.Width, value.Height))
        if self.on_write:
            self.on_write()
        self.dimensions = value.Width, value.Height

    @property
    def StreamName(self):
        self.reads.append("StreamName")
        if self.owner.state == extension.LOADED:
            self.owner.changeState(extension.RUNNING)
        if self.on_name:
            self.on_name()
        return self.owner.entry


class WriterScaleTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.old = (b"original native", b"original PNG", (800, 400))
        self.new = (b"edited native", b"edited PNG", (600, 600))
        self.obj = extension.Embedded(None, self.old)
        self.obj.parent, self.obj.entry = "storage", "Persisted A"
        self.obj.state = extension.RUNNING
        self.a = WriterFrame(self.obj, (1000, 500))
        self.other = extension.Embedded(None, (b"B native", b"B PNG", (400, 500)))
        self.other.entry = "Persisted B"
        self.b = WriterFrame(self.other, (400, 500))
        # Friendly names deliberately point at the wrong persisted identity.
        self.frames = {"Persisted A": self.b, "Renamed user-facing label": self.a}
        self.modified = []
        self.host = SimpleNamespace(
            supportsService=lambda name: name == "com.sun.star.text.TextDocument",
            getEmbeddedObjects=lambda: SimpleNamespace(
                getElementNames=lambda: tuple(self.frames), getByName=self.frames.__getitem__
            ),
            setModified=self.modified.append,
        )
        self.scale = (1, 1)
        self.events, self.visibility, self.stored = [], [], []
        self.on_save, self.on_event, self.on_visible = None, None, None

        def resize():
            self.a.dimensions = tuple(
                round(intrinsic * scale) for intrinsic, scale in zip(self.obj.extent, self.scale)
            )

        def visible(value):
            self.visibility.append(value)
            resize()  # visibilityChanged → Invalidate → Writer ViewChanged.
            if self.on_visible:
                self.on_visible()

        def save():
            self.obj.storeOwn()
            self.obj.update()  # Native saveObject also calls update before returning.
            if self.on_save:
                self.on_save()

        def event(value):
            self.events.append(value.EventName)
            if value.EventName == "OnVisAreaChanged":
                resize()
            if self.on_event:
                self.on_event(value.EventName)

        self.client = SimpleNamespace(
            getComponent=lambda: self.host, saveObject=save, visibilityChanged=visible
        )
        self.obj.client = self.client
        self.other.client = SimpleNamespace(getComponent=lambda: self.host)
        self.other_events = []
        self.other.addEventListener(SimpleNamespace(notifyEvent=self.other_events.append))
        self.obj.addEventListener(SimpleNamespace(notifyEvent=event))
        self.guard = SimpleNamespace(model=self.host, owns=lambda *args: True, release=Mock())
        path = self.root / "drawing.rsk"
        path.write_bytes(self.new[0])
        self.session = {
            "token": "original owned session",
            "directory": self.root,
            "path": path,
            "guard": self.guard,
            "process": SimpleNamespace(poll=lambda: 0),
            "accepted": self.old[0],
            "invalid": False,
            "error": None,
            "error_reported": False,
            "watch_complete": False,
            "completions": set(),
            "completion_lock": threading.Lock(),
        }
        self.obj.session = self.session
        write = patch.object(
            self.obj,
            "_write",
            side_effect=lambda parent, entry: self.stored.append(
                (parent, entry, self.obj.native, self.obj.png, self.obj.extent)
            ),
        )
        write.start()
        self.addCleanup(write.stop)
        report = patch.object(extension, "show_error")
        self.report = report.start()
        self.addCleanup(report.stop)

    def accept(self, data=None):
        done = threading.Event()
        self.obj._accept(self.session, data or self.new, done)
        self.assertTrue(done.is_set())
        return done

    def assert_other_untouched(self):
        self.assertEqual(self.b.dimensions, (400, 500))
        self.assertEqual(self.b.writes, [])
        self.assertEqual(
            (self.other.native, self.other.png, self.other.extent),
            (b"B native", b"B PNG", (400, 500)),
        )
        self.assertEqual(self.other_events, [])

    def test_native_like_visibility_and_content_resets_preserve_scale_across_two_edits(self):
        for dimensions, cached_scale in (
            ((800, 400), (1, 1)),
            ((1000, 500), (1, 1)),
            ((1000, 300), (1, 1)),
            ((1000, 500), (1.25, 1.25)),
        ):
            with self.subTest(dimensions=dimensions, cached_scale=cached_scale):
                self.obj.native, self.obj.png, self.obj.extent = self.old
                self.obj.state = extension.RUNNING
                self.a.dimensions, self.scale = dimensions, cached_scale
                self.session["error"] = None
                self.obj._set_state(extension.ACTIVE)
                self.assertEqual(self.a.dimensions, dimensions)
                self.accept()
                expected = tuple(
                    round(d * n / o) for d, n, o in zip(dimensions, self.new[2], self.old[2])
                )
                self.assertEqual(self.a.dimensions, expected)
                second = (b"second edit", b"second PNG", (1000, 200))
                self.accept(second)
                expected = tuple(
                    round(d * n / o) for d, n, o in zip(dimensions, second[2], self.old[2])
                )
                self.assertEqual(self.a.dimensions, expected)
                self.obj._set_state(extension.RUNNING)
                self.assertEqual(self.a.dimensions, expected)
                self.assertIsNone(self.session["error"])
                self.assertEqual(self.session["accepted"], second[0])
                self.assert_other_untouched()
        self.assertEqual(self.visibility, [True, False] * 4)
        self.assertEqual(self.events.count("OnVisAreaChanged"), 16)
        self.assertEqual(self.events.count("OnSaveDone"), 16)

    def test_no_edit_open_close_restores_visibility_resize_and_then_cleans_up(self):
        self.obj._set_state(extension.ACTIVE)
        self.assertEqual(self.a.dimensions, (1000, 500))
        self.obj._finish(self.session)
        self.assertEqual(self.a.dimensions, (1000, 500))
        self.assertEqual(self.visibility, [True, False])
        self.assertEqual(self.events, [])
        self.assert_other_untouched()
        self.assertFalse(self.root.exists())
        self.guard.release.assert_called_once_with(self.obj, self.session)

    def test_loaded_stream_name_reads_size_first_without_synthetic_resize(self):
        self.obj.state = extension.LOADED
        self.accept()
        self.assertEqual(self.a.reads, ["Size", "StreamName"])
        self.assertEqual(self.b.reads, ["Size", "StreamName"])
        self.assertEqual(self.a.dimensions, (750, 750))
        self.assertEqual(
            self.events, ["OnSaveDone", "OnVisAreaChanged", "OnVisAreaChanged", "OnSaveDone"]
        )
        self.assert_other_untouched()

    def test_missing_and_duplicate_persisted_entries_fail_before_launch_or_save(self):
        for kind in ("missing", "duplicate"):
            with self.subTest(kind=kind):
                self.frames = {"B": self.b}
                if kind == "duplicate":
                    self.frames.update(A=self.a, alias=WriterFrame(self.obj, (99, 99)))
                self.session["error"] = None
                self.obj.session = self.session
                self.accept()
                self.assertTrue(self.session["error"])
                self.assertEqual(self.stored, [])
                self.assertEqual(self.a.writes, [])
                self.assertFalse((self.root / "accepted.json").exists())
                self.obj.session = None
                with (
                    patch.object(extension, "executable", return_value="editor"),
                    patch.object(
                        extension,
                        "acquire_host_guard",
                        side_effect=lambda obj, s: s.__setitem__("guard", self.guard),
                    ),
                    patch.object(extension.subprocess, "Popen") as launch,
                ):
                    self.obj.doVerb(0)
                launch.assert_not_called()
                self.assertIsNone(self.obj.session)
                self.assert_other_untouched()

    def test_save_as_reentered_during_lookup_defers_completion_until_new_storage_ready(self):
        def save_as():
            self.a.on_name = None
            self.obj.storeAsEntry("destination", "New persisted A", (), ())

        self.a.on_name = save_as
        done = threading.Event()
        with (
            patch.object(self.obj, "_queue_replacement"),
            patch.object(extension, "post", side_effect=lambda ctx, action: action()),
        ):
            self.obj._accept(self.session, self.new, done)
            self.assertFalse(done.is_set())
            self.assertEqual(self.obj.native, self.old[0])
            self.assertEqual(self.stored, [("destination", "New persisted A", *self.old)])
            self.assertEqual(self.a.writes, [])
            self.obj.saveCompleted(True)
        self.assertTrue(done.is_set())
        self.assertIsNone(self.session["error"])
        self.assertEqual(self.stored[-1], ("destination", "New persisted A", *self.new))
        self.assertEqual(self.a.dimensions, (750, 750))
        self.assertEqual(self.session["accepted"], self.new[0])

    def test_completed_destination_change_during_lookup_fails_before_mutation(self):
        self.a.on_name = lambda: setattr(self.obj, "entry", "Changed entry")
        self.accept()
        self.assertTrue(self.session["error"])
        self.assertEqual(self.obj.native, self.old[0])
        self.assertEqual(self.stored, [])
        self.assertEqual(self.a.writes, [])

    def test_failure_after_persistence_restores_original_payload_storage_and_size(self):
        with patch.object(Path, "write_text", side_effect=OSError("ACK rejected")):
            self.accept()
        self.assertIn("ACK rejected", self.session["error"])
        self.assertEqual(
            self.stored,
            [("storage", self.obj.entry, *self.new), ("storage", self.obj.entry, *self.old)],
        )
        self.assertEqual((self.obj.native, self.obj.png, self.obj.extent), self.old)
        self.assertEqual(self.a.dimensions, (1000, 500))
        self.assertEqual(self.session["accepted"], self.old[0])
        self.assert_other_untouched()

    def test_failure_during_hands_off_never_rolls_back_old_storage(self):
        def fail():
            self.obj.pending = ("new destination", "new entry")
            raise RuntimeError("host failed during HandsOff")

        self.on_save = fail
        self.accept()
        self.assertEqual(len(self.stored), 1)
        self.assertEqual(self.a.writes, [])
        self.assertEqual(self.session["accepted"], self.old[0])
        self.assertIn("HandsOff", self.session["error"])

    def test_completed_save_as_during_save_or_content_never_rolls_back_changed_destination(self):
        for boundary in ("save", "content"):
            with self.subTest(boundary=boundary):
                self.obj.parent, self.obj.entry = "storage", "Persisted A"
                self.obj.native, self.obj.png, self.obj.extent = self.old
                self.session["error"] = None
                self.stored.clear()
                self.a.writes.clear()
                self.on_save = self.on_event = None

                def move():
                    self.obj.parent, self.obj.entry = "new storage", "New entry"

                if boundary == "save":
                    self.on_save = move
                else:
                    self.on_event = lambda name: move() if name == "OnVisAreaChanged" else None
                self.accept()
                self.assertIn("changed the drawing's storage", self.session["error"])
                self.assertEqual(len(self.stored), 1)
                self.assertEqual(self.a.writes, [])
                self.assertEqual(self.session["accepted"], self.old[0])
                self.assertFalse((self.root / "accepted.json").exists())

    def test_disposal_and_client_replacement_stop_later_geometry_and_storage_writes(self):
        original = self.client
        for boundary in ("getComponent", "Size", "StreamName", "content", "save", "restore"):
            for kind in ("dispose", "client"):
                with self.subTest(boundary=boundary, kind=kind):
                    self.obj.client, self.obj.disposed = original, False
                    self.obj.native, self.obj.png, self.obj.extent = self.old
                    self.session["error"] = None
                    self.stored.clear()
                    self.a.writes.clear()
                    self.a.on_size = self.a.on_name = self.a.on_write = None
                    self.on_event = self.on_save = None

                    def lose():
                        if kind == "dispose":
                            self.obj.disposed = True
                        else:
                            self.obj.client = SimpleNamespace(getComponent=lambda: self.host)

                    self.client.getComponent = lambda: self.host
                    if boundary == "getComponent":
                        self.client.getComponent = lambda: (lose(), self.host)[1]
                    elif boundary == "Size":
                        self.a.on_size = lose
                    elif boundary == "StreamName":
                        self.a.on_name = lose
                    elif boundary == "content":
                        self.on_event = lambda name: lose() if name == "OnVisAreaChanged" else None
                    elif boundary == "save":
                        self.on_save = lose
                    else:
                        self.a.on_write = lose
                    self.accept()
                    self.assertTrue(self.session["error"])
                    self.assertEqual(self.session["accepted"], self.old[0])
                    self.assertLessEqual(len(self.stored), 1)
                    self.assertEqual(len(self.a.writes), 1 if boundary == "restore" else 0)
                    self.assertFalse((self.root / "accepted.json").exists())

    def test_changed_client_in_changing_state_never_receives_visibility_callback(self):
        replacement = SimpleNamespace(visibilityChanged=Mock())
        self.obj.addStateChangeListener(
            SimpleNamespace(
                changingState=lambda *args: setattr(self.obj, "client", replacement),
                stateChanged=Mock(),
            )
        )
        with self.assertRaisesRegex(RuntimeError, "no longer available"):
            self.obj._set_state(extension.ACTIVE)
        replacement.visibilityChanged.assert_not_called()
        self.assertEqual(self.visibility, [])
        self.assertEqual(self.a.writes, [])

    def test_lifetime_loss_at_each_state_callback_stops_following_callbacks_and_restore(self):
        for boundary in ("changing", "visible", "changed"):
            for kind in ("disposed", "invalid", "guard", "model"):
                with self.subTest(boundary=boundary, kind=kind):
                    self.obj.disposed = self.session["invalid"] = False
                    self.guard.owns, self.guard.model = lambda *args: True, self.host
                    self.obj.state = extension.RUNNING
                    self.obj.states.clear()
                    self.a.writes.clear()
                    visited = []

                    def callback(name):
                        visited.append(name)
                        if name != boundary:
                            return
                        if kind == "disposed":
                            self.obj.disposed = True
                        elif kind == "invalid":
                            self.session["invalid"] = True
                        elif kind == "guard":
                            self.guard.owns = lambda *args: False
                        else:
                            self.guard.model = object()

                    self.obj.addStateChangeListener(
                        SimpleNamespace(
                            changingState=lambda *args: callback("changing"),
                            stateChanged=lambda *args: callback("changed"),
                        )
                    )
                    self.on_visible = lambda: callback("visible")
                    with self.assertRaisesRegex(RuntimeError, "no longer available"):
                        self.obj._set_state(extension.ACTIVE)
                    self.assertEqual(
                        visited,
                        ["changing", "visible", "changed"][
                            : ("changing", "visible", "changed").index(boundary) + 1
                        ],
                    )
                    self.assertEqual(self.a.writes, [])

    def test_finish_lookup_failure_retains_draft_and_releases_once_without_visibility(self):
        self.obj.state = extension.ACTIVE
        self.frames = {"B": self.b}
        with self.assertRaisesRegex(extension.IOException, "Cannot locate"):
            self.obj._finish(self.session)
        self.assertIsNone(self.obj.session)
        self.assertEqual(self.obj.state, extension.RUNNING)
        self.assertTrue(self.session["path"].exists())
        self.assertEqual(self.visibility, [])
        self.assertEqual(self.a.writes, [])
        self.guard.release.assert_called_once_with(self.obj, self.session)

    def test_activation_failure_keeps_started_watcher_and_original_draft_until_child_exit(self):
        self.obj.session = None
        self.on_visible = lambda: (_ for _ in ()).throw(RuntimeError("activation callback failed"))
        directory = self.root / "launched"
        directory.mkdir()
        process = SimpleNamespace(poll=lambda: None)
        with (
            patch.object(extension, "executable", return_value="editor"),
            patch.object(extension.tempfile, "mkdtemp", return_value=str(directory)),
            patch.object(
                extension,
                "acquire_host_guard",
                side_effect=lambda obj, s: s.__setitem__("guard", self.guard),
            ),
            patch.object(extension.subprocess, "Popen", return_value=process) as launch,
            patch.object(extension.threading, "Thread") as watcher,
        ):
            self.obj.doVerb(0)
        launch.assert_called_once()
        watcher.return_value.start.assert_called_once()
        session = self.obj.session
        self.assertIn("activation callback failed", session["error"])
        self.assertEqual(self.a.dimensions, (1000, 500))
        self.obj._finish(session)
        self.assertIs(self.obj.session, session)
        self.guard.release.assert_not_called()
        self.assertEqual(session["path"].read_bytes(), self.old[0])
        self.on_visible = None
        process.poll = lambda: 0
        self.obj._finish(session)
        self.assertTrue(session["path"].exists())
        self.guard.release.assert_called_once_with(self.obj, session)

    def test_finish_callback_and_restore_and_report_failures_keep_primary_error_and_draft(self):
        self.obj.state = extension.ACTIVE
        self.on_visible = lambda: (_ for _ in ()).throw(RuntimeError("primary callback failure"))
        self.a.on_write = lambda: (_ for _ in ()).throw(RuntimeError("secondary restore failure"))
        self.report.side_effect = RuntimeError("tertiary reporting failure")
        with self.assertRaisesRegex(RuntimeError, "primary callback failure.*secondary restore"):
            self.obj._finish(self.session)
        self.assertIsNone(self.obj.session)
        self.assertEqual(self.obj.state, extension.RUNNING)
        self.assertEqual(self.session["path"].read_bytes(), self.new[0])
        self.assertIn("primary callback failure", self.session["error"])
        self.assertIn("secondary restore failure", self.session["error"])
        self.assertIn("tertiary reporting failure", self.session["error"])
        self.guard.release.assert_called_once_with(self.obj, self.session)

    def test_finish_replacement_session_blocks_restore_and_preserves_new_session(self):
        self.obj.state = extension.ACTIVE
        replacement = {"token": "replacement"}
        self.on_visible = lambda: setattr(self.obj, "session", replacement)
        with self.assertRaisesRegex(RuntimeError, "no longer available"):
            self.obj._finish(self.session)
        self.assertIs(self.obj.session, replacement)
        self.assertEqual(self.a.writes, [])
        self.assertTrue(self.session["path"].exists())
        self.guard.release.assert_called_once_with(self.obj, self.session)


if __name__ == "__main__":
    unittest.main()
