"""Portable ODF embedded drawings, using LibreOffice's Python UNO runtime.

Document storage contains data only. Only the locally configured ReShiki
executable is launched, with fixed arguments and an isolated edit directory.
"""

import base64
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import uuid
from contextlib import suppress
from pathlib import Path

import uno
import unohelper
from com.sun.star.awt import XActionListener, XCallback
from com.sun.star.datatransfer import UnsupportedFlavorException, XTransferable
from com.sun.star.document import XDocumentEventListener
from com.sun.star.embed import (
    WrongStateException,
    XEmbeddedObject,
    XEmbeddedObjectCreator,
    XEmbedPersist,
    XTransactionListener,
)
from com.sun.star.embed.EmbedStates import ACTIVE, LOADED, RUNNING
from com.sun.star.frame import (
    XDispatch,
    XDispatchProvider,
    XDispatchProviderInterceptor,
    XDispatchResultListener,
    XInterceptorInfo,
    XNotifyingDispatch,
)
from com.sun.star.frame.DispatchResultState import DONTKNOW, FAILURE, SUCCESS
from com.sun.star.frame.InfobarType import DANGER, INFO
from com.sun.star.io import IOException
from com.sun.star.lang import XComponent, XInitialization, XServiceInfo
from com.sun.star.uno import RuntimeException as UnoRuntimeException
from com.sun.star.util import CloseVetoException, XCloseable, XCloseListener, XModifyListener

CLASS_ID = "8E86A932-EBBE-4E9F-8D26-CA2D82096856"
MIME = "application/vnd.reshiki.embedded-drawing"
NATIVE_MIME = "application/x-reshiki-drawing+json"
NATIVE_CLIPBOARD_TYPES = (
    NATIVE_MIME,
    "dev.reshiki.drawing",
    "application/x-moruno-drawing+json",
    "dev.moruno.drawing",
)
FACTORY = "dev.reshiki.libreoffice.EmbeddedFactory"
HANDLER = "dev.reshiki.libreoffice.ProtocolHandler"
PROTOCOL = "dev.reshiki.libreoffice:"
LIMIT = 64 * 1024 * 1024


def prop(name, value):
    result = uno.createUnoStruct("com.sun.star.beans.PropertyValue")
    result.Name, result.Value = name, value
    return result


def size(width, height):
    result = uno.createUnoStruct("com.sun.star.awt.Size")
    result.Width, result.Height = width, height
    return result


def flavor(mime):
    result = uno.createUnoStruct("com.sun.star.datatransfer.DataFlavor")
    result.MimeType = mime
    result.HumanPresentableName = "ReShiki drawing" if mime == NATIVE_MIME else "PNG preview"
    result.DataType = uno.getTypeByName("[]byte")
    return result


def service(ctx, name):
    return ctx.ServiceManager.createInstanceWithContext(name, ctx)


def uses_host_clipboard():
    # A separate Wayland helper does not own LibreOffice's input focus/serial.
    # Let the host's VCL backend handle both Linux desktop clipboard protocols.
    return sys.platform.startswith("linux")


def bounded_clipboard_bytes(value):
    if isinstance(value, uno.ByteSequence):
        value = value.value
    if not isinstance(value, (bytes, bytearray)):
        raise ValueError("The clipboard did not provide binary drawing data.")
    if not 0 < len(value) <= LIMIT:
        raise ValueError("Clipboard drawing or preview exceeds 64 MB or is empty.")
    return bytes(value)


def transferable_bytes(contents, requested):
    if requested.DataType != uno.getTypeByName("[]byte"):
        raise ValueError("The clipboard drawing has an unsupported data type.")
    return bounded_clipboard_bytes(contents.getTransferData(requested))


def read_host_native(ctx):
    clipboard = service(ctx, "com.sun.star.datatransfer.clipboard.SystemClipboard")
    contents = clipboard.getContents()
    offered = contents.getTransferDataFlavors() if contents else ()
    for mime in NATIVE_CLIPBOARD_TYPES:
        for requested in offered:
            if requested.MimeType == mime:
                return transferable_bytes(contents, requested)
    raise ValueError("Copy an editable drawing in ReShiki before using Paste ReShiki Drawing.")


class ClipboardDrawing(unohelper.Base, XTransferable):
    """Immutable clipboard bytes, independent of a document or edit session."""

    def __init__(self, native, png):
        native, png = bounded_clipboard_bytes(native), bounded_clipboard_bytes(png)
        if len(native) + len(png) > LIMIT:
            raise ValueError("Combined clipboard drawing and preview exceed 64 MB.")
        self._data = ((NATIVE_MIME, native), ("image/png", png))

    def getTransferDataFlavors(self):
        return tuple(flavor(mime) for mime, _ in self._data)

    def isDataFlavorSupported(self, requested):
        return requested.DataType == uno.getTypeByName("[]byte") and any(
            mime == requested.MimeType for mime, _ in self._data
        )

    def getTransferData(self, requested):
        if self.isDataFlavorSupported(requested):
            for mime, raw in self._data:
                if mime == requested.MimeType:
                    return uno.ByteSequence(raw)
        raise UnsupportedFlavorException("Unsupported ReShiki clipboard format.", self)


def publish_host_clipboard(ctx, frame, drawing, finished):
    """Publish on the UNO thread, then verify retention on its next callback."""
    window = frame.getContainerWindow()
    active = service(ctx, "com.sun.star.awt.Toolkit").getActiveTopWindow()
    if not window or not active or active != window:
        raise RuntimeError("Return to the LibreOffice drawing window and copy again.")
    clipboard = service(ctx, "com.sun.star.datatransfer.clipboard.SystemClipboard")
    clipboard.setContents(drawing, None)

    def verify():
        try:
            current = clipboard.getContents()
            if current is None:
                raise RuntimeError("The clipboard changed before copying finished. Copy again.")
            for requested in drawing.getTransferDataFlavors():
                if (
                    transferable_bytes(current, requested)
                    != drawing.getTransferData(requested).value
                ):
                    raise RuntimeError("The clipboard changed before copying finished. Copy again.")
        except Exception as error:
            finished(str(error))
        else:
            finished()

    # GTK VCL queues publication inside setContents; an immediate getContents
    # or change listener only echoes its cached provider. This later callback
    # verifies host retention after that event, not compositor acknowledgement.
    post(ctx, verify)


def settings_path(ctx):
    url = service(ctx, "com.sun.star.util.PathSubstitution").substituteVariables("$(user)", True)
    return Path(uno.fileUrlToSystemPath(url)) / "reshiki-integration.json"


def executable(ctx):
    settings = settings_path(ctx)
    if settings.exists():
        candidate = Path(json.loads(settings.read_text(encoding="utf-8"))["executable"])
        if candidate.is_file():
            return str(candidate)
        raise RuntimeError(
            "The configured ReShiki app has moved. Use ReShiki → Choose ReShiki App."
        )
    candidates = [shutil.which("reshiki")]
    if os.name == "nt":
        candidates += [
            str(Path(os.environ.get("LOCALAPPDATA", "")) / "Programs/ReShiki/reshiki.exe")
        ]
    else:
        candidates += ["/Applications/ReShiki.app/Contents/MacOS/reshiki"]
    for candidate in candidates:
        if candidate and Path(candidate).is_file():
            return str(Path(candidate).resolve())
    raise RuntimeError(
        "Use ReShiki → Choose ReShiki App to select the installed ReShiki executable."
    )


def worker(program, mode, data=b""):
    # Disk-backed output avoids trusting a faulty subprocess to bound stdout.
    with tempfile.TemporaryFile() as output, tempfile.TemporaryFile() as errors:
        try:
            result = subprocess.run(
                [program, mode], input=data, stdout=output, stderr=errors, timeout=90, check=False
            )
        except subprocess.TimeoutExpired:
            raise RuntimeError(
                "ReShiki took too long to start or complete the operation. "
                "Open ReShiki once, then retry."
            ) from None
        errors.seek(0)
        message = errors.read(8192).decode("utf-8", "replace").strip()
        if result.returncode:
            raise RuntimeError(message or "ReShiki could not complete the operation.")
        output.seek(0)
        raw = output.read(LIMIT * 3 + 1)
        if len(raw) > LIMIT * 3:
            raise ValueError("ReShiki returned an oversized response.")
        return json.loads(raw)


def packet(value):
    if value.get("version") != 1:
        raise ValueError("Unsupported ReShiki embedding version.")
    native = base64.b64decode(value["native"], validate=True)
    png = base64.b64decode(value["png"], validate=True)
    extent = value["extent"]
    if not 0 < len(native) <= LIMIT or not 0 < len(png) <= LIMIT:
        raise ValueError("Embedded drawing or preview exceeds 64 MB.")
    if not png.startswith(b"\x89PNG\r\n\x1a\n"):
        raise ValueError("The embedded preview is not PNG.")
    if (
        not isinstance(extent, list)
        or len(extent) != 2
        or any(type(n) is not int or not 0 < n <= 2**31 - 1 for n in extent)
    ):
        raise ValueError("Invalid physical drawing size.")
    if not isinstance(json.loads(native), dict):
        raise ValueError("Invalid native drawing data.")
    return native, png, tuple(extent)


class Callback(unohelper.Base, XCallback):
    def __init__(self, action):
        self.action = action

    def notify(self, ignored):
        self.action()


