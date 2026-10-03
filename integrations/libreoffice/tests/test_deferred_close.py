"""Source-backed native Quit/cache/consent regressions without a running desktop.

The host double follows the pinned native suspend -> PrepareClose -> frame close
ordering and the separate CloseWin backing transition. It does not claim native
UI results. Production extension classes and installed UNO types are exercised.
"""

import threading
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

import test_host_guard as host_tests

extension = host_tests.extension


class NativeHost:
    def __init__(self, model, owner):
        self.model, self.owner = model, owner
        self.disk, self.prepared = owner.native, False
        self.native_choices, self.save_outcomes = [], []
        self.dialogs, self.calls, self.closed_frames = [], [], []
        self.suspended = {}
        self.in_native = self.closed = False
        self.backing = self.no_close_result = False
        self.before_close = self.after_save = self.after_view = None
        self.bind(model.frame)

    def bind(self, frame):
        frame.locks = 0
        frame.controller.suspend = lambda value: self.suspend(frame, value)
        delegate = SimpleNamespace(
            dispatch=lambda url, args: self.dispatch(frame, url, args, None),
            dispatchWithNotification=lambda url, args, result: self.dispatch(
                frame, url, args, result
            ),
        )
        frame.native_provider = SimpleNamespace(queryDispatch=lambda *args: delegate)
        return frame

    def frames(self):
        return [controller.getFrame() for controller in self.model.controllers]

    def save(self, frame):
        if self.suspended.get(frame):
            raise RuntimeError("Native Save dispatcher is locked while suspended")
        outcome = self.save_outcomes.pop(0) if self.save_outcomes else "save"
        if outcome == "error":
            raise RuntimeError("Native Save write failure")
        if outcome in ("cancel", "false", "failure", "missing", "none"):
            return outcome
        self.disk = self.owner.native
        if not self.model.URL or getattr(self.model, "readonly", False):
            self.model.URL = "file:///chosen-save-as.odt"
        self.model.setModified(outcome == "dirty")
        if self.after_save:
            self.after_save()
        return outcome

    def suspend(self, frame, value):
        if not value:
            self.suspended[frame] = False
            return True
        if len(self.frames()) == 1 and not self.prepared:
            if self.model.modified:
                self.dialogs.append(self.owner.native)
                decision = self.native_choices.pop(0) if self.native_choices else "discard"
                if decision == "cancel":
                    return False
                if decision == "save":
                    # PrepareClose's own synchronous Save happens before suspend.
                    if self.save(frame) != "save":
                        return False
            self.prepared = True
        self.suspended[frame] = True
        return True

    def close_frame(self, frame, backing=False):
        controller = frame.controller
        if not backing:
            frame.query(False)
        if frame.locks:
            raise extension.CloseVetoException("Native action lock", None)
        # Controller.dispose emits OnViewClosed even when backing mode skipped
        # the frame's XCloseListener and leaves that frame alive for Start Center.
        self.model.event("OnViewClosed", controller)
        if controller in self.model.controllers:
            self.model.controllers.remove(controller)
        frame.controller = None
        self.closed_frames.append(frame)
        if self.after_view:
            self.after_view(frame)
        if not self.model.controllers:
            self.model.query(False)
            self.model.closed(disposal=True)
            self.closed = True
        if not backing:
            frame.closed(disposal=True)

    def native_quit(self, only=None):
        self.in_native = True
        try:
            for frame in (only,) if only is not None else tuple(self.frames()):
                if self.suspend(frame, True):
                    try:
                        self.close_frame(frame)
                    except extension.CloseVetoException:
                        self.suspend(frame, False)
        finally:
            self.in_native = False

    def dispatch(self, frame, url, args, listener):
        command = url.Complete
        self.calls.append((frame, command, args))
        state, result = extension.SUCCESS, True
        if command == ".uno:Save":
            outcome = self.save(frame)
            if outcome == "missing":
                return
            result = outcome not in ("cancel", "false", "failure", "none")
            if outcome == "none":
                result = None
            if outcome == "failure":
                state = extension.FAILURE
        elif command.startswith(".uno:Close"):
            try:
                if self.before_close:
                    self.before_close()
                if command == ".uno:CloseDoc":
                    for other in tuple(self.frames()):
                        if other is not frame:
                            self.close_frame(other)
                if self.suspend(frame, True):
                    self.close_frame(frame, self.backing)
                else:
                    state, result = extension.FAILURE, False
            except extension.CloseVetoException:
                self.suspend(frame, False)
                state, result = extension.FAILURE, False
            if self.no_close_result:
                return
        if listener is not None:
            listener.dispatchFinished(SimpleNamespace(State=state, Result=result))


