"""Host close broadcasts and editor lifetime, without a running desktop.

Run with LibreOffice's UNO Python, alongside test_session.py. These fakes model
the native close/lock boundaries; a native Writer close reproduction is still
required because the real dispatcher can replace a controller with Start Center.
"""

import importlib.util
import tempfile
import threading
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

SOURCE = Path(__file__).parents[1] / "extension" / "reshiki.py"
SPEC = importlib.util.spec_from_file_location("reshiki_guard_extension", SOURCE)
extension = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(extension)


class CloseSource:
    def __init__(self):
        self.listeners = []
        self.fail_add = False

    def addCloseListener(self, listener):
        if self.fail_add:
            raise RuntimeError("listener acquisition failed")
        self.listeners.append(listener)

    def removeCloseListener(self, listener):
        self.listeners.remove(listener)

    def query(self, ownership):
        for listener in tuple(self.listeners):
            listener.queryClosing(SimpleNamespace(Source=self), ownership)

    def closed(self, disposal=False):
        for listener in tuple(self.listeners):
            event = SimpleNamespace(Source=self)
            if disposal:
                listener.disposing(event)
            else:
                listener.notifyClosing(event)
        self.listeners.clear()


class Model(CloseSource):
    def __init__(self):
        super().__init__()
        self.URL, self.modified = "file:///original.odt", False
        self.frame = Frame(self)

    def getCurrentController(self):
        return self.frame.controller

    def setModified(self, value):
        self.modified = value


class Frame(CloseSource):
    def __init__(self, model):
        super().__init__()
        self.controller = SimpleNamespace(getModel=lambda: model, getFrame=lambda: self)
        self.locks, self.adds, self.removes = 2, 0, 0  # Two locks belong to other code.
        self.fail_lock, self.on_unlock = False, None
        self.dispatches, self.results = [], []
        self.dispatch_error = None

    def getController(self):
        return self.controller

    def addActionLock(self):
        if self.fail_lock:
            raise RuntimeError("lock acquisition failed")
        self.locks += 1
        self.adds += 1

    def removeActionLock(self):
        self.locks -= 1
        self.removes += 1
        if self.on_unlock:
            self.on_unlock()

    def queryDispatch(self, url, target, flags):
        return self

    def dispatchWithNotification(self, url, args, listener):
        self.dispatches.append(url.Complete)
        if self.dispatch_error:
            raise RuntimeError(self.dispatch_error)
        self.results.append(listener)


class Process:
    def __init__(self, running=False):
        self.running = running

    def poll(self):
        return None if self.running else 0


class HostGuardTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.old = (b'{"version":15}', b"old PNG", (100, 200))
        self.new = (b'{"version":15,"title":"new"}', b"new PNG", (300, 400))
        self.model = Model()
        self.queue = []
        extension.HOST_GUARDS.clear()
        self.addCleanup(extension.HOST_GUARDS.clear)
        extension.RETIRED_HOST_GUARDS.clear()
        self.addCleanup(extension.RETIRED_HOST_GUARDS.clear)
        self.post = patch.object(
            extension, "post", side_effect=lambda ctx, action: self.queue.append(action)
        )
        self.post.start()
        self.addCleanup(self.post.stop)
        errors = patch.object(extension, "show_error")
        self.errors = errors.start()
        self.addCleanup(errors.stop)
        self.serial = 0

    def owner(self, model=None):
        model = model or self.model
        owner = extension.Embedded(None, self.old)
        owner.parent, owner.entry = "storage", "Object 1"
        owner.client = SimpleNamespace(
            getComponent=lambda: model,
            saveObject=owner.storeOwn,
            visibilityChanged=lambda value: None,
        )
        return owner

    def session(self, owner, running=False):
        self.serial += 1
        directory = self.root / str(self.serial)
        directory.mkdir()
        path = directory / "drawing.rsk"
        path.write_bytes(self.old[0])
        session = {
            "token": str(self.serial),
            "directory": directory,
            "path": path,
            "process": Process(running),
            "guard": None,
            "accepted": self.old[0],
            "invalid": False,
            "error": None,
            "completion_lock": threading.Lock(),
            "completions": set(),
            "watch_complete": False,
            "error_reported": False,
        }
        owner.session = session
        extension.acquire_host_guard(owner, session)
        return session

    def pump(self):
        while self.queue:
            self.queue.pop(0)()

    def vetoed(self, source, ownership=False):
        with self.assertRaises(extension.CloseVetoException):
            source.query(ownership)

    def recovery_notices(self):
        return [
            call.args[1]
            for call in self.errors.call_args_list
            if "Your saved drawing remains at:" in str(call.args[1])
        ]

    def test_false_host_broadcasts_veto_and_finish_never_auto_closes(self):
        owner = self.owner()
        session = self.session(owner)
        self.vetoed(self.model)
        self.vetoed(self.model.frame)
        owner._finish(session)
        self.pump()
        self.assertEqual(self.model.frame.dispatches, [])
        self.assertEqual(self.model.listeners, [])
        self.assertEqual(self.model.frame.listeners, [])
        self.assertEqual(self.model.frame.locks, 2)
        self.assertEqual(extension.HOST_GUARDS, [])

    def test_two_objects_share_guard_across_save_as_and_release_exact_tokens(self):
        first, second = self.owner(), self.owner()
        a = self.session(first)
        self.model.URL = "file:///save-as.odt"
        b = self.session(second)
        self.assertIs(a["guard"], b["guard"])
        self.assertNotEqual(a["token"], b["token"])
        with patch.object(extension.subprocess, "Popen") as launch:
            first.doVerb(0)
            launch.assert_not_called()
        second._finish(b)
        self.vetoed(self.model)
        self.assertEqual(self.model.frame.adds, 1)
        self.assertEqual(self.model.frame.removes, 0)
        second._finish(b)
        first._finish(a)
        self.assertEqual(self.model.frame.removes, 1)
        self.assertEqual(self.model.frame.locks, 2)

    def test_equal_urls_do_not_share_guards_between_documents(self):
        other = Model()
        a, b = self.session(self.owner()), self.session(self.owner(other))
        self.assertIsNot(a["guard"], b["guard"])
        self.assertEqual(len(extension.HOST_GUARDS), 2)

    def test_second_active_view_is_rejected_before_launch_without_changing_first(self):
        first = self.owner()
        a = self.session(first)
        protected = self.model.frame
        self.model.frame = Frame(self.model)
        second = self.owner()
        with (
            patch.object(extension, "executable", return_value="editor"),
            patch.object(extension.subprocess, "Popen") as launch,
        ):
            second.doVerb(0)
        launch.assert_not_called()
        self.assertIsNone(second.session)
        self.assertTrue(a["guard"].owns(first, a))
        self.assertEqual(len(a["guard"].sessions), 1)
        self.assertEqual(protected.locks, 3)
        self.assertEqual(self.model.frame.locks, 2)
        self.assertIn("other LibreOffice window", str(self.errors.call_args.args[1]))

    def test_true_duty_uses_original_scope_and_survives_cancel_without_loop(self):
        for kind, command in (("model", ".uno:CloseDoc"), ("frame", ".uno:CloseWin")):
            with self.subTest(kind=kind):
                self.model = Model()
                owner = self.owner()
                session = self.session(owner)
                guard = session["guard"]
                source = self.model if kind == "model" else self.model.frame
                self.vetoed(source, True)
                self.model.modified = True  # Edited after the initial request.
                owner._finish(session)
                self.assertEqual(self.model.frame.dispatches, [])
                self.assertIn(kind, guard.obligations)
                self.pump()
                self.assertTrue(self.model.modified)  # No implicit store or clear-modified.
                self.assertEqual(self.model.frame.dispatches, [command])
                self.model.frame.results[0].dispatchFinished(SimpleNamespace(State=0))
                self.pump()
                self.assertIn(kind, guard.obligations)
                self.assertEqual(self.model.frame.dispatches, [command])
                source.closed()
                self.assertNotIn(kind, guard.obligations)

    def test_new_true_request_after_cancel_gets_one_new_source_correct_retry(self):
        for kind, command in (("model", ".uno:CloseDoc"), ("frame", ".uno:CloseWin")):
            with self.subTest(kind=kind):
                self.model = Model()
                source = self.model if kind == "model" else self.model.frame
                first = self.owner()
                a = self.session(first)
                self.vetoed(source, True)
                first._finish(a)
                self.pump()
                self.model.frame.results[0].dispatchFinished(SimpleNamespace(State=0))
                self.pump()
                self.assertEqual(self.model.frame.dispatches, [command])
                second = self.owner()
                b = self.session(second)
                self.assertIs(a["guard"], b["guard"])
                self.vetoed(source, True)
                second._finish(b)
                self.pump()
                self.assertEqual(self.model.frame.dispatches, [command, command])
                self.model.frame.results[1].dispatchFinished(SimpleNamespace(State=0))
                self.pump()
                self.assertEqual(self.model.frame.dispatches, [command, command])
                source.closed()

    def test_retained_frame_duty_does_not_block_later_view_or_invalidate_its_session(self):
        first = self.owner()
        a = self.session(first)
        old_frame = self.model.frame
        self.vetoed(old_frame, True)
        first._finish(a)
        self.pump()
        old_frame.results[0].dispatchFinished(SimpleNamespace(State=0))
        self.model.frame = Frame(self.model)
        second = self.owner()
        b = self.session(second, running=True)
        self.assertIsNot(a["guard"], b["guard"])
        self.assertIn(a["guard"], extension.RETIRED_HOST_GUARDS)
        self.assertEqual(extension.HOST_GUARDS, [b["guard"]])
        self.assertEqual(a["guard"].obligations, {"frame": True})
        old_frame.query(False)  # Its idle guard must not veto the new frame's edit.
        self.vetoed(self.model)
        self.vetoed(self.model.frame)
        old_frame.closed(disposal=True)
        self.assertEqual(extension.RETIRED_HOST_GUARDS, [])
        self.assertEqual(extension.HOST_GUARDS, [b["guard"]])
        self.assertTrue(b["guard"].owns(second, b))
        self.assertFalse(b["invalid"])
        self.assertEqual(self.model.frame.locks, 3)
        self.assertEqual(self.model.frame.dispatches, [])

    def test_new_true_before_old_cancel_callback_retries_without_stale_result_interference(self):
        for kind, command in (("model", ".uno:CloseDoc"), ("frame", ".uno:CloseWin")):
            with self.subTest(kind=kind):
                self.model = Model()
                source = self.model if kind == "model" else self.model.frame
                first = self.owner()
                a = self.session(first)
                self.vetoed(source, True)
                first._finish(a)
                self.pump()
                old_result = self.model.frame.results[0]
                second = self.owner()
                b = self.session(second)
                self.vetoed(source, True)
                second._finish(b)
                self.pump()
                self.assertEqual(self.model.frame.dispatches, [command])
                old_result.dispatchFinished(SimpleNamespace(State=0))
                self.pump()
                self.assertEqual(self.model.frame.dispatches, [command, command])
                new_result = self.model.frame.results[1]
                old_result.dispatchFinished(SimpleNamespace(State=0))
                self.assertIs(b["guard"].inflight[kind], new_result)
                new_result.dispatchFinished(SimpleNamespace(State=0))
                self.pump()
                self.assertEqual(self.model.frame.dispatches, [command, command])
                source.closed()

    def test_multiple_retained_frames_keep_their_own_duties_without_redirecting(self):
        frames, guards = [], []
        for _ in range(3):
            owner = self.owner()
            session = self.session(owner)
            frame = self.model.frame
            self.vetoed(frame, True)
            owner._finish(session)
            self.pump()
            frame.results[0].dispatchFinished(SimpleNamespace(State=0))
            frames.append(frame)
            guards.append(session["guard"])
            self.model.frame = Frame(self.model)
        active = self.owner()
        current = self.session(active, running=True)
        self.assertEqual(extension.RETIRED_HOST_GUARDS, guards)
        for frame, guard in zip(frames, guards):
            frame.controller = object()  # A later unrelated controller, never a retry target.
            guard._settle()
            self.pump()
            self.assertEqual(frame.dispatches, [".uno:CloseWin"])
            frame.closed(disposal=True)
            self.assertTrue(current["guard"].owns(active, current))
        self.assertEqual(extension.HOST_GUARDS, [current["guard"]])
        self.assertEqual(extension.RETIRED_HOST_GUARDS, [])

    def test_actual_close_query_feedback_is_queued_deduplicated_and_cannot_suppress_veto(self):
        owner = self.owner()
        session = self.session(owner)
        self.vetoed(self.model)
        self.vetoed(self.model.frame)
        self.errors.assert_not_called()
        self.assertEqual(len(self.queue), 1)
        self.pump()
        self.errors.assert_called_once()
        with patch.object(extension, "post", side_effect=RuntimeError("callback unavailable")):
            self.vetoed(self.model)
        self.assertTrue(session["guard"].owns(owner, session))
        self.assertFalse(session["guard"].notice_pending)

    def test_dispatch_success_or_start_center_does_not_discharge_frame_ownership(self):
        owner = self.owner()
        session = self.session(owner)
        guard = session["guard"]
        self.vetoed(self.model.frame, True)
        owner._finish(session)
        self.pump()
        self.model.frame.controller = object()  # Start Center or a different document.
        self.model.frame.results[0].dispatchFinished(SimpleNamespace(State=1))
        guard._settle()
        self.pump()
        self.assertIn("frame", guard.obligations)
        self.assertTrue(guard.listeners["frame"].alive)
        self.assertEqual(self.model.frame.dispatches, [".uno:CloseWin"])
        self.model.frame.closed(disposal=True)
        self.assertEqual(guard.obligations, {})

    def test_retry_save_error_keeps_duty_and_does_not_spin(self):
        owner = self.owner()
        session = self.session(owner)
        self.vetoed(self.model, True)
        self.model.frame.dispatch_error = "save failed"
        owner._finish(session)
        self.pump()
        self.assertEqual(session["guard"].obligations, {"model": True})
        self.assertEqual(self.model.frame.dispatches, [".uno:CloseDoc"])
        self.assertEqual(self.queue, [])

    def test_simultaneous_model_and_frame_duties_do_not_prompt_again_after_cancel(self):
        owner = self.owner()
        session = self.session(owner)
        guard = session["guard"]
        self.vetoed(self.model.frame, True)
        self.vetoed(self.model, True)
        owner._finish(session)
        self.pump()
        self.assertEqual(self.model.frame.dispatches, [".uno:CloseDoc"])
        self.model.frame.results[0].dispatchFinished(SimpleNamespace(State=0))
        guard._settle()
        self.pump()
        self.assertEqual(self.model.frame.dispatches, [".uno:CloseDoc"])
        self.assertEqual(guard.obligations, {"frame": True, "model": True})

    def test_queued_frame_retry_rechecks_later_model_request_before_dispatch(self):
        first = self.owner()
        a = self.session(first)
        self.vetoed(self.model.frame, True)
        first._finish(a)  # A CloseWin retry is queued but has not run.
        self.assertEqual(a["guard"].scheduled, {"frame"})
        second = self.owner()
        b = self.session(second)
        self.assertIs(a["guard"], b["guard"])
        self.vetoed(self.model, True)
        second._finish(b)
        self.pump()
        self.assertEqual(self.model.frame.dispatches, [".uno:CloseDoc"])
        self.assertEqual(b["guard"].obligations, {"frame": True, "model": True})
        self.model.frame.results[0].dispatchFinished(SimpleNamespace(State=0))
        self.pump()
        self.assertEqual(self.model.frame.dispatches, [".uno:CloseDoc"])

    def test_unlock_remembered_self_close_still_vetoes_after_last_token_removed(self):
        owner = self.owner()
        session = self.session(owner)
        guard = session["guard"]

        def remembered_close():
            self.assertEqual(guard.sessions, {})
            self.assertEqual(guard.phase, "TEARING_DOWN")
            self.vetoed(self.model.frame, True)

        self.model.frame.on_unlock = remembered_close
        owner._finish(session)
        self.assertEqual(self.model.frame.locks, 2)
        self.assertEqual(guard.obligations, {"frame": False})
        self.assertEqual(self.model.frame.dispatches, [])

    def test_partial_acquisition_and_spawn_failure_balance_only_owned_resources(self):
        for failure in ("model listener", "frame listener", "lock", "spawn"):
            with self.subTest(failure=failure):
                self.model = Model()
                self.model.fail_add = failure == "model listener"
                self.model.frame.fail_add = failure == "frame listener"
                self.model.frame.fail_lock = failure == "lock"
                owner = self.owner()
                with (
                    patch.object(extension, "executable", return_value="editor"),
                    patch.object(
                        extension.subprocess, "Popen", side_effect=OSError("spawn failed")
                    ) as launch,
                ):
                    owner.doVerb(0)
                self.assertIsNone(owner.session)
                self.assertEqual(self.model.listeners, [])
                self.assertEqual(self.model.frame.listeners, [])
                self.assertEqual(self.model.frame.locks, 2)
                self.assertEqual(extension.HOST_GUARDS, [])
                self.assertEqual(launch.call_count, int(failure == "spawn"))

    def test_ambiguous_unlock_failure_never_retries_or_claims_edit_action_can_recover(self):
        for decremented in (False, True):
            with self.subTest(decremented=decremented):
                self.model = Model()
                owner = self.owner()
                session = self.session(owner)
                guard = session["guard"]
                frame = self.model.frame

                def fail_unlock():
                    frame.removes += 1
                    if decremented:
                        frame.locks -= 1
                    raise RuntimeError("ambiguous native unlock failure")

                with patch.object(frame, "removeActionLock", side_effect=fail_unlock):
                    owner._finish(session)
                    self.pump()
                    self.assertEqual(guard.phase, "TEARING_DOWN")
                    self.assertEqual(guard.sessions, {})
                    self.assertIn("Save your work", guard.unlock_error)
                    self.assertIn("Edit in ReShiki cannot clear", guard.unlock_error)
                    self.assertEqual(frame.locks, 2 if decremented else 3)
                    with (
                        patch.object(extension, "executable", return_value="editor"),
                        patch.object(extension.subprocess, "Popen") as launch,
                    ):
                        owner.doVerb(0)
                    launch.assert_not_called()
                    self.assertIsNone(owner.session)
                    self.assertIn(
                        "Edit in ReShiki cannot clear", str(self.errors.call_args.args[1])
                    )
                    self.vetoed(self.model)
                    self.vetoed(frame)
                    guard._unprotect()
                    self.model.closed(disposal=True)
                    self.assertEqual(frame.removes, 1)
                    self.assertEqual(frame.locks, 2 if decremented else 3)
                    frame.closed(disposal=True)
                self.assertEqual(frame.removes, 1)
                self.assertNotIn(guard, extension.HOST_GUARDS)
                self.pump()

    def test_watcher_start_failure_explicitly_recovers_live_child_without_saveback(self):
        owner = self.owner()
        process = Process(running=True)
        with (
            patch.object(extension, "executable", return_value="editor"),
            patch.object(extension.subprocess, "Popen", return_value=process),
            patch.object(extension.threading, "Thread") as thread,
        ):
            thread.return_value.start.side_effect = RuntimeError("cannot start watcher")
            owner.doVerb(0)
            session = thread.call_args.kwargs["args"][0]
        self.assertIsNone(owner.session)
        self.assertTrue(session["invalid"])
        self.assertIsNone(process.poll())
        self.assertTrue(session["path"].exists())
        self.addCleanup(extension.shutil.rmtree, session["directory"])
        self.assertEqual(self.model.frame.locks, 2)
        self.assertIn("will not update LibreOffice", str(self.errors.call_args.args[1]))
        with patch.object(owner, "_write") as write:
            owner._accept(session, self.new, threading.Event())
        write.assert_not_called()
        self.assertFalse((session["directory"] / "accepted.json").exists())

    def test_finish_with_running_child_keeps_protection_even_after_error(self):
        owner = self.owner()
        session = self.session(owner, running=True)
        session["error"] = "failed save"
        owner._finish(session)
        self.assertIs(owner.session, session)
        self.vetoed(self.model.frame)
        self.assertTrue(session["directory"].exists())

    def test_render_failure_waits_for_actual_exit_then_preserves_recovery(self):
        owner = self.owner()
        session = self.session(owner, running=True)
        session["path"].write_bytes(self.new[0])

        def exit_after_protection_check(delay):
            self.pump()
            self.assertEqual(len(self.recovery_notices()), 1)
            self.assertIn(str(session["path"]), self.recovery_notices()[0])
            self.assertIsNone(session["process"].poll())
            self.vetoed(self.model)
            self.assertIs(owner.session, session)
            session["process"].running = False

        with (
            patch.object(extension, "worker", side_effect=RuntimeError("render failed")),
            patch.object(extension.time, "sleep", side_effect=exit_after_protection_check),
        ):
            owner._watch(session, "editor")
        self.vetoed(self.model)
        self.pump()
        self.assertIsNone(owner.session)
        self.assertEqual(session["path"].read_bytes(), self.new[0])
        self.assertIn("render failed", session["error"])
        self.assertEqual(self.model.frame.locks, 2)
        self.assertEqual(len(self.recovery_notices()), 1)

    def test_accept_failure_with_live_child_keeps_guard_until_exit(self):
        owner = self.owner()
        session = self.session(owner, running=True)
        session["path"].write_bytes(self.new[0])

        def exit_after_veto(delay):
            self.assertIn("host rejected", session["error"])
            self.assertEqual(len(self.recovery_notices()), 1)
            self.assertIsNone(session["process"].poll())
            self.vetoed(self.model.frame)
            session["process"].running = False

        with (
            patch.object(extension, "post", side_effect=lambda ctx, action: action()),
            patch.object(extension, "worker", return_value={}),
            patch.object(extension, "packet", return_value=self.new),
            patch.object(owner, "_write", side_effect=RuntimeError("host rejected")),
            patch.object(extension.time, "sleep", side_effect=exit_after_veto),
        ):
            owner._watch(session, "editor")
        self.assertEqual(session["path"].read_bytes(), self.new[0])
        self.assertFalse((session["directory"] / "accepted.json").exists())
        self.assertEqual(self.model.frame.locks, 2)
        self.assertEqual(len(self.recovery_notices()), 1)

    def test_transient_final_enqueue_failure_retries_then_releases_on_ui_callback(self):
        owner = self.owner()
        session = self.session(owner)
        attempts = []

        def enqueue(ctx, action):
            attempts.append(action)
            if len(attempts) == 1:
                raise RuntimeError("transient callback failure")
            self.queue.append(action)

        with (
            patch.object(extension, "post", side_effect=enqueue),
            patch.object(extension.time, "sleep"),
        ):
            owner._watch(session, "editor")
        self.assertEqual(len(attempts), 2)
        self.assertTrue(session["watch_complete"])
        self.assertIs(owner.session, session)
        self.assertEqual(self.model.frame.locks, 3)
        self.pump()
        self.assertIsNone(owner.session)
        self.assertEqual(self.model.frame.locks, 2)
        self.assertTrue(session["path"].exists())
        self.assertEqual(len(self.recovery_notices()), 1)

    def test_persistent_final_enqueue_failure_is_reaped_by_later_exact_ui_edit_action(self):
        owner = self.owner()
        session = self.session(owner)
        with (
            patch.object(
                extension, "post", side_effect=RuntimeError("callbacks unavailable")
            ) as enqueue,
            patch.object(extension.time, "sleep"),
        ):
            owner._watch(session, "editor")
        self.assertEqual(enqueue.call_count, 3)
        self.assertTrue(session["watch_complete"])
        self.assertEqual(session["completions"], set())
        self.assertEqual(self.model.frame.locks, 3)
        with patch.object(extension.subprocess, "Popen") as launch:
            owner.doVerb(0)
        launch.assert_not_called()
        self.assertIsNone(owner.session)
        self.assertEqual(self.model.frame.locks, 2)
        self.assertTrue(session["path"].exists())
        self.assertEqual(len(self.recovery_notices()), 1)

    def test_ui_reaping_never_releases_before_pending_final_acceptance(self):
        owner = self.owner()
        session = self.session(owner)
        session["watch_complete"] = True
        done = owner._queue_accept(session, self.new)
        owner.doVerb(0)
        self.assertIs(owner.session, session)
        self.assertFalse(done.is_set())
        self.assertTrue(session["guard"].owns(owner, session))
        self.vetoed(self.model)

    def test_final_acceptance_finishes_before_host_guard_is_released(self):
        owner = self.owner()
        session = self.session(owner)
        session["path"].write_bytes(self.new[0])

        def store(parent, entry):
            self.assertTrue(session["guard"].owns(owner, session))
            self.vetoed(self.model)
            self.vetoed(self.model.frame)

        with (
            patch.object(extension, "post", side_effect=lambda ctx, action: action()),
            patch.object(extension, "worker", return_value={}),
            patch.object(extension, "packet", return_value=self.new),
            patch.object(owner, "_write", side_effect=store),
        ):
            owner._watch(session, "editor")
        self.assertEqual(session["accepted"], self.new[0])
        self.assertIsNone(session["error"])
        self.assertIsNone(owner.session)
        self.assertEqual(self.model.frame.locks, 2)

    def test_disposal_before_callback_unblocks_waiter_and_rejects_save_as_deferral(self):
        for kind in ("model", "frame"):
            with self.subTest(kind=kind):
                self.model = Model()
                owner = self.owner()
                session = self.session(owner, running=True)
                done = owner._queue_accept(session, self.new)
                self.assertFalse(done.is_set())
                owner.pending = ("new storage", "New Object")
                source = self.model if kind == "model" else self.model.frame
                source.closed(disposal=True)
                self.assertTrue(done.is_set())
                self.assertEqual(session["completions"], set())
                with patch.object(owner, "_write") as write:
                    self.pump()
                write.assert_not_called()
                self.assertIsNone(owner.deferred_update)
                self.assertFalse((session["directory"] / "accepted.json").exists())
                self.assertTrue(session["path"].exists())

    def test_disposal_during_save_as_deferral_clears_and_completes_it(self):
        owner = self.owner()
        session = self.session(owner)
        owner.pending = ("new storage", "New Object")
        done = owner._queue_accept(session, self.new)
        self.pump()
        self.assertIsNotNone(owner.deferred_update)
        self.assertFalse(done.is_set())
        self.model.closed(disposal=True)
        self.assertTrue(done.is_set())
        self.assertIsNone(owner.deferred_update)
        with patch.object(owner, "_write") as write:
            owner.saveCompleted(True)
            self.pump()
        write.assert_not_called()

    def test_reentrant_disposal_after_storage_commit_never_acks_or_rolls_back_dead_storage(self):
        owner = self.owner()
        session = self.session(owner)

        def save_and_dispose():
            owner.storeOwn()
            self.model.closed(disposal=True)

        owner.client.saveObject = save_and_dispose
        done = threading.Event()
        with patch.object(owner, "_write") as write:
            owner._accept(session, self.new, done)
        write.assert_called_once_with("storage", "Object 1")
        self.assertTrue(done.is_set())
        self.assertEqual(session["accepted"], self.old[0])
        self.assertFalse((session["directory"] / "accepted.json").exists())

    def test_stale_finish_and_same_token_copy_cannot_release_replacement_session(self):
        owner = self.owner()
        old = self.session(owner)
        owner._finish(old)
        new = self.session(owner)
        owner._finish(old)
        copied = dict(new)
        new["guard"].release(owner, copied)
        self.assertTrue(new["guard"].owns(owner, new))
        self.vetoed(self.model)


if __name__ == "__main__":
    unittest.main()