def post(ctx, action):
    service(ctx, "com.sun.star.awt.AsyncCallback").addCallback(Callback(action), None)


def show_error(ctx, message, frame=None):
    if frame is None:
        desktop = service(ctx, "com.sun.star.frame.Desktop")
        frame = desktop.getCurrentFrame()
    if frame:
        toolkit = service(ctx, "com.sun.star.awt.Toolkit")
        box = toolkit.createMessageBox(
            frame.getContainerWindow(),
            uno.Enum("com.sun.star.awt.MessageBoxType", "ERRORBOX"),
            1,
            "ReShiki",
            str(message),
        )
        box.execute()
        box.dispose()


class CopyFeedback:
    """Optional host UI; every method runs on the LibreOffice callback thread."""

    def __init__(self, ctx, frame):
        self.ctx, self.frame, self.indicator = ctx, frame, None
        self.controller = None
        with suppress(Exception):
            self.controller = frame.getController()
        self.show("Copying editable drawing…", "Wait for confirmation before pasting.")

    def show(self, title, detail, failed=False):
        try:
            controller = self.controller
            kind = DANGER if failed else INFO
            if controller.hasInfobar("reshiki-copy"):
                controller.updateInfobar("reshiki-copy", title, detail, kind)
            else:
                controller.appendInfobar("reshiki-copy", title, detail, kind, (), True)
            return
        except Exception:
            # Some hosts expose only the older frame status indicator. Neither
            # missing UI nor a disposed frame may turn a good copy into failure.
            pass
        try:
            text = title + " " + detail
            if self.indicator is None:
                self.indicator = self.frame.createStatusIndicator()
                self.indicator.start(text, 1)
            else:
                self.indicator.setText(text)
        except Exception:
            pass

    def finish(self, succeeded):
        if succeeded:
            self.show("Editable drawing copied", "Ready to paste in ReShiki.")
        else:
            self.show("Drawing was not copied", "See the error message before retrying.", True)
        if self.indicator is not None:
            # Keep fallback completion visible briefly without leaving the
            # host's progress indicator active indefinitely.
            def clear():
                with suppress(Exception):
                    post(self.ctx, self.end)

            try:
                timer = threading.Timer(4, clear)
                timer.daemon = True
                timer.start()
            except Exception:
                self.end()

    def end(self):
        with suppress(Exception):
            self.indicator.end()


class ReplacementTransaction(unohelper.Base, XTransactionListener):
    def __init__(self, owner, parent, name, png):
        self.owner, self.parent, self.name, self.png = owner, parent, name, png

    def preCommit(self, event):
        self.owner._write_replacement(self.parent, self.name, self.png)

    def commited(self, event):
        self.cancel()

    def preRevert(self, event):
        pass

    def reverted(self, event):
        self.cancel()

    def disposing(self, event):
        self.cancel(disposed=True)

    def cancel(self, disposed=False):
        if self.owner is not None:
            if not disposed:
                self.parent.removeTransactionListener(self)
            self.owner.replacements.remove(self)
            self.owner, self.parent = None, None


HOST_GUARDS = []  # UNO model equality, never a filename (Save As keeps the model).


class CloseChoiceAction(unohelper.Base, XActionListener):
    def __init__(self, dialog, choice):
        self.dialog, self.choice, self.selected = dialog, choice, False

    def actionPerformed(self, event):
        self.selected = True
        self.dialog.endExecute()

    def disposing(self, event):
        pass


def choose_host_close(ctx, model, frame, all_views):
    """Fresh user intent, parented to the captured document, never the active tab."""
    title = model.getTitle()
    scope = "this document and all of its windows" if all_views else "this window"
    dialog_model = service(ctx, "com.sun.star.awt.UnoControlDialogModel")
    dialog_model.Width, dialog_model.Height = 284, 100
    dialog_model.Title = "Close document — ReShiki"
    text = dialog_model.createInstance("com.sun.star.awt.UnoControlFixedTextModel")
    text.PositionX, text.PositionY, text.Width, text.Height = 10, 10, 264, 46
    text.MultiLine = True
    text.Label = (
        f"Close {scope} for “{title}”?\n"
        "Choose whether to save the document's current changes. "
        "Cancel keeps it open."
    )
    dialog_model.insertByName("message", text)
    for index, (name, label) in enumerate(
        (("save", "Save and Close"), ("discard", "Don't Save and Close"), ("cancel", "Cancel"))
    ):
        button = dialog_model.createInstance("com.sun.star.awt.UnoControlButtonModel")
        button.PositionX, button.PositionY = 10 + index * 90, 68
        button.Width, button.Height = 84, 20
        button.Label, button.PushButtonType = label, 2 if name == "cancel" else 0
        button.DefaultButton = name == "cancel"
        dialog_model.insertByName(name, button)
    dialog = service(ctx, "com.sun.star.awt.UnoControlDialog")
    dialog.setModel(dialog_model)
    actions = []
    try:
        for name in ("save", "discard"):
            action = CloseChoiceAction(dialog, name)
            dialog.getControl(name).addActionListener(action)
            actions.append(action)
        dialog.createPeer(service(ctx, "com.sun.star.awt.Toolkit"), frame.getContainerWindow())
        dialog.execute()
        return next((action.choice for action in actions if action.selected), "cancel")
    finally:
        dialog.dispose()


def command_url(command):
    url = uno.createUnoStruct("com.sun.star.util.URL")
    url.Complete, url.Protocol, url.Path = command, ".uno:", command[5:]
    return url


class DispatchCompletion(unohelper.Base, XDispatchResultListener):
    def __init__(self):
        self.done, self.state, self.result = False, None, None

    def dispatchFinished(self, event):
        self.done, self.state, self.result = True, event.State, event.Result

    def disposing(self, event):
        self.done = True


class GuardedCloseDispatch(unohelper.Base, XNotifyingDispatch):
    def __init__(self, view, delegate):
        self.view, self.delegate = view, delegate

    def dispatch(self, url, args):
        self.dispatchWithNotification(url, args, None)

    def dispatchWithNotification(self, url, args, listener):
        view, guard = self.view, self.view.guard
        blocked = False
        if not guard.invalid and not view.current():
            # Cached dispatches may outlive a controller (e.g. a view switch).
            # A replacement for this SAME model still needs its current guard;
            # only an unrelated model may bypass the old model's protection.
            controller = view.frame.getController()
            if controller is not None and controller.getModel() == guard.model:
                current = next(
                    (item for item in guard.views if item.frame == view.frame and item.current()),
                    None,
                )
                if current is None:
                    guard.protection_failed(
                        RuntimeError(
                            "The document controller changed before protection was ready."
                        ),
                        HostView(guard, view.frame, controller, "untracked"),
                    )
                    blocked = True
                else:
                    view = current
        if not blocked and view.current() and not guard.invalid:
            kind = "model" if url.Complete == ".uno:CloseDoc" else view.kind
            if guard.permits_dispatch(view, url.Complete):
                pass
            elif guard.blocks_close() or guard.in_choice:
                guard.notice_veto(view)
                blocked = True
            elif guard.tainted:
                guard.request_choice(kind)
                blocked = True
        if blocked:
            state = FAILURE
        elif hasattr(self.delegate, "dispatchWithNotification"):
            self.delegate.dispatchWithNotification(url, args, listener)
            return
        else:
            self.delegate.dispatch(url, args)
            state = DONTKNOW
        if listener is not None:
            event = uno.createUnoStruct("com.sun.star.frame.DispatchResultEvent")
            event.Source, event.State = self, state
            listener.dispatchFinished(event)

    def addStatusListener(self, listener, url):
        self.delegate.addStatusListener(listener, url)

    def removeStatusListener(self, listener, url):
        self.delegate.removeStatusListener(listener, url)


class HostCloseInterceptor(unohelper.Base, XDispatchProviderInterceptor, XInterceptorInfo):
    URLS = (".uno:CloseWin", ".uno:CloseDoc", ".uno:CloseFrame", ".uno:Quit")

    def __init__(self, view):
        self.view, self.master, self.slave = view, None, None

    def getInterceptedURLs(self):
        return self.URLS

    def getMasterDispatchProvider(self):
        return self.master

    def setMasterDispatchProvider(self, value):
        self.master = value

    def getSlaveDispatchProvider(self):
        return self.slave

    def setSlaveDispatchProvider(self, value):
        self.slave = value

    def queryDispatch(self, url, target, flags):
        delegate = self.slave.queryDispatch(url, target, flags) if self.slave else None
        if delegate is not None and url.Complete in self.URLS and target in ("", "_self", "_top"):
            return GuardedCloseDispatch(self.view, delegate)
        return delegate

    def queryDispatches(self, descriptors):
        return tuple(
            self.queryDispatch(item.FeatureURL, item.FrameName, item.SearchFlags)
            for item in descriptors
        )


