"""Run with a Python interpreter that provides LibreOffice's uno/unohelper.

These tests exercise real session methods with controlled host/subprocess
boundaries; they do not require a desktop or a running LibreOffice instance.
"""

import importlib.util
import tempfile
import threading
import unittest
from pathlib import Path
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


if __name__ == "__main__":
    unittest.main()
