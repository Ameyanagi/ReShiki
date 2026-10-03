"""Early frame-close prevention, with the pinned Sfx PrepareClose cache modeled.

This covers frame dispatches. Native macOS Quit calls Desktop.terminate directly
and is covered separately by the deferred-choice regression suite.
"""

import threading
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

import test_host_guard as host_tests

extension = host_tests.extension


def url(command):
    value = extension.uno.createUnoStruct("com.sun.star.util.URL")
    value.Complete, value.Protocol, value.Path = command, ".uno:", command[5:]
    return value


class NativeCloseDispatch:
    """Only the source-proven cache/choice/late-veto ordering, not an LO process."""

    def __init__(self, model, owner):
        self.model, self.owner = model, owner
        self.prepared, self.suspended, self.closed = False, False, False
        self.disk = owner.native
        self.decisions, self.dialogs, self.calls = [], [], []
        self.status = []

    def dispatch(self, command, args):
        self.dispatchWithNotification(command, args, None)

    def dispatchWithNotification(self, command, args, listener):
        self.calls.append((command.Complete, args))
        success = True
        if command.Complete.startswith(".uno:Close") or command.Complete == ".uno:Quit":
            # SfxObjectShell::PrepareClose returns immediately for this flag,
            # even if a later edit has set the document's modified flag again.
            if not self.prepared:
                if self.model.modified:
                    self.dialogs.append(self.owner.native)
                    decision = self.decisions.pop(0)
                    if decision in ("cancel", "save failure"):
                        success = False
                    elif decision == "save":
                        self.disk = self.owner.native
                        self.model.modified = False
                if success:
                    self.prepared = True
            if success:
                self.suspended = True
                try:
                    self.model.frame.query(False)
                    self.model.query(False)
                except extension.CloseVetoException:
                    success = False
                    self.suspended = False  # suspend(false) does not reset prepared.
                else:
                    self.closed = True
                    self.model.closed(disposal=True)
                    self.model.frame.closed(disposal=True)
        if listener:
            listener.dispatchFinished(SimpleNamespace(State=1 if success else 0, Result=success))

    def addStatusListener(self, listener, command):
        self.status.append(("add", listener, command))

    def removeStatusListener(self, listener, command):
        self.status.append(("remove", listener, command))