class HostCloseListener(unohelper.Base, XCloseListener):
    def __init__(self, guard, kind, source):
        self.guard, self.kind, self.source = guard, kind, source
        self.attached, self.alive = False, True

    def queryClosing(self, event, ownership):
        guard = self.guard
        if event.Source != self.source or guard.invalid:
            return
        if self.kind != "model" and not guard.view_for(self.kind).current():
            return
        if guard.permits(self.kind, closing_model=self.kind == "model"):
            return
        if not guard.tainted and not guard.blocks_close() and not guard.in_choice:
            return
        # Native macOS Quit/Desktop.terminate bypass dispatch interception. Its
        # Save/Discard may already be cached by PrepareClose before this veto.
        guard.tainted = True
        if ownership:
            guard.obligations[self.kind] = False
        if guard.blocks_close() or guard.in_choice:
            guard.notice_veto(guard.view_for(self.kind))
        else:
            guard.request_choice(self.kind)
        raise CloseVetoException(
            guard.unlock_error or "Choose whether to save before closing this document.", self
        )

    def notifyClosing(self, event):
        if event.Source == self.source and self.kind != "model":
            self.guard.view_closed(self.kind)

    def disposing(self, event):
        if event.Source == self.source:
            if self.kind == "model":
                self.guard.model_disposed()
            else:
                self.guard.view_closed(self.kind)


class HostModelEvents(unohelper.Base, XDocumentEventListener, XModifyListener):
    def __init__(self, guard):
        self.guard = guard

    def documentEventOccured(self, event):
        guard = self.guard
        if event.Source != guard.model or guard.invalid:
            return
        controller = event.ViewController
        if event.EventName == "OnViewCreated" and controller is not None:
            try:
                guard.ensure_view(controller.getFrame(), controller)
            except Exception as error:
                guard.protection_failed(error)
        elif event.EventName == "OnViewClosed" and controller is not None:
            for view in tuple(guard.views):
                if view.controller == controller:
                    guard.view_closed(view.kind)

    def modified(self, event):
        if event.Source == self.guard.model:
            # This is XModifyListener, not the boolean OnModifyChanged event.
            self.guard.content_generation += 1

    def disposing(self, event):
        if event.Source == self.guard.model:
            self.guard.model_disposed()


class HostView:
    def __init__(self, guard, frame, controller, kind):
        self.guard, self.frame, self.controller, self.kind = guard, frame, controller, kind
        self.listener = HostCloseListener(guard, kind, frame)
        self.interceptor = HostCloseInterceptor(self)
        self.intercepting, self.attaching, self.temporary_lock = False, False, False

    def current(self):
        try:
            return (
                self.listener.alive
                and self.frame.getController() == self.controller
                and self.controller.getModel() == self.guard.model
            )
        except Exception:
            return False