class DeferredCloseTests(unittest.TestCase):
    setUp = host_tests.HostGuardTests.setUp
    owner = host_tests.HostGuardTests.owner
    session = host_tests.HostGuardTests.session
    pump = host_tests.HostGuardTests.pump
    vetoed = host_tests.HostGuardTests.vetoed

    def prepare(self, prior="save"):
        owner = self.owner()
        native = NativeHost(self.model, owner)
        session = self.session(owner)
        self.accept(owner, session, self.new)
        native.native_choices = [prior]
        native.native_quit()
        return owner, session, native

    def accept(self, owner, session, data):
        done = threading.Event()
        with patch.object(owner, "_write"):
            owner._accept(session, data, done)
        self.assertTrue(done.is_set())
        self.assertIsNone(session["error"])
        self.assertEqual(session["accepted"], data[0])
        session["path"].write_bytes(data[0])

    def finish_second(self, owner, session):
        second = (b'{"version":15,"title":"second"}', b"second PNG", (500, 400))
        self.accept(owner, session, second)
        owner._finish(session)
        return second

    def close(self, frame=None, command=".uno:CloseWin", pump=True):
        frame = frame or self.model.frame
        url = extension.command_url(command)
        frame.queryDispatch(url, "_self", 0).dispatch(url, ())
        if pump:
            self.pump()

    def close_commands(self, native):
        return [command for _, command, _ in native.calls if command.startswith(".uno:Close")]

    def test_native_quit_save_or_discard_then_second_ack_requires_fresh_final_choice(self):
        for prior in ("save", "discard"):
            for final in ("save", "discard", "cancel"):
                with self.subTest(prior=prior, final=final):
                    self.model = host_tests.Model()
                    owner, session, native = self.prepare(prior)
                    guard = session["guard"]
                    self.assertTrue(native.prepared)
                    self.assertTrue(guard.tainted)
                    second = self.finish_second(owner, session)
                    self.pump()
                    self.assertFalse(native.closed)
                    choices_before = self.choices.call_count
                    self.choices.return_value = final
                    self.close()
                    self.assertEqual(self.choices.call_count, choices_before + 1)
                    before = self.new[0] if prior == "save" else self.old[0]
                    self.assertEqual(native.disk, second[0] if final == "save" else before)
                    self.assertEqual(native.closed, final != "cancel")
                    self.assertIsNone(guard.permit)
                    self.assertFalse(guard.in_choice)
                    if final == "cancel":
                        self.assertTrue(guard.tainted)
                        self.assertTrue(session["path"].exists())
                        self.assertTrue(self.model.modified)
                        self.model.closed(disposal=True)
                    else:
                        self.assertFalse(session["directory"].exists())
                        self.assertNotIn(guard, extension.HOST_GUARDS)
                        self.assertEqual(self.model.document_events, [])
                        self.assertEqual(self.model.modify_events, [])
                        self.assertEqual(guard.views, [])
                        self.assertIsNone(guard.model)

    def test_false_quit_veto_never_replays_when_child_finishes(self):
        owner, session, native = self.prepare()
        self.finish_second(owner, session)
        self.pump()
        self.choices.assert_not_called()
        self.assertEqual(self.close_commands(native), [])
        self.assertTrue(session["path"].exists())
        self.assertEqual(session["guard"].obligations, {})

    def test_native_cancel_never_taints_and_idle_native_close_has_no_custom_choice(self):
        owner, session, native = self.prepare("cancel")
        guard = session["guard"]
        self.assertFalse(native.prepared)
        self.assertFalse(guard.tainted)
        second = self.finish_second(owner, session)
        self.assertEqual(self.model.frame.locks, 0)
        self.assertFalse(session["directory"].exists())
        native.native_choices = ["save"]
        self.close()
        self.assertTrue(native.closed)
        self.assertEqual(native.disk, second[0])
        self.choices.assert_not_called()
        self.assertNotIn(guard, extension.HOST_GUARDS)

    def test_idle_native_quit_choice_runs_after_caller_unwinds_and_resumes_controller(self):
        owner, session, native = self.prepare()
        self.finish_second(owner, session)
        frame = self.model.frame

        def choose(*args):
            self.assertFalse(native.in_native)
            self.assertFalse(native.suspended[frame])
            self.assertIs(args[2], frame)
            return "save"

        self.choices.side_effect = choose
        native.native_quit()
        self.choices.assert_not_called()
        self.assertFalse(native.suspended[frame])
        self.pump()
        self.assertTrue(native.closed)
        self.assertEqual(native.calls[0][1], ".uno:Save")

    def test_save_failure_cancel_false_missing_dirty_and_exception_keep_host_and_draft(self):
        for outcome in ("cancel", "false", "failure", "missing", "none", "dirty", "error"):
            with self.subTest(outcome=outcome):
                self.model = host_tests.Model()
                owner, session, native = self.prepare()
                self.finish_second(owner, session)
                self.choices.return_value = "save"
                native.save_outcomes = [outcome]
                self.close()
                guard = session["guard"]
                self.assertFalse(native.closed)
                self.assertTrue(session["path"].exists())
                self.assertTrue(guard.tainted)
                self.assertIsNone(guard.permit)
                self.assertFalse(guard.in_choice)
                self.assertEqual(self.close_commands(native), [])
                self.assertEqual(self.queue, [])
                self.model.closed(disposal=True)

    def test_untitled_and_readonly_save_as_changes_location_on_same_model(self):
        for readonly in (False, True):
            with self.subTest(readonly=readonly):
                self.model = host_tests.Model()
                owner, session, native = self.prepare("discard")
                self.finish_second(owner, session)
                self.model.URL = "file:///readonly.odt" if readonly else ""
                self.model.readonly = readonly
                self.choices.return_value = "save"
                self.close()
                self.assertTrue(native.closed)
                self.assertEqual(self.model.URL, "file:///chosen-save-as.odt")
                self.assertFalse(session["directory"].exists())

    def test_clean_untitled_save_as_cancel_remains_protected(self):
        owner, session, native = self.prepare("discard")
        owner._finish(session)
        self.model.URL, self.model.modified = "", False
        native.save_outcomes = ["cancel"]
        self.choices.return_value = "save"
        self.close()
        self.assertEqual(self.model.URL, "")
        self.assertFalse(native.closed)
        self.assertTrue(session["guard"].tainted)
        self.assertTrue(session["path"].exists())
        self.assertEqual(self.close_commands(native), [])

    def test_other_listener_veto_consumes_one_permit_and_requires_new_choice(self):
        owner, session, native = self.prepare()
        self.finish_second(owner, session)
        frame = self.model.frame
        other = SimpleNamespace(
            queryClosing=Mock(side_effect=extension.CloseVetoException("other", None))
        )
        frame.addCloseListener(other)
        self.close()
        self.assertFalse(native.closed)
        self.assertTrue(session["path"].exists())
        self.assertIsNone(session["guard"].permit)
        self.assertFalse(session["guard"].in_choice)
        count = self.choices.call_count
        self.pump()
        self.assertEqual(self.choices.call_count, count)
        frame.removeCloseListener(other)
        self.close()
        self.assertEqual(self.choices.call_count, count + 1)
        self.assertTrue(native.closed)

    def test_conservative_taint_allows_second_native_confirmation_and_honors_cancel(self):
        owner = self.owner()
        native = NativeHost(self.model, owner)
        session = self.session(owner)
        self.accept(owner, session, self.new)
        self.vetoed(self.model.frame)  # Direct UNO caller never called PrepareClose.
        owner._finish(session)
        self.assertFalse(native.prepared)
        native.native_choices = ["cancel", "discard"]
        self.close()
        self.assertFalse(native.closed)
        self.assertTrue(session["guard"].tainted)
        self.assertIsNone(session["guard"].permit)
        self.assertTrue(session["path"].exists())
        self.close()
        self.assertTrue(native.closed)
        self.assertEqual(native.dialogs, [self.new[0], self.new[0]])
        self.assertEqual(self.choices.call_count, 2)

    def test_frame_scope_closes_only_original_view_and_retains_model_taint_and_draft(self):
        owner, session, native = self.prepare()
        self.finish_second(owner, session)
        first = self.model.frame
        second = native.bind(host_tests.Frame(self.model))
        self.model.event("OnViewCreated", second.controller)
        self.close(first)
        self.assertEqual(native.closed_frames, [first])
        self.assertFalse(native.closed)
        self.assertIsNotNone(second.controller)
        self.assertTrue(session["guard"].tainted)
        self.assertTrue(session["path"].exists())
        self.assertFalse(self.choices.call_args.args[3])
        self.model.frame = second
        next_owner = self.owner()
        next_session = self.session(next_owner)
        self.assertIs(next_session["guard"], session["guard"])
        self.assertTrue(next_session["guard"].owns(next_owner, next_session))
        self.vetoed(second)

    def test_close_doc_expected_view_disposal_preserves_only_captured_scope(self):
        owner, session, native = self.prepare()
        self.finish_second(owner, session)
        first = self.model.frame
        second = native.bind(host_tests.Frame(self.model))
        self.model.event("OnViewCreated", second.controller)
        unrelated = host_tests.Model()
        native.after_view = lambda frame: self.assertIsNotNone(session["guard"].permit)
        self.close(first, ".uno:CloseDoc")
        self.assertTrue(native.closed)
        self.assertEqual(native.closed_frames, [second, first])
        self.assertTrue(self.choices.call_args.args[3])
        self.assertEqual(unrelated.frame.dispatches, [])
        self.assertEqual(unrelated.frame.interceptor_adds, 0)
        self.assertFalse(session["directory"].exists())

    def test_new_remaining_view_stays_guarded_after_original_view_closes(self):
        owner, session, native = self.prepare()
        second_data = self.finish_second(owner, session)
        old = self.model.frame
        new = native.bind(host_tests.Frame(self.model))
        self.model.event("OnViewCreated", new.controller)
        self.close(old)
        self.model.frame = new
        self.model.setModified(True)  # New native host edit after original view vanished.
        self.choices.return_value = "cancel"
        self.close(new)
        self.assertFalse(native.closed)
        self.assertTrue(session["guard"].tainted)
        self.assertEqual(session["path"].read_bytes(), second_data[0])
        self.choices.return_value = "save"
        self.close(new)
        self.assertTrue(native.closed)
        self.assertEqual(native.disk, second_data[0])

    def test_new_editor_during_modal_choice_is_rejected_and_invalidates_that_choice(self):
        owner, session, native = self.prepare()
        self.finish_second(owner, session)
        other = self.owner()

        def choose(*args):
            with (
                patch.object(extension, "executable", return_value="editor"),
                patch.object(extension.subprocess, "Popen") as launch,
            ):
                other.doVerb(0)
            launch.assert_not_called()
            self.assertIsNone(other.session)
            return "discard"

        self.choices.side_effect = choose
        self.close()
        self.assertFalse(native.closed)
        self.assertEqual(self.close_commands(native), [])
        self.assertIsNone(session["guard"].permit)
        self.assertTrue(session["path"].exists())

    def test_new_editor_during_native_save_aborts_close_after_verified_save(self):
        owner, session, native = self.prepare()
        self.finish_second(owner, session)
        other = self.owner()

        def new_editor():
            with (
                patch.object(extension, "executable", return_value="editor"),
                patch.object(extension.subprocess, "Popen") as launch,
            ):
                other.doVerb(0)
            launch.assert_not_called()

        native.after_save = new_editor
        self.choices.return_value = "save"
        self.close()
        self.assertFalse(native.closed)
        self.assertEqual(self.close_commands(native), [])
        self.assertTrue(session["guard"].tainted)
        self.assertTrue(session["path"].exists())

    def test_new_view_during_modal_choice_is_protected_and_revokes_old_scope(self):
        owner, session, native = self.prepare()
        self.finish_second(owner, session)
        created = []

        def choose(*args):
            frame = native.bind(host_tests.Frame(self.model))
            created.append(frame)
            self.model.event("OnViewCreated", frame.controller)
            return "discard"

        self.choices.side_effect = choose
        self.close()
        self.assertFalse(native.closed)
        self.assertEqual(self.close_commands(native), [])
        frame = created[0]
        self.assertEqual(frame.locks, 0)
        self.assertEqual(len(frame.interceptors), 1)
        self.assertEqual(len(frame.listeners), 1)
        self.assertTrue(session["path"].exists())

    def test_replaced_controller_during_modal_choice_never_closes_replacement(self):
        owner, session, native = self.prepare()
        self.finish_second(owner, session)
        replacement = SimpleNamespace(getModel=lambda: object())

        def choose(*args):
            self.model.frame.controller = replacement
            return "discard"

        self.choices.side_effect = choose
        self.close()
        self.assertIs(self.model.frame.controller, replacement)
        self.assertEqual(self.close_commands(native), [])
        self.assertTrue(session["path"].exists())
        self.assertIsNone(session["guard"].permit)

    def test_actual_modify_while_already_dirty_invalidates_discard_choice(self):
        owner, session, native = self.prepare("discard")
        self.finish_second(owner, session)
        self.assertTrue(self.model.modified)
        generation = session["guard"].content_generation

        def choose(*args):
            self.model.setModified(True)  # XModifyListener fires with dirty already true.
            return "discard"

        self.choices.side_effect = choose
        self.close()
        self.assertGreater(session["guard"].content_generation, generation)
        self.assertEqual(self.close_commands(native), [])
        self.assertTrue(session["path"].exists())

    def test_each_accepted_update_advances_generation_without_host_modified_notification(self):
        owner = self.owner()
        native = NativeHost(self.model, owner)
        session = self.session(owner)
        guard = session["guard"]
        generation = guard.content_generation
        with patch.object(
            self.model,
            "setModified",
            side_effect=lambda value: setattr(self.model, "modified", value),
        ):
            self.accept(owner, session, self.new)
            self.finish_second(owner, session)
        self.assertEqual(guard.content_generation, generation + 2)
        self.assertTrue(self.model.modified)
        self.assertEqual(native.disk, self.old[0])

    def test_taint_survives_successful_save_veto_and_further_content_change(self):
        owner, session, native = self.prepare()
        second = self.finish_second(owner, session)
        frame = self.model.frame
        other = SimpleNamespace(
            queryClosing=Mock(side_effect=extension.CloseVetoException("other", None))
        )
        frame.addCloseListener(other)
        self.choices.return_value = "save"
        self.close()
        self.assertEqual(native.disk, second[0])
        self.assertFalse(self.model.modified)
        self.assertTrue(session["guard"].tainted)
        frame.removeCloseListener(other)
        owner.native = b'{"version":15,"title":"third host edit"}'
        self.model.setModified(True)
        self.choices.return_value = "cancel"
        self.close()
        self.assertEqual(native.disk, second[0])
        self.assertFalse(native.closed)
        self.choices.return_value = "save"
        self.close()
        self.assertTrue(native.closed)
        self.assertEqual(native.disk, owner.native)

    def test_future_view_registration_failure_keeps_barrier_existing_guard_and_recovery(self):
        for failure in ("interceptor", "listener", "lock"):
            with self.subTest(failure=failure):
                self.model = host_tests.Model()
                owner, session, native = self.prepare()
                self.finish_second(owner, session)
                first = self.model.frame
                new = native.bind(host_tests.Frame(self.model))
                new.fail_interceptor_add = failure == "interceptor"
                new.fail_add = failure == "listener"
                new.fail_lock = failure == "lock"
                self.model.event("OnViewCreated", new.controller)
                guard = session["guard"]
                self.assertTrue(guard.unlock_error)
                self.assertTrue(session["path"].exists())
                self.assertEqual(new.locks, 0 if failure == "lock" else 1)
                self.vetoed(first)
                self.vetoed(self.model)
                with self.assertRaises(RuntimeError):
                    guard.acquire(self.owner(), {"token": "blocked"}, new)
                if failure != "lock":
                    native.native_quit(only=new)
                    self.assertIsNotNone(new.controller)
                self.model.closed(disposal=True)

    def test_temporary_view_lock_release_replays_true_only_after_both_guards_attach(self):
        owner, session, native = self.prepare()
        frame = native.bind(host_tests.Frame(self.model))

        def remembered_close():
            self.assertEqual(len(frame.interceptors), 1)
            self.assertEqual(len(frame.listeners), 1)
            self.vetoed(frame, True)

        frame.on_unlock = remembered_close
        self.model.event("OnViewCreated", frame.controller)
        self.assertEqual((frame.adds, frame.removes, frame.locks), (1, 1, 0))
        self.assertIn(False, session["guard"].obligations.values())
        self.assertTrue(session["guard"].tainted)
        self.choices.assert_not_called()

    def test_interceptor_registration_reentry_has_late_listener_before_native_quit(self):
        owner = self.owner()
        native = NativeHost(self.model, owner)
        self.model.modified = True
        native.native_choices = ["discard"]
        frame = self.model.frame
        original = frame.registerDispatchProviderInterceptor

        def register(interceptor):
            original(interceptor)
            native.native_quit()

        with patch.object(frame, "registerDispatchProviderInterceptor", side_effect=register):
            session = self.session(owner)
        self.assertTrue(native.prepared)
        self.assertFalse(native.closed)
        self.assertTrue(session["guard"].tainted)
        self.assertTrue(session["guard"].owns(owner, session))

    def test_modal_exception_and_missing_close_result_clear_permit_without_loop(self):
        for failure in ("modal", "result"):
            with self.subTest(failure=failure):
                self.model = host_tests.Model()
                owner, session, native = self.prepare()
                self.finish_second(owner, session)
                if failure == "modal":
                    self.choices.side_effect = RuntimeError("dialog failed")
                else:
                    self.choices.side_effect = None
                    native.no_close_result = True
                    native.before_close = lambda: (_ for _ in ()).throw(
                        extension.CloseVetoException("other", None)
                    )
                self.close()
                guard = session["guard"]
                self.assertFalse(native.closed)
                self.assertIsNone(guard.permit)
                self.assertFalse(guard.in_choice)
                self.assertEqual(self.queue, [])
                self.assertTrue(session["path"].exists())
                self.model.closed(disposal=True)

    def test_queued_false_choice_drops_if_a_new_editor_starts_before_callback(self):
        owner, session, native = self.prepare()
        self.finish_second(owner, session)
        self.close(pump=False)
        next_owner = self.owner()
        next_session = self.session(next_owner)
        self.pump()
        self.choices.assert_not_called()
        next_owner._finish(next_session)
        self.pump()
        self.choices.assert_not_called()
        self.assertEqual(self.close_commands(native), [])
        self.assertTrue(session["path"].exists())

    def test_close_permit_cannot_authorize_reentrant_quit_or_duplicate_dispatch(self):
        owner, session, native = self.prepare()
        self.finish_second(owner, session)
        frame = self.model.frame
        feedback = Mock()

        def reenter():
            for command in (".uno:Quit", ".uno:CloseWin"):
                url = extension.command_url(command)
                frame.queryDispatch(url, "_self", 0).dispatchWithNotification(
                    url, (), SimpleNamespace(dispatchFinished=feedback)
                )

        native.before_close = reenter
        self.close()
        self.assertTrue(native.closed)
        self.assertEqual([command for _, command, _ in native.calls], [".uno:CloseWin"])
        self.assertEqual(feedback.call_count, 2)
        self.assertTrue(
            all(call.args[0].State == extension.FAILURE for call in feedback.call_args_list)
        )

    def test_rejected_editor_during_native_close_creates_no_session_or_late_false_veto(self):
        owner, session, native = self.prepare()
        self.finish_second(owner, session)
        other = self.owner()

        def reenter():
            with (
                patch.object(extension, "executable", return_value="editor"),
                patch.object(extension.subprocess, "Popen") as launch,
            ):
                other.doVerb(0)
            launch.assert_not_called()
            self.assertIsNone(other.session)

        native.before_close = reenter
        self.close()
        self.assertTrue(native.closed)
        self.assertIsNone(session["guard"].permit)

    def test_new_view_after_first_close_doc_view_disposal_revokes_remaining_scope(self):
        owner, session, native = self.prepare()
        self.finish_second(owner, session)
        first = self.model.frame
        second = native.bind(host_tests.Frame(self.model))
        self.model.event("OnViewCreated", second.controller)
        added = []

        def after_view(frame):
            if not added:
                new = native.bind(host_tests.Frame(self.model))
                added.append(new)
                self.model.event("OnViewCreated", new.controller)

        native.after_view = after_view
        self.close(first, ".uno:CloseDoc")
        self.assertFalse(native.closed)
        self.assertEqual(native.closed_frames, [second])
        self.assertIsNotNone(first.controller)
        self.assertIsNotNone(added[0].controller)
        self.assertTrue(session["path"].exists())
        self.assertIsNone(session["guard"].permit)
        self.assertEqual(self.choices.call_count, 1)

    def test_start_center_path_is_blocked_early_and_then_uses_fresh_synchronous_consent(self):
        owner, session, native = self.prepare()
        self.finish_second(owner, session)
        native.backing = True
        frame = self.model.frame
        self.choices.return_value = "cancel"
        self.close()
        self.assertIsNotNone(frame.controller)
        self.assertEqual(self.close_commands(native), [])
        self.choices.return_value = "save"
        self.close()
        self.assertTrue(native.closed)
        self.assertIsNone(frame.controller)
        self.assertEqual(frame.interceptors, [])
        self.assertFalse(session["directory"].exists())

    def test_notify_closing_is_not_actual_model_disposal_or_recovery_cleanup(self):
        owner, session, native = self.prepare()
        self.finish_second(owner, session)
        guard = session["guard"]
        guard.listeners["model"].notifyClosing(SimpleNamespace(Source=self.model))
        self.assertFalse(guard.invalid)
        self.assertTrue(guard.tainted)
        self.assertTrue(session["path"].exists())
        self.assertIn(guard, extension.HOST_GUARDS)

    def test_unexpected_model_disposal_preserves_recovery_and_releases_registry_references(self):
        owner, session, native = self.prepare()
        self.finish_second(owner, session)
        guard = session["guard"]
        self.model.closed(disposal=True)
        self.assertTrue(session["path"].exists())
        self.assertTrue(guard.invalid)
        self.assertNotIn(guard, extension.HOST_GUARDS)
        self.assertEqual(guard.views, [])
        self.assertEqual(guard.listeners, {})
        self.assertIsNone(guard.model)
        self.assertEqual(self.model.frame.interceptors, [])

    def test_manual_native_save_does_not_clear_taint_for_later_edits(self):
        owner, session, native = self.prepare()
        self.finish_second(owner, session)
        url = extension.command_url(".uno:Save")
        self.model.frame.queryDispatch(url, "_self", 0).dispatch(url, ())
        self.assertFalse(self.model.modified)
        self.assertTrue(session["guard"].tainted)
        self.model.setModified(True)
        self.choices.return_value = "cancel"
        self.close()
        self.assertFalse(native.closed)
        self.assertEqual(self.choices.call_count, 1)

    def test_cached_old_controller_dispatch_uses_current_same_model_view_guard(self):
        owner, session, native = self.prepare()
        self.finish_second(owner, session)
        frame = self.model.frame
        url = extension.command_url(".uno:CloseWin")
        cached = frame.queryDispatch(url, "_self", 0)
        previous = frame.controller
        self.model.event("OnViewClosed", previous)
        self.model.controllers.remove(previous)
        frame.controller = SimpleNamespace(getModel=lambda: self.model, getFrame=lambda: frame)
        self.model.controllers.append(frame.controller)
        native.bind(frame)
        self.model.event("OnViewCreated", frame.controller)
        self.choices.return_value = "cancel"
        cached.dispatch(url, ())
        self.pump()
        self.assertFalse(native.closed)
        self.assertEqual(self.close_commands(native), [])
        self.assertEqual(self.choices.call_count, 1)
        self.assertIs(self.choices.call_args.args[2], frame)
        self.assertTrue(session["path"].exists())

    def test_unannounced_same_model_controller_replacement_blocks_cached_dispatch(self):
        owner, session, native = self.prepare()
        self.finish_second(owner, session)
        frame = self.model.frame
        url = extension.command_url(".uno:CloseWin")
        cached = frame.queryDispatch(url, "_self", 0)
        frame.controller = SimpleNamespace(getModel=lambda: self.model, getFrame=lambda: frame)
        cached.dispatch(url, ())
        self.pump()
        self.assertFalse(native.closed)
        self.assertEqual(self.close_commands(native), [])
        self.assertTrue(session["guard"].unlock_error)
        self.assertTrue(session["path"].exists())
        self.assertIn("controller changed", self.errors.call_args.args[1])

    def test_model_true_during_reconciliation_upgrades_one_choice_to_all_current_views(self):
        for decision in ("cancel", "save", "discard"):
            for retire_original in (False, True):
                with self.subTest(decision=decision, retire_original=retire_original):
                    self.model = host_tests.Model()
                    owner, session, native = self.prepare()
                    self.finish_second(owner, session)
                    guard = session["guard"]
                    original = self.model.frame
                    remaining = native.bind(host_tests.Frame(self.model))
                    self.model.event("OnViewCreated", remaining.controller)
                    enumerate_controllers = self.model.getControllers
                    reentered = False

                    def reconcile():
                        nonlocal reentered
                        if not reentered:
                            reentered = True
                            self.vetoed(self.model, True)
                            if retire_original:
                                controller = original.controller
                                self.model.event("OnViewClosed", controller)
                                self.model.controllers.remove(controller)
                                original.controller = None
                                self.model.frame = remaining
                        return enumerate_controllers()

                    self.choices.return_value = decision
                    choices_before = self.choices.call_count
                    self.close(original, pump=False)
                    with patch.object(self.model, "getControllers", side_effect=reconcile):
                        self.pump()
                    self.assertTrue(reentered)
                    self.assertEqual(self.choices.call_count, choices_before + 1)
                    self.assertTrue(self.choices.call_args.args[3])
                    self.assertIs(
                        self.choices.call_args.args[2], remaining if retire_original else original
                    )
                    self.assertIsNone(guard.pending_choice)
                    self.assertIsNone(guard.permit)
                    self.assertFalse(guard.in_choice)
                    self.assertEqual(self.queue, [])
                    if decision == "cancel":
                        self.assertEqual(guard.obligations, {"model": True})
                        self.assertEqual(self.close_commands(native), [])
                        self.assertTrue(session["path"].exists())
                        guard._settle()
                        self.pump()
                        self.assertEqual(self.choices.call_count, choices_before + 1)
                        self.assertEqual(self.queue, [])
                        self.model.closed(disposal=True)
                    else:
                        self.assertEqual(self.close_commands(native), [".uno:CloseDoc"])
                        self.assertTrue(native.closed)
                        self.assertEqual(guard.obligations, {})
                        self.assertFalse(session["directory"].exists())
                        self.assertEqual(
                            native.closed_frames,
                            [remaining] if retire_original else [remaining, original],
                        )


if __name__ == "__main__":
    unittest.main()