class CloseDispatchTests(unittest.TestCase):
    setUp = host_tests.HostGuardTests.setUp
    owner = host_tests.HostGuardTests.owner
    session = host_tests.HostGuardTests.session
    pump = host_tests.HostGuardTests.pump

    def prepare(self):
        owner = self.owner()
        native = NativeCloseDispatch(self.model, owner)
        self.model.frame.native_provider = SimpleNamespace(queryDispatch=lambda *args: native)
        session = self.session(owner)
        return owner, session, native

    def accept(self, owner, session, data):
        done = threading.Event()
        with patch.object(owner, "_write"):
            owner._accept(session, data, done)
        self.assertTrue(done.is_set())
        self.assertIsNone(session["error"])
        self.assertEqual(session["accepted"], data[0])

    def dispatch(self, command=".uno:CloseWin", listener=None, target="_self", args=()):
        command = url(command)
        dispatch = self.model.frame.queryDispatch(command, target, 0)
        if listener is None:
            dispatch.dispatch(command, args)
        else:
            dispatch.dispatchWithNotification(command, args, listener)
        return dispatch

    def test_save_after_active_close_then_second_ack_requires_fresh_save_for_current_content(self):
        owner, session, native = self.prepare()
        native.decisions = ["save", "save"]
        self.accept(owner, session, self.new)
        self.dispatch()
        active_dialogs = list(native.dialogs)
        self.assertFalse(native.closed)
        second = (b"second accepted drawing", b"second PNG", (500, 400))
        self.accept(owner, session, second)
        owner._finish(session)
        self.dispatch()
        # Without early interception the first Save is cached and this remains
        # the first ACK, reproducing the native P2 loss despite modified=True.
        self.assertEqual(native.disk, second[0])
        self.assertEqual(active_dialogs, [])
        self.assertEqual(native.dialogs, [second[0]])
        self.assertTrue(native.closed)
        self.assertEqual(self.model.frame.interceptors, [])

    def test_post_finish_save_discard_cancel_and_save_failure_keep_native_decisions(self):
        for decision in ("save", "discard", "cancel", "save failure"):
            with self.subTest(decision=decision):
                self.model = host_tests.Model()
                owner, session, native = self.prepare()
                self.accept(owner, session, self.new)
                self.dispatch()
                owner._finish(session)
                native.decisions = [decision]
                self.dispatch()
                self.assertEqual(native.dialogs, [self.new[0]])
                self.assertEqual(native.closed, decision in ("save", "discard"))
                self.assertEqual(native.disk, self.new[0] if decision == "save" else self.old[0])
                self.assertEqual(self.model.modified, decision != "save")
                if decision in ("cancel", "save failure"):
                    self.assertFalse(native.prepared)
                    native.decisions = ["save"]
                    self.dispatch()
                    self.assertEqual(native.dialogs, [self.new[0], self.new[0]])
                    self.assertEqual(native.disk, self.new[0])

    def test_owned_frame_close_commands_stop_before_native_prepare_and_do_not_take_ownership(self):
        owner, session, native = self.prepare()
        for command in (".uno:CloseWin", ".uno:CloseDoc", ".uno:CloseFrame", ".uno:Quit"):
            for target in ("", "_self", "_top"):
                with self.subTest(command=command, target=target):
                    listener = SimpleNamespace(dispatchFinished=Mock())
                    self.dispatch(command, listener, target)
                    self.assertEqual(listener.dispatchFinished.call_args.args[0].State, 0)
        self.assertEqual(native.calls, [])
        self.assertFalse(native.prepared)
        self.assertEqual(session["guard"].obligations, {})
        self.pump()
        self.errors.assert_called_once()
        owner._finish(session)
        self.pump()
        self.assertEqual(native.calls, [])  # A blocked FALSE UI intent is not replayed.

    def test_unrelated_commands_named_targets_status_and_arguments_forward_unchanged(self):
        owner, session, native = self.prepare()
        command = url(".uno:Save")
        args = (extension.prop("SynchronMode", True),)
        self.assertIs(self.model.frame.queryDispatch(command, "_self", 7), native)
        self.dispatch(".uno:Save", args=args)
        self.assertEqual(native.calls, [(".uno:Save", args)])
        self.assertIs(self.model.frame.queryDispatch(url(".uno:CloseWin"), "OtherView", 7), native)
        wrapper = self.model.frame.queryDispatch(url(".uno:CloseWin"), "_self", 0)
        listener = object()
        wrapper.addStatusListener(listener, command)
        wrapper.removeStatusListener(listener, command)
        self.assertEqual(native.status, [("add", listener, command), ("remove", listener, command)])
        owner._finish(session)

    def test_cached_wrapper_rechecks_live_guard_and_forwards_after_last_editor(self):
        owner, session, native = self.prepare()
        wrapper = self.model.frame.queryDispatch(url(".uno:CloseWin"), "_self", 0)
        wrapper.dispatch(url(".uno:CloseWin"), ())
        self.assertEqual(native.calls, [])
        owner._finish(session)
        listener = SimpleNamespace(dispatchFinished=Mock())
        wrapper.dispatchWithNotification(url(".uno:CloseWin"), (), listener)
        self.assertTrue(native.closed)
        self.assertEqual(listener.dispatchFinished.call_args.args[0].State, 1)

    def test_shared_tokens_keep_one_interceptor_until_last_editor_and_true_retry_still_runs(self):
        owner, first, native = self.prepare()
        other = self.owner()
        second = self.session(other)
        self.assertEqual(self.model.frame.interceptor_adds, 1)
        with self.assertRaises(extension.CloseVetoException):
            self.model.frame.query(True)
        owner._finish(first)
        self.assertEqual(self.model.frame.interceptor_removes, 0)
        self.dispatch()
        self.assertEqual(native.calls, [])
        other._finish(second)
        self.assertEqual(self.model.frame.interceptor_removes, 0)
        self.pump()
        self.assertEqual(len(native.calls), 1)
        self.assertEqual(native.calls[0][0], ".uno:CloseWin")
        self.assertEqual(native.calls[0][1][0].Name, "SynchronMode")
        self.assertTrue(native.calls[0][1][0].Value)
        self.assertEqual(self.model.frame.interceptor_removes, 1)

    def test_controller_replaced_in_original_frame_does_not_block_unrelated_document(self):
        owner, session, native = self.prepare()
        self.model.frame.controller = SimpleNamespace(getModel=lambda: object())
        # Use a harmless delegate to isolate targeting from the late close guard.
        delegate = SimpleNamespace(dispatchWithNotification=Mock())
        wrapper = extension.GuardedCloseDispatch(session["guard"].active_view, delegate)
        wrapper.dispatch(url(".uno:CloseWin"), ())
        delegate.dispatchWithNotification.assert_called_once()
        self.errors.assert_not_called()
        owner._finish(session)

    def test_view_cleanup_failure_retains_draft_and_blocks_new_edit_without_retry(self):
        other = host_tests.Frame(self.model)
        owner, session, native = self.prepare()
        self.accept(owner, session, self.new)
        session["path"].write_bytes(self.new[0])
        frame = self.model.frame
        with self.assertRaises(extension.CloseVetoException):
            frame.query(False)
        owner._finish(session)
        frame.fail_interceptor_remove = True
        frame.closed(disposal=True)
        self.model.frame = other
        guard = session["guard"]
        self.assertIsNone(owner.session)
        self.assertEqual(session["path"].read_bytes(), self.new[0])
        self.assertIn("protect this document", guard.unlock_error)
        self.assertEqual(frame.interceptor_removes, 1)
        with self.assertRaises(extension.CloseVetoException):
            self.model.query(False)
        with self.assertRaises(extension.CloseVetoException):
            other.query(False)
        guard._unprotect()
        self.assertEqual(frame.interceptor_removes, 1)
        self.assertEqual(native.calls, [])


if __name__ == "__main__":
    unittest.main()