class HostGuard:
    """One model's editors, view coverage and fresh close decisions on the UI thread."""

    def __init__(self, ctx, model, frame):
        self.ctx, self.model, self.frame = ctx, model, frame
        self.controller = frame.getController()
        self.views = []
        self.listeners = {"model": HostCloseListener(self, "model", model)}
        self.events = HostModelEvents(self)
        self.document_events, self.modify_events = False, False
        self.sessions, self.obligations = {}, {}
        self.scheduled = set()
        self.phase, self.locked, self.invalid = "IDLE", False, False
        self.notice_pending, self.tainted, self.in_choice = False, False, False
        self.notice_view = None
        self.unlock_error, self.pending_choice, self.permit = None, None, None
        self.content_generation, self.activity_generation = 0, 0
        self.recovery = set()
        self.active_view = self._new_view(frame, self.controller)

    def _new_view(self, frame, controller):
        kind = "frame" if not self.views else "frame:" + str(len(self.views))
        view = HostView(self, frame, controller, kind)
        self.views.append(view)
        self.listeners[kind] = view.listener
        self.activity_generation += 1
        return view

    def view_for(self, kind):
        return next((view for view in self.views if view.kind == kind), self.active_view)

    def blocks_close(self):
        return (
            bool(self.sessions)
            or self.phase in ("ACQUIRING", "TEARING_DOWN")
            or bool(self.unlock_error)
            or any(view.attaching for view in self.views)
        )

    def accepted_update(self):
        # Explicitly track every acknowledged embedded update, including when
        # the host was already dirty and no boolean modified event fires.
        self.content_generation += 1

    def notice_veto(self, view=None):
        # A queued editor notice may outlive its original controller. Preserve
        # the newest exact-view error target while coalescing callbacks.
        self.notice_view = view or self.active_view
        if self.notice_pending:
            return
        self.notice_pending = True

        def notify():
            view = self.notice_view
            try:
                if not self.invalid and self.blocks_close() and view.current():
                    completed = self.sessions and all(
                        session["watch_complete"] and session["process"].poll() is not None
                        for _, session in self.sessions.values()
                    )
                    message = self.unlock_error or (
                        "The editor has closed, but LibreOffice has not finished the session. "
                        "Select the object and choose Edit in ReShiki to finish recovery."
                        if completed
                        else "Close the ReShiki editing window before closing this document."
                    )
                    show_error(self.ctx, message, view.frame)
            finally:
                self.notice_pending = False
                self.notice_view = None

        try:
            post(self.ctx, notify)
        except Exception:
            self.notice_pending = False

    def protection_failed(self, error, view=None):
        self.tainted = True
        self.unlock_error = self.unlock_error or (
            "LibreOffice could not safely protect this document's windows. "
            "New ReShiki editing is unavailable; existing windows remain protected. "
            "Save your work before restarting LibreOffice. "
            "Edit in ReShiki cannot clear this state. " + str(error)
        )
        self.activity_generation += 1
        self.phase = "TEARING_DOWN"
        self.notice_veto(view)

    def owns(self, owner, session):
        registered = self.sessions.get(session["token"])
        return (
            not self.invalid
            and registered is not None
            and registered[0] is owner
            and registered[1] is session
            and self.listeners["model"].alive
            and self.active_view.current()
        )

    def ensure_view(self, frame, controller):
        if (
            self.invalid
            or controller.getModel() != self.model
            or frame.getController() != controller
        ):
            raise RuntimeError("The document window changed while protection was being installed.")
        view = next(
            (item for item in self.views if item.frame == frame and item.controller == controller),
            None,
        )
        if view is None:
            view = self._new_view(frame, controller)
        if view.intercepting and view.listener.attached:
            return view
        view.attaching = True
        try:
            # A new view's notification cannot veto creation. This owned lock
            # covers normal frame.close AND CloseDispatcher's Start Center path
            # until both early and late protection have attached.
            frame.addActionLock()
            view.temporary_lock = True
            # Listener first: interceptor registration calls contextChanged and
            # may reenter native Quit. Its late veto must mark model taint even
            # while the temporary lock is still held.
            view.listener.attached = True
            frame.addCloseListener(view.listener)
            view.intercepting = True  # Include partially failed registration.
            frame.registerDispatchProviderInterceptor(view.interceptor)
            if not view.current() or self.invalid:
                raise RuntimeError("The document closed while installing close protection.")
            if view is self.active_view and self.sessions and not self.locked:
                self.locked = True  # Transfer this one owned lock to editing.
                view.temporary_lock = False
            else:
                # removeActionLock can synchronously replay close(True). Keep
                # attaching=True until the exact one decrement has returned.
                frame.removeActionLock()
                view.temporary_lock = False
            return view
        except Exception as error:
            self.protection_failed(error)
            raise
        finally:
            view.attaching = False

    def reconcile_views(self):
        enumeration = self.model.getControllers()
        while enumeration.hasMoreElements():
            controller = enumeration.nextElement()
            self.ensure_view(controller.getFrame(), controller)

    def acquire(self, owner, session, frame):
        if self.permit is not None:
            # No editor is created once native close is already underway. A
            # rejected launch must not turn expected view disposal into a veto.
            raise RuntimeError("The document is closing. Try editing again afterward.")
        self.activity_generation += 1
        if self.in_choice:
            raise RuntimeError(
                "The document is deciding whether to close. Try editing again afterward."
            )
        if self.unlock_error:
            raise RuntimeError(self.unlock_error)
        if self.invalid or self.phase == "TEARING_DOWN":
            raise RuntimeError("The document's editing window is no longer available.")
        if self.sessions and frame != self.frame:
            raise RuntimeError(
                "Finish ReShiki editing in the other LibreOffice window for this document "
                "before editing from this window."
            )
        controller = frame.getController()
        if controller.getModel() != self.model:
            raise RuntimeError("The document's editing window changed.")
        self.active_view = next(
            (view for view in self.views if view.frame == frame and view.controller == controller),
            None,
        ) or self._new_view(frame, controller)
        self.frame, self.controller = frame, controller
        self.sessions[session["token"]] = (owner, session)
        session["guard"] = self
        self.phase = "ACQUIRING"
        try:
            listener = self.listeners["model"]
            if not listener.attached:
                listener.attached = True
                self.model.addCloseListener(listener)
            if not self.document_events:
                self.document_events = True
                self.model.addDocumentEventListener(self.events)
            if not self.modify_events:
                self.modify_events = True
                self.model.addModifyListener(self.events)
            self.reconcile_views()
            self.ensure_view(frame, controller)
            if not self.locked:
                frame.addActionLock()
                self.locked = True
            if self.invalid or not self.active_view.current():
                raise RuntimeError("The document closed while preparing the editor.")
            self.phase = "ACTIVE"
        except Exception as error:
            self.protection_failed(error)
            self.sessions.pop(session["token"], None)
            self.retain_recovery(session)
            self.phase = "TEARING_DOWN"
            raise

    def release(self, owner, session):
        registered = self.sessions.get(session["token"])
        if registered is None or registered[0] is not owner or registered[1] is not session:
            return
        del self.sessions[session["token"]]
        self.activity_generation += 1
        if self.tainted or self.unlock_error:
            self.retain_recovery(session)
        if self.sessions:
            self.phase = "ACTIVE"
            return
        self._unprotect()

    def retain_recovery(self, session):
        if self.tainted or self.unlock_error:
            self.recovery.add(session["directory"])
            return True
        return False

    def _unprotect(self):
        self.phase = "TEARING_DOWN"
        if self.unlock_error:
            return  # Never repeat a native decrement with uncertain outcome.
        if self.locked:
            try:
                self.frame.removeActionLock()
            except Exception as error:
                self.protection_failed(error)
                return
            self.locked = False
        self.phase = "IDLE"
        # Keep idle, untainted coverage dormant until model disposal. Ordinary
        # closes pass through. Removing/reinstalling listeners here would create
        # a reentrant gap just as native preparation can become cached.
        self._settle()

    def _settle(self):
        if self.invalid or self.blocks_close() or self.in_choice or self.pending_choice is not None:
            return
        pending = [kind for kind, attempted in self.obligations.items() if not attempted]
        if pending:
            self.request_choice("model" if "model" in pending else pending[0])

    def request_choice(self, kind):
        if self.invalid or self.blocks_close() or self.in_choice:
            return
        if self.pending_choice is not None:
            if kind == "model":
                self.pending_choice = kind
            return
        self.pending_choice = kind
        self.scheduled.add(kind)
        try:
            post(self.ctx, self._run_choice)
        except Exception:
            self.pending_choice = None
            self.scheduled.clear()
            # Failed scheduling is not permission to close or an automatic loop.
            if kind in self.obligations:
                self.obligations[kind] = True

    def _choice_snapshot(self, kind):
        self.reconcile_views()
        # Enumeration/attachment can reenter a model close(TRUE). Resolve the
        # transferred scope only AFTER those calls, before choosing a surviving
        # frame or presenting/consuming this single decision.
        if self.obligations.get("model") is False:
            kind = "model"
        view = self.view_for(kind)
        if kind == "model" and not view.current():
            view = next((item for item in self.views if item.current()), None)
        if view is None or not view.current() or self.invalid or self.blocks_close():
            raise RuntimeError("The original document window is no longer available.")
        live = tuple(item for item in self.views if item.current())
        return kind, (view, live, self.activity_generation, self.content_generation)

    def _check_choice(self, snapshot, content=True):
        view, live, activity, revision = snapshot
        if (
            self.invalid
            or self.blocks_close()
            or not view.current()
            or self.activity_generation != activity
            or (content and self.content_generation != revision)
            or any(not item.current() for item in live)
        ):
            raise RuntimeError(
                "The document changed during the close decision. Close it again when ready."
            )

    def _dispatch_sync(self, frame, command):
        url = command_url(command)
        dispatch = frame.queryDispatch(url, "_self", 0)
        if dispatch is None or not hasattr(dispatch, "dispatchWithNotification"):
            raise RuntimeError("LibreOffice cannot confirm the requested operation.")
        result = DispatchCompletion()
        dispatch.dispatchWithNotification(url, (prop("SynchronMode", True),), result)
        if not result.done:
            raise RuntimeError("LibreOffice did not synchronously confirm the requested operation.")
        return result

    def _run_choice(self):
        kind, self.pending_choice = self.pending_choice, None
        self.scheduled.clear()
        if kind is None or self.invalid or self.blocks_close() or self.in_choice:
            return
        self.in_choice = True
        view = self.view_for(kind)
        try:
            kind, snapshot = self._choice_snapshot(kind)
            view, live, _, _ = snapshot
            for source in tuple(self.obligations):
                if kind == "model" or source == kind:
                    self.obligations[source] = True
            choice = choose_host_close(self.ctx, self.model, view.frame, kind == "model")
            if choice not in ("save", "discard"):
                return
            self._check_choice(snapshot)
            # Original CloseDispatcher/Desktop caller has unwound and normally
            # resumed this controller. Explicit reactivation is safe for direct
            # UNO callers too; it does not reset the private prepared cache.
            if not view.controller.suspend(False):
                raise RuntimeError("LibreOffice could not reactivate the document window.")
            self._check_choice(snapshot)
            if choice == "save" and self.model.isModified():
                result = self._dispatch_sync(view.frame, ".uno:Save")
                if result.state != SUCCESS or result.result is not True:
                    raise RuntimeError("The document was not saved. It remains open.")
                # Save As may change title/URL on this same model. A new clean
                # checkpoint follows successful native Save, not the old URL.
                self._check_choice(snapshot, content=False)
                if not self.model.hasLocation() or self.model.isModified():
                    raise RuntimeError("The document still has unsaved changes. It remains open.")
                snapshot = (view, live, self.activity_generation, self.content_generation)
            elif choice == "save" and not self.model.hasLocation():
                # A clean untitled model still needs native Save As.
                result = self._dispatch_sync(view.frame, ".uno:Save")
                if result.state != SUCCESS or result.result is not True:
                    raise RuntimeError("The document was not saved. It remains open.")
                self._check_choice(snapshot, content=False)
                if not self.model.hasLocation() or self.model.isModified():
                    raise RuntimeError("The document still has unsaved changes. It remains open.")
                snapshot = (view, live, self.activity_generation, self.content_generation)
            self._check_choice(snapshot)
            allowed = live if kind == "model" else (view,)
            self.permit = {
                "kind": kind,
                "command": ".uno:CloseDoc" if kind == "model" else ".uno:CloseWin",
                "source_view": view,
                "dispatched": False,
                "views": allowed,
                "activity": self.activity_generation,
                "revision": self.content_generation,
                "choice": choice,
                "gone": set(),
                "model_close": False,
                "last_view": kind == "model" or len(live) == 1,
            }
            result = self._dispatch_sync(
                view.frame, ".uno:CloseDoc" if kind == "model" else ".uno:CloseWin"
            )
            if not self.invalid and result.state != SUCCESS:
                raise RuntimeError("The document remains open. Close it again when ready.")
        except Exception as error:
            if not self.invalid and view is not None and view.current():
                show_error(self.ctx, str(error), view.frame)
        finally:
            # No permission may survive an async callback, Cancel, another
            # listener's veto, a failed Save, or any reentrant/exceptional exit.
            self.permit = None
            self.in_choice = False
            # Deliberately do not _settle here: cancellation/failure never loops.

    def permits_dispatch(self, view, command):
        permit = self.permit
        if (
            permit is None
            or permit["source_view"] is not view
            or permit["command"] != command
            or permit["dispatched"]
            or not self.permits(permit["kind"])
        ):
            return False
        permit["dispatched"] = True
        return True

    def permits(self, kind, closing_model=False):
        permit = self.permit
        if (
            permit is None
            or self.invalid
            or self.blocks_close()
            or self.activity_generation != permit["activity"]
            or self.content_generation != permit["revision"]
        ):
            return False
        for view in permit["views"]:
            if view.kind not in permit["gone"] and not view.current():
                return False
        if permit["choice"] == "save":
            try:
                if self.model.isModified():
                    return False
            except Exception:
                return False
        if kind == "model":
            if not permit["last_view"]:
                return False
            if closing_model:
                permit["model_close"] = True
            return True
        return any(view.kind == kind for view in permit["views"])

    def view_closed(self, kind):
        view = self.view_for(kind)
        if view is None or not view.listener.alive:
            return
        expected = self.permit is not None and any(item is view for item in self.permit["views"])
        if expected:
            self.permit["gone"].add(kind)
        else:
            self.activity_generation += 1
        view.listener.alive = False
        self._detach_view(view)
        self.obligations.pop(kind, None)
        if view is self.active_view:
            self.locked = False
            for owner, session in tuple(self.sessions.values()):
                self.retain_recovery(session)
                owner._invalidate_session(session, "The LibreOffice editing window closed.", True)
            self.sessions.clear()
            self.phase = "IDLE"
        # The model's taint and other views outlive this exact source.

    def _detach_view(self, view):
        # A CloseWin backing transition keeps the frame alive. Remove only our
        # interceptor/listener even when its old controller is already gone.
        try:
            if view.intercepting:
                view.intercepting = False
                view.frame.releaseDispatchProviderInterceptor(view.interceptor)
            if view.listener.attached:
                view.listener.attached = False
                view.frame.removeCloseListener(view.listener)
        except Exception as error:
            if not self.invalid:
                self.protection_failed(error)

    def model_disposed(self):
        if self.invalid:
            return
        permit = self.permit
        cleanup = (
            permit is not None
            and permit["model_close"]
            and self.activity_generation == permit["activity"]
            and self.content_generation == permit["revision"]
            and not self.unlock_error
        )
        self.invalid = True
        self.listeners["model"].alive = False
        for owner, session in tuple(self.sessions.values()):
            self.retain_recovery(session)
            owner._invalidate_session(session, "The LibreOffice document closed.", True)
        self.sessions.clear()
        self.obligations.clear()
        self.pending_choice = None
        self.scheduled.clear()
        self.locked = False
        for view in self.views:
            view.listener.alive = False
            self._detach_view(view)
        if cleanup:
            for directory in self.recovery:
                with suppress(OSError):
                    shutil.rmtree(directory)
            self.recovery.clear()
        # Native disposal clears all these listener containers too. Remove our
        # references explicitly while the model is still in its dispose callback.
        with suppress(Exception):
            self.model.removeCloseListener(self.listeners["model"])
        with suppress(Exception):
            self.model.removeDocumentEventListener(self.events)
        with suppress(Exception):
            self.model.removeModifyListener(self.events)
        self.document_events = self.modify_events = False
        for listener in self.listeners.values():
            listener.attached = False
            listener.source = None
        self.listeners.clear()
        self.views.clear()
        self.active_view = self.frame = self.controller = self.model = None
        self.tainted = False  # Only actual model disposal ends this lifetime.
        HOST_GUARDS[:] = [guard for guard in HOST_GUARDS if guard is not self]


def acquire_host_guard(owner, session):
    if owner.client is None:
        raise RuntimeError("The drawing has no LibreOffice document.")
    model = owner.client.getComponent()
    frame = model.getCurrentController().getFrame()
    guard = next((item for item in HOST_GUARDS if item.model == model), None)
    if guard is None:
        guard = HostGuard(owner.ctx, model, frame)
        HOST_GUARDS.append(guard)
    guard.acquire(owner, session, frame)


class Embedded(
    unohelper.Base,
    XEmbeddedObject,
    XEmbedPersist,
    XServiceInfo,
    XCloseable,
    XComponent,
    XTransferable,
):
    def __init__(self, ctx, initial=None):
        self.ctx = ctx
        self.native, self.png, self.extent = initial or (b"", b"", (1, 1))
        self.parent, self.entry, self.pending = None, "", None
        self.replacements = []
        self.persisted, self.persist_error = None, None
        self.deferred_update = None
        self.client, self.state, self.readonly = None, LOADED, False
        self.events, self.states, self.closes = [], [], []
        self.session = None
        self.disposed = False

    def getImplementationName(self):
        return "dev.reshiki.libreoffice.EmbeddedObject"

    def supportsService(self, name):
        return name in self.getSupportedServiceNames()

    def getSupportedServiceNames(self):
        return (self.getImplementationName(),)

    def getClassID(self):
        return uno.ByteSequence(uuid.UUID(CLASS_ID).bytes)

    def getClassName(self):
        return "ReShiki drawing"

    def setClassInfo(self, class_id, name):
        if class_id.value != uuid.UUID(CLASS_ID).bytes:
            raise IOException("Cannot change the embedded drawing class.", self)

    def getComponent(self):
        return self

    def getTransferDataFlavors(self):
        return (flavor(NATIVE_MIME), flavor("image/png"))

    def isDataFlavorSupported(self, requested):
        return requested.MimeType in (NATIVE_MIME, "image/png")

    def getTransferData(self, requested):
        if not self.isDataFlavorSupported(requested):
            raise UnsupportedFlavorException("Unsupported ReShiki format.", self)
        return uno.ByteSequence(self.native if requested.MimeType == NATIVE_MIME else self.png)

    def getMapUnit(self, aspect):
        return 0  # MapUnit.MM_100TH

    def getVisualAreaSize(self, aspect):
        return size(*self.extent)

    def setVisualAreaSize(self, aspect, value):
        # This drawing does not reflow to the host's frame. Writer supplies its
        # rounded twip dimensions on each reopen; adopting them as the intrinsic
        # size causes cumulative growth. Frame scaling belongs to the container,
        # while only a newly rendered drawing changes our physical extent.
        pass

    def getPreferredVisualRepresentation(self, aspect):
        result = uno.createUnoStruct("com.sun.star.embed.VisualRepresentation")
        result.Flavor, result.Data = flavor("image/png"), uno.ByteSequence(self.png)
        return result

    def addEventListener(self, value):
        self.events.append(value)

    def removeEventListener(self, value):
        self.events = [item for item in self.events if item != value]

    def addStateChangeListener(self, value):
        self.states.append(value)

    def removeStateChangeListener(self, value):
        self.states = [item for item in self.states if item != value]

    def addCloseListener(self, value):
        self.closes.append(value)

    def removeCloseListener(self, value):
        self.closes = [item for item in self.closes if item != value]

    def event(self, name):
        event = uno.createUnoStruct("com.sun.star.document.EventObject")
        event.Source, event.EventName = self, name
        for listener in tuple(self.events):
            try:
                listener.notifyEvent(event)
            except (AttributeError, UnoRuntimeException):
                pass

    def changeState(self, value):
        if value == ACTIVE:
            self.doVerb(0)
            return
        if value not in (LOADED, RUNNING):
            raise IOException("ReShiki supports editing in its own window.", self)
        self._set_state(value)

    def _set_state(self, value, snapshot=None, finishing_session=None):
        old = self.state
        if old == value:
            return
        session = finishing_session or self.session
        active_transition = old == ACTIVE or value == ACTIVE
        if active_transition and self.client and snapshot is None:
            snapshot = self._frame_snapshot(session, writer_only=True)
        client = snapshot[0] if snapshot else self.client

        def check():
            if snapshot:
                self._require_host(session, client, snapshot[1], finishing_session is not None)

        event = uno.createUnoStruct("com.sun.star.lang.EventObject")
        event.Source = self
        failure = None
        try:
            check()
            try:
                for listener in tuple(self.states):
                    try:
                        listener.changingState(event, old, value)
                    except (WrongStateException, UnoRuntimeException):
                        pass
                    check()
            finally:
                # Notifications cannot veto a launched editor or its completed exit.
                if self.session is session or self.session is None:
                    self.state = value
            if client and active_transition:
                try:
                    client.visibilityChanged(value == ACTIVE)
                except (WrongStateException, UnoRuntimeException):
                    pass
                check()
            for listener in tuple(self.states):
                try:
                    listener.stateChanged(event, old, value)
                except UnoRuntimeException:
                    pass
                check()
            host = snapshot[1] if snapshot else client.getComponent() if client else None
            if self.disposed or self.client is not client:
                raise RuntimeError("The host drawing is no longer available.")
            writer = hasattr(host, "supportsService") and host.supportsService(
                "com.sun.star.text.TextDocument"
            )
            if self.disposed or self.client is not client:
                raise RuntimeError("The host drawing is no longer available.")
            check()
            # Writer's new native client starts with scale 1. A lifecycle-only
            # visual event can resize a reopened frame despite unchanged content.
            # Other hosts still need this repaint, notably Impress.
            if not writer:
                self.event("OnVisAreaChanged")
                check()
        except Exception as error:
            failure = error
        finally:
            if snapshot and snapshot[3] is not None:
                try:
                    check()
                    # visibilityChanged also invokes Writer's resizing ViewChanged.
                    snapshot[3].Size = size(*snapshot[4])
                    check()
                except Exception as error:
                    failure = (
                        RuntimeError(
                            str(failure) + "; could not restore the host frame: " + str(error)
                        )
                        if failure is not None
                        else error
                    )
        if failure is not None:
            raise failure

    def getReachableStates(self):
        return (LOADED, RUNNING, ACTIVE)

    def getCurrentState(self):
        return self.state

    def getSupportedVerbs(self):
        verb = uno.createUnoStruct("com.sun.star.embed.VerbDescriptor")
        verb.VerbID, verb.VerbName, verb.VerbAttributes = 0, "Edit in ReShiki", 2
        return (verb,)

    def setClientSite(self, value):
        self.client = value

    def getClientSite(self):
        return self.client

    def getStatus(self, aspect):
        return 0

    def setContainerName(self, name):
        pass

    def setUpdateMode(self, mode):
        pass

    def update(self):
        self.event("OnVisAreaChanged")

    def close(self, ownership):
        if self.session is not None:
            raise CloseVetoException(
                "Close the ReShiki editing window before closing this document.", self
            )
        event = uno.createUnoStruct("com.sun.star.lang.EventObject")
        event.Source = self
        for listener in tuple(self.closes):
            listener.queryClosing(event, ownership)
        for listener in tuple(self.closes):
            listener.notifyClosing(event)
        self.disposed = True
        self.client = None

    def dispose(self):
        self.close(True)

    def _write(self, parent, name):
        storage = parent.openStorageElement(name, 7)  # ElementModes.READWRITE
        try:
            storage.setPropertyValue("MediaType", MIME)
            metadata = json.dumps({"version": 1, "extent": list(self.extent)}).encode("utf-8")
            for entry, value in (
                ("drawing.rsk", self.native),
                ("preview.png", self.png),
                ("metadata.json", metadata),
            ):
                stream = storage.openStreamElement(entry, 12)  # WRITE | TRUNCATE
                output = stream.getOutputStream()
                output.writeBytes(uno.ByteSequence(value))
                output.closeOutput()
                stream.dispose()
            storage.commit()
        except Exception:
            storage.revert()
            raise
        finally:
            storage.dispose()

    def _queue_replacement(self, parent, name):
        # StoreAsChildren owns the image substorage during object callbacks.
        # Its handle is released before the destination's preCommit event.
        # Keep the exact PNG paired with this stored native-data snapshot.
        for transaction in tuple(self.replacements):
            if transaction.parent == parent and transaction.name == name:
                transaction.cancel()
        transaction = ReplacementTransaction(self, parent, name, self.png)
        parent.addTransactionListener(transaction)
        self.replacements.append(transaction)

    def _write_replacement(self, parent, name, png):
        # An active object's fallback is not copied by LibreOffice's Save As
        # path when link updates are disabled. Persist our already-rendered PNG
        # in the requested container so its draw:image reference stays valid.
        images = parent.openStorageElement("ObjectReplacements", 7)
        try:
            stream = images.openStreamElement(name, 12)
            output = None
            try:
                stream.setPropertyValue("MediaType", "image/png")
                stream.setPropertyValue("UseCommonStoragePasswordEncryption", True)
                output = stream.getOutputStream()
                output.writeBytes(uno.ByteSequence(png))
                output.closeOutput()
            except Exception:
                if output is not None:
                    with suppress(Exception):
                        output.closeOutput()
                with suppress(Exception):
                    stream.dispose()
                raise
            else:
                stream.dispose()
            images.commit()
        except Exception:
            with suppress(Exception):
                images.revert()
            with suppress(Exception):
                images.dispose()
            raise
        else:
            images.dispose()

    def _read(self, parent, name):
        storage = parent.openStorageElement(name, 1)
        try:

            def read(entry):
                stream = storage.openStreamElement(entry, 1).getInputStream()
                try:
                    count, raw = stream.readBytes(None, LIMIT + 1)
                    if count > LIMIT:
                        raise ValueError("Embedded data exceeds 64 MB.")
                    return raw.value
                finally:
                    stream.closeInput()

            metadata = json.loads(read("metadata.json"))
            value = {
                "version": metadata["version"],
                "extent": metadata["extent"],
                "native": base64.b64encode(read("drawing.rsk")).decode("ascii"),
                "png": base64.b64encode(read("preview.png")).decode("ascii"),
            }
            self.native, self.png, self.extent = packet(value)
        finally:
            storage.dispose()

    def storeOwn(self):
        self.persisted, self.persist_error = None, None
        try:
            self._ready()
            if self.readonly:
                raise IOException("The document is read-only.", self)
            self._write(self.parent, self.entry)
        except Exception as error:
            self.persist_error = str(error)
            raise
        self.persisted = self.native, self.png, self.extent
        self.event("OnSaveDone")

    def isReadonly(self):
        return self.readonly

    def reload(self, media, args):
        self._ready()
        self._read(self.parent, self.entry)

    def setPersistentEntry(self, parent, name, mode, media, args):
        if self.pending is not None and mode == 2:  # NO_INIT completes host Save As
            if self.pending == (parent, name):
                self.saveCompleted(True)
            else:
                # The host can reject the staged destination or select another
                # storage. Retarget before releasing a deferred edit callback.
                if (self.parent, self.entry) != (parent, name):
                    self.readonly = any(item.Name == "ReadOnly" and item.Value for item in media)
                    self.parent, self.entry = parent, name
                self.saveCompleted(False)
            return
        self._ready()
        self.readonly = any(item.Name == "ReadOnly" and item.Value for item in media)
        if mode == 0:  # DEFAULT_INIT
            self._read(parent, name)
        elif mode != 2:  # NO_INIT just changes storage ownership after a host save
            self._write(parent, name)
        self.parent, self.entry = parent, name

    def storeToEntry(self, parent, name, media, args):
        self._ready()
        self._write(parent, name)
        self._queue_replacement(parent, name)

    def storeAsEntry(self, parent, name, media, args):
        self._ready()
        self._write(parent, name)
        self._queue_replacement(parent, name)
        self.pending = (parent, name)

    def saveCompleted(self, use_new):
        if use_new and self.pending:
            self.parent, self.entry = self.pending
        elif self.pending:
            for transaction in tuple(self.replacements):
                if (transaction.parent, transaction.name) == self.pending:
                    transaction.cancel()
        self.pending = None
        if self.deferred_update:
            session, data, completed = self.deferred_update
            self.deferred_update = None
            try:
                post(self.ctx, lambda: self._accept(session, data, completed))
            except Exception as error:
                session["error"] = str(error)
                self._complete(session, completed)

    def _ready(self):
        if self.pending is not None:
            raise WrongStateException("The host has not completed Save As.", self)

    def hasEntry(self):
        return self.parent is not None

    def getEntryName(self):
        return self.entry

    def doVerb(self, verb):
        if verb not in (0, -2):
            raise IOException("Only Edit in ReShiki is supported.", self)
        if self.readonly or self.disposed:
            raise IOException("This drawing is read-only or its document has closed.", self)
        if self.session:
            session = self.session
            # Recovery for a live host whose callback service stopped accepting
            # work. Only the UI thread may release UNO listeners/action locks.
            if session["watch_complete"] and session["process"].poll() is not None:
                self._finish(session)
            return
        session = None
        try:
            program = executable(self.ctx)
            directory = Path(tempfile.mkdtemp(prefix="reshiki-libreoffice-"))
            path = directory / "drawing.rsk"
            session = {
                "token": uuid.uuid4().hex,
                "directory": directory,
                "path": path,
                "process": None,
                "guard": None,
                "invalid": False,
                "watch_complete": False,
                "error_reported": False,
                "completions": set(),
                "completion_lock": threading.Lock(),
                "accepted": self.native,
                "error": None,
            }
            self.session = session
            acquire_host_guard(self, session)
            # Reject a missing/ambiguous Writer frame before creating an editor.
            snapshot = self._frame_snapshot(session, writer_only=True)
            path.write_bytes(self.native)
            session["process"] = subprocess.Popen(
                [program, "--open", str(path), "--libreoffice-edit"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            try:
                self._set_state(ACTIVE, snapshot=snapshot)
            except Exception as error:
                session["error"] = str(error)
                raise
            finally:
                # Even an unexpected notification failure must leave the
                # launched editor watched so its saves and exit are handled.
                try:
                    threading.Thread(
                        target=self._watch, args=(session, program), daemon=True
                    ).start()
                except Exception as error:
                    # The child already exists. Explicitly abandon saveback,
                    # preserving its draft, before releasing host protection.
                    self._invalidate_session(
                        session,
                        "The editor could not be monitored; further saves in that window "
                        "will not update LibreOffice. " + str(error),
                    )
                    self._finish(session)
        except Exception as error:
            if session is not None and session["process"] is None:
                if self.session is session:
                    self.session = None
                if session["guard"] is not None:
                    session["guard"].release(self, session)
                shutil.rmtree(session["directory"], ignore_errors=True)
            show_error(self.ctx, error)

    def _session_live(self, session):
        return (
            not self.disposed
            and self.session is session
            and self.client is not None
            and not session["invalid"]
            and session["guard"].owns(self, session)
        )

    def _require_session(self, session):
        if not self._session_live(session):
            raise RuntimeError("The host drawing is no longer available.")

    def _require_host(self, session, client, host, finishing=False):
        if (
            self.disposed
            or self.client is not client
            or session is None
            or session["invalid"]
            or not (self.session is session or (finishing and self.session is None))
            or not session["guard"].owns(self, session)
            or host != session["guard"].model
        ):
            raise RuntimeError("The host drawing is no longer available.")

    def _frame_snapshot(self, session, writer_only=False):
        self._require_session(session)
        client, entry = self.client, self.entry
        host = client.getComponent()

        def check():
            self._require_host(session, client, host)

        check()
        if writer_only:
            writer = hasattr(host, "supportsService") and host.supportsService(
                "com.sun.star.text.TextDocument"
            )
            check()
            if not writer:
                return client, host, entry, None, None
        frame, dimensions = self._drawing_frame(host, entry, check)
        check()
        if writer_only and (self.pending is not None or self.entry != entry):
            raise IOException("The host changed the drawing's storage during activation.", self)
        return client, host, entry, frame, dimensions

    def _invalidate_session(self, session, reason, host_closed=False):
        with session["completion_lock"]:
            session["invalid"] = True
            session["error"] = reason
            completions = tuple(session["completions"])
            session["completions"].clear()
        if self.deferred_update and self.deferred_update[0] is session:
            completions += (self.deferred_update[2],)
            self.deferred_update = None
        if host_closed and self.session is session:
            self.disposed, self.client = True, None
        for completed in completions:
            completed.set()

    def _complete(self, session, completed):
        with session["completion_lock"]:
            session["completions"].discard(completed)
        completed.set()

    def _queue_accept(self, session, data):
        completed = threading.Event()
        # Register BEFORE posting: disposal can occur before the callback is
        # delivered, including during the host's Save As HandsOff interval.
        with session["completion_lock"]:
            if session["invalid"]:
                completed.set()
                return completed
            session["completions"].add(completed)
        try:
            post(self.ctx, lambda: self._accept(session, data, completed))
        except Exception:
            self._complete(session, completed)
            raise
        return completed

    def _watch(self, session, program):
        seen = hashlib.sha256(session["accepted"]).digest()
        try:
            while not session["invalid"] and not session["error"]:
                # Observe exit before reading: Save+exit may occur while this
                # iteration reads the previous atomic file. In that case the
                # next iteration must still read and accept the final save.
                exited = session["process"].poll() is not None
                with session["path"].open("rb") as source:
                    raw = source.read(LIMIT + 1)
                if len(raw) > LIMIT:
                    raise ValueError("The saved drawing exceeds 64 MB.")
                digest = hashlib.sha256(raw).digest()
                if digest != seen:
                    rendered = packet(worker(program, "--libreoffice-preview", raw))
                    completed = self._queue_accept(session, rendered)
                    completed.wait()
                    if session["error"]:
                        break
                    seen = digest
                if exited:
                    break
                time.sleep(0.3)
        except Exception as error:
            session["error"] = str(error)
        if session["error"]:
            with suppress(Exception):
                post(self.ctx, lambda: self._report_failure(session))
        # A render/accept failure does not mean that the editing window closed.
        # Keep its token and guard until actual exit; preserve the failed draft.
        while not session["invalid"] and session["process"].poll() is None:
            time.sleep(0.3)
        session["watch_complete"] = True
        # A transient enqueue failure must not strand the owned host lock.
        # Persistent failure remains recoverable by a later UI doVerb above;
        # never manipulate UNO state from this watcher thread.
        for attempt in range(3):
            try:
                post(self.ctx, lambda: self._finish(session))
                return
            except Exception as error:
                if not session["error"]:
                    session["error"] = (
                        "LibreOffice could not finish the editing session. "
                        "Select Edit in ReShiki again to finish recovery. " + str(error)
                    )
                if attempt < 2:
                    time.sleep(0.3)

    def _accept(self, session, data, completed):
        try:
            self._require_session(session)
        except Exception as error:
            session["error"] = str(error)
            self._complete(session, completed)
            return
        if self.pending is not None:
            # XEmbedPersist's HandsOff interval must not write the old storage.
            # The watcher serializes saves, so there can be only one waiter.
            self.deferred_update = (session, data, completed)
            return
        old = self.native, self.png, self.extent
        save_attempted = False
        deferred = False
        frame, frame_size = None, None
        client, host = self.client, None
        parent, entry = self.parent, self.entry

        def check():
            self._require_host(session, client, host)
            if save_attempted:
                if self.pending is not None:
                    raise WrongStateException("The host has not completed Save As.", self)
                if self.parent != parent or self.entry != entry:
                    raise IOException(
                        "The host changed the drawing's storage during saveback.", self
                    )

        def can_restore():
            try:
                check()
                return self.pending is None
            except Exception:
                return False

        try:
            client, host, entry, frame, frame_size = self._frame_snapshot(session)
            check()
            # UNO property reads may reenter the host's Save As callbacks.
            if self.pending is not None:
                self.deferred_update = (session, data, completed)
                deferred = True
                return
            if self.parent != parent or self.entry != entry:
                raise IOException("The host changed the drawing's storage during saveback.", self)
            self.native, self.png, self.extent = data
            save_attempted = True
            self.persisted, self.persist_error = None, None
            client.saveObject()
            check()
            # Native clients may catch a storeOwn exception and return normally.
            # Acknowledgement requires our exact drawing to have been committed.
            if self.persisted != data:
                raise IOException(
                    self.persist_error or "LibreOffice did not store the edited drawing.", self
                )
            if hasattr(host, "setModified"):
                host.setModified(True)
                check()
            self.event("OnVisAreaChanged")
            check()
            if frame is not None:
                # Use the pre-save frame, before native callbacks can reset it.
                frame.Size = size(
                    max(1, round(frame_size[0] * self.extent[0] / old[2][0])),
                    max(1, round(frame_size[1] * self.extent[1] / old[2][1])),
                )
                check()
            self.event("OnSaveDone")
            check()
            acknowledgement = {
                "version": 1,
                "accepted_sha256": hashlib.sha256(self.native).hexdigest(),
            }
            (session["directory"] / "accepted.json").write_text(
                json.dumps(acknowledgement), encoding="utf-8"
            )
            session["accepted"] = self.native
            session["guard"].accepted_update()
        except Exception as error:
            if not save_attempted and self.pending is not None and self._session_live(session):
                # A lookup interrupted by HandsOff must retain its waiter.
                if self.client is client:
                    self.deferred_update = (session, data, completed)
                    deferred = True
                    return
            if self.session is session:
                self.native, self.png, self.extent = old
            if save_attempted and can_restore():
                try:
                    try:
                        self._write(self.parent, self.entry)
                        check()
                        self.event("OnVisAreaChanged")
                    finally:
                        if frame_size is not None and can_restore():
                            frame.Size = size(*frame_size)
                            check()
                except Exception as rollback_error:
                    error = RuntimeError(
                        str(error)
                        + "; could not restore the previous stored drawing: "
                        + str(rollback_error)
                    )
            session["error"] = str(error)
        finally:
            if not deferred:
                self._complete(session, completed)

    def _drawing_frame(self, host, entry, check):
        if not hasattr(host, "supportsService"):
            return None, None
        writer = host.supportsService("com.sun.star.text.TextDocument")
        check()
        if writer:
            objects = host.getEmbeddedObjects()
            check()
            names = objects.getElementNames()
            check()
            matches = []
            for name in names:
                frame = objects.getByName(name)
                check()
                value = frame.Size
                dimensions = value.Width, value.Height
                check()
                # Writer's StreamName getter calls TryRunning. Read Size first;
                # LOADED→RUNNING must not send a synthetic geometry event.
                persisted_name = frame.StreamName
                check()
                if persisted_name == entry:
                    matches.append((frame, dimensions))
            if len(matches) == 1:
                return matches[0]
            if matches:
                raise IOException("The embedded drawing's host frame is ambiguous.", self)
            raise IOException("Cannot locate the embedded drawing's host frame.", self)
        spreadsheet = host.supportsService("com.sun.star.sheet.SpreadsheetDocument")
        check()
        drawing = spreadsheet
        if not spreadsheet:
            for name in (
                "com.sun.star.presentation.PresentationDocument",
                "com.sun.star.drawing.DrawingDocument",
            ):
                supported = host.supportsService(name)
                check()
                drawing = drawing or supported
        if not drawing:
            return None, None

        def find(shapes):
            count = shapes.getCount()
            check()
            for index in range(count):
                shape = shapes.getByIndex(index)
                check()
                persisted_name = getattr(shape, "PersistName", None)
                check()
                if persisted_name == entry:
                    return shape
                if hasattr(shape, "getCount"):
                    found = find(shape)
                    if found is not None:
                        return found
            return None

        if spreadsheet:
            sheets = host.getSheets()
            check()
            count = sheets.getCount()
            check()
            pages = []
            for index in range(count):
                sheet = sheets.getByIndex(index)
                check()
                pages.append(sheet.getDrawPage())
                check()
        else:
            collections = [host.getDrawPages()]
            check()
            if hasattr(host, "getMasterPages"):
                collections.append(host.getMasterPages())
                check()
            pages = []
            for collection in collections:
                count = collection.getCount()
                check()
                for index in range(count):
                    pages.append(collection.getByIndex(index))
                    check()
            if hasattr(host, "getHandoutMasterPage"):
                pages.append(host.getHandoutMasterPage())
                check()
        for page in pages:
            frame = find(page)
            if frame is None and hasattr(page, "getNotesPage"):
                # Inspect each notes page once; never follow notes recursively.
                notes = page.getNotesPage()
                check()
                frame = find(notes)
            if frame is not None:
                value = frame.Size
                dimensions = value.Width, value.Height
                check()
                return frame, dimensions
        raise IOException("Cannot locate the embedded drawing's host frame.", self)

    def _report_failure(self, session):
        if not session["error"] or session["error_reported"]:
            return
        session["error_reported"] = True
        try:
            show_error(
                self.ctx,
                "The LibreOffice editing session needs attention. Your saved drawing remains at:\n"
                + str(session["path"])
                + "\n\n"
                + session["error"],
            )
        except Exception:
            session["error_reported"] = False
            raise

    def _finish(self, session):
        if self.session is not session:
            return
        if not session["invalid"] and session["process"].poll() is None:
            return
        with session["completion_lock"]:
            if session["completions"] and not session["invalid"]:
                return
        if self.deferred_update and self.deferred_update[0] is session and not session["invalid"]:
            return
        failure = None
        try:
            snapshot = None
            try:
                if (
                    not self.disposed
                    and not session["invalid"]
                    and self.state == ACTIVE
                    and self.client
                ):
                    snapshot = self._frame_snapshot(session, writer_only=True)
            finally:
                # State callbacks may reenter; they must never see a finished editor.
                if self.session is session:
                    self.session = None
            if not self.disposed and not session["invalid"]:
                self._set_state(RUNNING, snapshot=snapshot, finishing_session=session)
        except Exception as error:
            failure = error
            session["error"] = (session["error"] + "; " if session["error"] else "") + str(error)
            session["error_reported"] = False
        finally:
            if self.session is None:
                self.state = RUNNING
            # Keep the recovery file until both lifecycle and host protection
            # teardown finish. A failed native unregister/unlock is not cleanup.
            try:
                session["guard"].release(self, session)
            except Exception as error:
                failure = failure or error
                session["error"] = (session["error"] + "; " if session["error"] else "") + str(
                    error
                )
            if session["guard"].unlock_error:
                session["error"] = (session["error"] + "; " if session["error"] else "") + session[
                    "guard"
                ].unlock_error
                session["error_reported"] = False
            try:
                if session["error"]:
                    self._report_failure(session)
                elif not session["guard"].retain_recovery(session):
                    shutil.rmtree(session["directory"])
            except Exception as error:
                failure = failure or error
                session["error"] = (session["error"] + "; " if session["error"] else "") + str(
                    error
                )
        if failure is not None:
            raise failure


class Factory(unohelper.Base, XEmbeddedObjectCreator, XServiceInfo):
    def __init__(self, ctx):
        self.ctx = ctx

    def getImplementationName(self):
        return FACTORY

    def supportsService(self, name):
        return name == FACTORY

    def getSupportedServiceNames(self):
        return (FACTORY,)

    def createInstanceInitNew(self, class_id, class_name, parent, name, args):
        raise IOException(
            "Use ReShiki → Paste ReShiki Drawing to insert an editable drawing.", self
        )

    def createInstanceInitFromEntry(self, parent, name, media, args):
        result = Embedded(self.ctx)
        result.setPersistentEntry(parent, name, 0, media, args)
        return result

    def createInstanceUserInit(self, class_id, class_name, parent, name, mode, media, args):
        if mode == 0:
            return self.createInstanceInitFromEntry(parent, name, media, args)
        return self.createInstanceInitNew(class_id, class_name, parent, name, args)

    def createInstanceInitFromMediaDescriptor(self, parent, name, media, args):
        raise IOException("Use Paste ReShiki Drawing to embed a drawing.", self)

    def createInstanceLink(self, parent, name, media, args):
        raise IOException("ReShiki drawings are embedded, not linked to external files.", self)


def writer_insertion_point(document):
    if not document.supportsService("com.sun.star.text.TextDocument"):
        return None
    guidance = (
        "Place a text cursor in the Writer document, then choose ReShiki → Paste ReShiki Drawing."
    )
    try:
        cursor = document.CurrentController.getViewCursor()
        owner = cursor.getText()
        position = cursor.getStart()
    except UnoRuntimeException as error:
        if error.Message != "no text selection":
            raise
        raise ValueError(guidance) from error
    if owner is None or position is None:
        raise ValueError(guidance)
    return owner, position


def insert(ctx, document, value):
    initial = packet(value)
    target = writer_insertion_point(document)
    storage = document.getDocumentStorage()
    entry = "ReShiki-" + uuid.uuid4().hex
    # Import through the public existing-storage path. Every object has its own
    # immutable entry; no shared factory seed can cross concurrent insertions.
    Embedded(ctx, initial)._write(storage, entry)
    obj, page, attached = None, None, False
    try:
        if target is not None:
            owner, position = target
            obj = document.createInstance("com.sun.star.text.TextEmbeddedObject")
            obj.StreamName = entry
            obj.AnchorType = uno.Enum("com.sun.star.text.TextContentAnchorType", "AS_CHARACTER")
            obj.Width, obj.Height = initial[2]
            owner.insertTextContent(position, obj, False)
            attached = True
        else:
            obj = document.createInstance("com.sun.star.drawing.OLE2Shape")
            if document.supportsService("com.sun.star.sheet.SpreadsheetDocument"):
                page = document.CurrentController.getActiveSheet().DrawPage
            else:
                page = document.CurrentController.getCurrentPage()
            page.add(obj)
            attached = True
            obj.PersistName = entry
            obj.Size = size(*initial[2])
        document.setModified(True)
        return obj
    except Exception:
        if attached:
            if page is not None:
                page.remove(obj)
            else:
                owner.removeTextContent(obj)
        if storage.hasByName(entry):
            storage.removeElement(entry)
        raise


def selected(document):
    selection = document.CurrentController.getSelection()
    if hasattr(selection, "getCount"):
        if selection.getCount() != 1:
            raise ValueError("Select one embedded ReShiki drawing first.")
        selection = selection.getByIndex(0)
    if hasattr(selection, "getExtendedControlOverEmbeddedObject"):
        embedded = selection.getExtendedControlOverEmbeddedObject()
    elif hasattr(selection, "EmbeddedObject"):
        embedded = selection.EmbeddedObject
    else:
        raise ValueError("Select an embedded ReShiki drawing first.")
    if embedded.getClassID().value != uuid.UUID(CLASS_ID).bytes:
        raise ValueError("The selected object is not a ReShiki drawing.")
    return embedded


class Handler(unohelper.Base, XDispatchProvider, XDispatch, XInitialization, XServiceInfo):
    def __init__(self, ctx):
        self.ctx, self.frame = ctx, None
        self.copying = False
        self.copy_failure = None

    def getImplementationName(self):
        return HANDLER

    def supportsService(self, name):
        return name in self.getSupportedServiceNames()

    def getSupportedServiceNames(self):
        return ("com.sun.star.frame.ProtocolHandler",)

    def initialize(self, args):
        if args:
            self.frame = args[0]

    def queryDispatch(self, url, target, flags):
        return self if url.Protocol == PROTOCOL else None

    def queryDispatches(self, descriptors):
        return tuple(
            self.queryDispatch(item.FeatureURL, item.FrameName, item.SearchFlags)
            for item in descriptors
        )

    def addStatusListener(self, listener, url):
        event = uno.createUnoStruct("com.sun.star.frame.FeatureStateEvent")
        event.Source, event.FeatureURL, event.IsEnabled = self, url, True
        listener.statusChanged(event)

    def removeStatusListener(self, listener, url):
        pass

    def copy(self, program, raw):
        if self.copying:
            return
        if self.copy_failure is not None:
            feedback, error = self.copy_failure
            self.copy_failure = None
            # A rejected callback cannot safely update UI from its worker.
            # Report on this next UNO-thread request before accepting a retry.
            feedback.finish(False)
            show_error(self.ctx, error)
            if self.copying:  # The modal notice may allow a reentrant copy.
                return
        host_clipboard = uses_host_clipboard()
        if host_clipboard:
            raw = bounded_clipboard_bytes(raw)
        frame = self.frame
        self.copying = True
        feedback = CopyFeedback(self.ctx, frame)

        def finished(error=None):
            self.copying = False
            feedback.finish(error is None)
            if error is not None:
                show_error(self.ctx, error)

        def execute():
            error = None
            drawing = None
            try:
                if host_clipboard:
                    native, png, _ = packet(worker(program, "--libreoffice-preview", raw))
                    if native != raw:
                        raise ValueError(
                            "ReShiki returned a different drawing while preparing copy."
                        )
                    drawing = ClipboardDrawing(native, png)
                else:
                    worker(program, "--libreoffice-copy", raw)
            except Exception as failure:
                error = str(failure)

            def complete():
                if error is not None or not host_clipboard:
                    finished(error)
                    return
                try:
                    publish_host_clipboard(self.ctx, frame, drawing, finished)
                except Exception as failure:
                    finished(str(failure))

            # Linux workers prepare bytes only; the focused host owns its
            # clipboard. Other platforms retain their native helper route.
            try:
                post(self.ctx, complete)
            except Exception as failure:
                # Publish the deferred notice before releasing the busy gate;
                # neither clipboard nor feedback belongs on this worker.
                self.copy_failure = (feedback, error or str(failure))
                self.copying = False

        try:
            threading.Thread(target=execute, daemon=True).start()
        except Exception as error:
            finished(str(error))

    def dispatch(self, url, args):
        try:
            if url.Path == "configure":
                picker = service(self.ctx, "com.sun.star.ui.dialogs.FilePicker")
                picker.setTitle("Choose the ReShiki executable")
                if picker.execute():
                    path = Path(uno.fileUrlToSystemPath(picker.getFiles()[0]))
                    if path.suffix == ".app":
                        path = path / "Contents/MacOS/reshiki"
                    if not path.is_file():
                        raise ValueError("Select the ReShiki application executable.")
                    settings_path(self.ctx).write_text(
                        json.dumps({"executable": str(path.resolve())}), encoding="utf-8"
                    )
                picker.dispose()
                return
            document = self.frame.getController().getModel()
            program = executable(self.ctx)
            if url.Path == "edit":
                selected(document).doVerb(0)
                return
            if url.Path == "copy":
                raw = selected(document).getComponent().getTransferData(flavor(NATIVE_MIME)).value
                self.copy(program, raw)
                return
            if url.Path == "paste":
                # Reject nontext Writer selections before rendering. Insertion
                # reacquires the target because selection can change meanwhile.
                writer_insertion_point(document)
            native = None
            if url.Path == "paste" and uses_host_clipboard():
                # Capture the current clipboard on the dispatch thread before
                # asynchronous validation; a later owner must not change it.
                native = read_host_native(self.ctx)

            def execute():
                try:
                    if url.Path == "paste":
                        if native is None:
                            value = worker(program, "--libreoffice-clipboard")
                        else:
                            value = worker(program, "--libreoffice-preview", native)
                            if packet(value)[0] != native:
                                raise ValueError("ReShiki returned a different clipboard drawing.")

                        def apply():
                            try:
                                insert(self.ctx, document, value)
                            except Exception as error:
                                show_error(self.ctx, error)

                        post(self.ctx, apply)
                except Exception as error:
                    post(self.ctx, lambda message=str(error): show_error(self.ctx, message))

            threading.Thread(target=execute, daemon=True).start()
        except Exception as error:
            show_error(self.ctx, error)


g_ImplementationHelper = unohelper.ImplementationHelper()
g_ImplementationHelper.addImplementation(Factory, FACTORY, (FACTORY,))
g_ImplementationHelper.addImplementation(Handler, HANDLER, ("com.sun.star.frame.ProtocolHandler",))
