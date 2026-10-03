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
from com.sun.star.awt import XCallback
from com.sun.star.datatransfer import UnsupportedFlavorException, XTransferable
from com.sun.star.embed import (
    WrongStateException,
    XEmbeddedObject,
    XEmbeddedObjectCreator,
    XEmbedPersist,
    XTransactionListener,
)
from com.sun.star.embed.EmbedStates import ACTIVE, LOADED, RUNNING
from com.sun.star.frame import XDispatch, XDispatchProvider, XDispatchResultListener
from com.sun.star.frame.InfobarType import DANGER, INFO
from com.sun.star.io import IOException
from com.sun.star.lang import XComponent, XInitialization, XServiceInfo
from com.sun.star.uno import RuntimeException as UnoRuntimeException
from com.sun.star.util import CloseVetoException, XCloseable, XCloseListener

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


def show_error(ctx, message):
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
RETIRED_HOST_GUARDS = []  # No sessions: retain only old source-specific close duties.


class HostCloseListener(unohelper.Base, XCloseListener):
    def __init__(self, guard, kind, source):
        self.guard, self.kind, self.source = guard, kind, source
        self.attached, self.alive = False, True

    def queryClosing(self, event, ownership):
        if event.Source == self.source and self.guard.blocks_close():
            if ownership:
                # A fresh veto can occur in a later edit after an earlier retry
                # was cancelled. Its new ownership must receive a new retry.
                self.guard.obligations[self.kind] = False
            self.guard.notice_veto()
            raise CloseVetoException(
                self.guard.unlock_error
                or "Close the ReShiki editing window before closing this document.",
                self,
            )

    def notifyClosing(self, event):
        if event.Source == self.source:
            self.guard.source_closed(self.kind)

    def disposing(self, event):
        self.notifyClosing(event)


class CloseResult(unohelper.Base, XDispatchResultListener):
    def __init__(self, guard, kind):
        self.guard, self.kind = guard, kind

    def dispatchFinished(self, event):
        # Even SUCCESS is not a source-closure notification: the last CloseWin
        # can replace the controller with Start Center in the same live frame.
        self._complete()

    def disposing(self, event):
        self._complete()

    def _complete(self):
        if self.guard.inflight.get(self.kind) is self:
            del self.guard.inflight[self.kind]
            # Only a genuinely new TRUE veto can have rearmed this duty while
            # its previous result was pending. Cancel alone does not retry.
            self.guard._settle()


class HostGuard:
    """UI-thread lifetime protection for one model and its active edit frame.

    A second active view is deliberately rejected before launching an editor.
    Multiple embedded objects in the same view share one owned action lock.
    """

    def __init__(self, ctx, model, frame):
        self.ctx, self.model, self.frame = ctx, model, frame
        self.controller = frame.getController()
        self.listeners = {
            "model": HostCloseListener(self, "model", model),
            "frame": HostCloseListener(self, "frame", frame),
        }
        self.sessions, self.obligations, self.inflight = {}, {}, {}
        self.scheduled = set()
        self.phase, self.locked, self.invalid = "IDLE", False, False
        self.notice_pending = False
        self.unlock_error = None

    def blocks_close(self):
        return bool(self.sessions) or self.phase in ("ACQUIRING", "TEARING_DOWN")

    def notice_veto(self):
        if self.notice_pending:
            return
        self.notice_pending = True

        def notify():
            try:
                if self.blocks_close():
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
                    show_error(self.ctx, message)
            finally:
                self.notice_pending = False

        try:
            post(self.ctx, notify)
        except Exception:
            # Reporting failure must never turn a veto into permission to close.
            self.notice_pending = False

    def owns(self, owner, session):
        registered = self.sessions.get(session["token"])
        return (
            not self.invalid
            and registered is not None
            and registered[0] is owner
            and registered[1] is session
            and all(listener.alive for listener in self.listeners.values())
        )

    def acquire(self, owner, session, frame):
        if self.unlock_error:
            raise RuntimeError(self.unlock_error)
        if self.invalid or self.phase == "TEARING_DOWN":
            raise RuntimeError("The document's editing window is no longer available.")
        if frame != self.frame:
            raise RuntimeError(
                "Finish ReShiki editing in the other LibreOffice window for this document "
                "before editing from this window."
            )
        self.sessions[session["token"]] = (owner, session)
        session["guard"] = self
        self.phase = "ACQUIRING"
        try:
            for listener in self.listeners.values():
                if not listener.attached:
                    listener.source.addCloseListener(listener)
                    listener.attached = listener.alive
                    if self.invalid:
                        raise RuntimeError("The document closed while preparing the editor.")
            if not self.locked:
                self.frame.addActionLock()
                self.locked = True
            if self.invalid:
                raise RuntimeError("The document closed while preparing the editor.")
            self.phase = "ACTIVE"
        except Exception:
            if self.sessions:
                self.release(owner, session)
            else:
                self._unprotect()
            raise

    def release(self, owner, session):
        registered = self.sessions.get(session["token"])
        if registered is None or registered[0] is not owner or registered[1] is not session:
            return
        del self.sessions[session["token"]]
        if self.sessions:
            self.phase = "ACTIVE"
            return
        self._unprotect()

    def _unprotect(self):
        # removeActionLock may synchronously call close(True) for a remembered
        # self-close. Keep BOTH listeners vetoing until that call returns.
        self.phase = "TEARING_DOWN"
        if self.unlock_error:
            return  # The native decrement may already have happened. Never retry it.
        if self.locked:
            try:
                self.frame.removeActionLock()
            except Exception as error:
                self.unlock_error = (
                    "LibreOffice could not safely release this document's editing lock. "
                    "The window remains protected. Save your work before restarting LibreOffice. "
                    "Edit in ReShiki cannot clear this state. " + str(error)
                )
                self.notice_veto()
                return  # Keep protection; the owned lock count is now uncertain.
            self.locked = False
        self.phase = "IDLE"
        self._settle()

    def _settle(self):
        if self.blocks_close():
            return
        for kind, listener in self.listeners.items():
            if kind not in self.obligations and listener.attached:
                if listener.alive:
                    listener.source.removeCloseListener(listener)
                listener.attached = False
        if not self.obligations:
            HOST_GUARDS[:] = [guard for guard in HOST_GUARDS if guard is not self]
            RETIRED_HOST_GUARDS[:] = [guard for guard in RETIRED_HOST_GUARDS if guard is not self]
            return
        if not self.invalid and not self.scheduled and not self.inflight:
            pending = [kind for kind in ("model", "frame") if self.obligations.get(kind) is False]
            if pending:
                kind = pending[0]
                self.scheduled.add(kind)
                post(self.ctx, lambda: self._retry(kind))

    def _retry(self, scheduled_kind):
        self.scheduled.discard(scheduled_kind)
        pending = [kind for kind in ("model", "frame") if self.obligations.get(kind) is False]
        if self.blocks_close() or self.invalid or not pending:
            return
        # Another edit/TRUE request can arrive while this callback is queued.
        # Select the currently authorized scope, not the originally queued one.
        kind = pending[0]
        # One UI decision for this batch of transferred obligations. If both
        # sources transferred ownership, CloseDoc is the authorized wider scope.
        # Cancel/save failure must not immediately prompt again via CloseWin.
        for source in tuple(self.obligations):
            if kind == "model" or source == "frame":
                self.obligations[source] = True
        try:
            controller = self.frame.getController()
            if controller != self.controller or controller.getModel() != self.model:
                raise RuntimeError("The original document window is no longer available.")
            url = uno.createUnoStruct("com.sun.star.util.URL")
            url.Complete = ".uno:CloseDoc" if kind == "model" else ".uno:CloseWin"
            url.Protocol, url.Path = ".uno:", url.Complete[5:]
            dispatch = self.frame.queryDispatch(url, "_self", 0)
            if dispatch is None or not hasattr(dispatch, "dispatchWithNotification"):
                raise RuntimeError("LibreOffice cannot confirm the requested close operation.")
            result = CloseResult(self, kind)
            self.inflight[kind] = result
            dispatch.dispatchWithNotification(url, (), result)
        except Exception as error:
            self.inflight.pop(kind, None)
            show_error(
                self.ctx, "The document remains open. Close it again when ready.\n" + str(error)
            )

    def source_closed(self, kind):
        listener = self.listeners[kind]
        if not listener.alive:
            return
        listener.alive, listener.attached = False, False
        self.obligations.pop(kind, None)
        self.inflight.pop(kind, None)
        self.invalid = True
        for owner, session in tuple(self.sessions.values()):
            owner._invalidate_session(
                session, "The LibreOffice document or editing window closed.", True
            )
        self.sessions.clear()
        if kind == "frame":
            self.locked = False  # A disposed frame owns no releasable lock.
            self.unlock_error = None
        self._unprotect()


def acquire_host_guard(owner, session):
    if owner.client is None:
        raise RuntimeError("The drawing has no LibreOffice document.")
    model = owner.client.getComponent()
    frame = model.getCurrentController().getFrame()
    guard = next((item for item in HOST_GUARDS if item.model == model), None)
    if guard is not None and frame != guard.frame and not guard.blocks_close():
        # A cancelled close can leave duties after all editors have finished.
        # Keep those exact old sources alive without treating them as the new
        # launch frame. Their later disposal must not invalidate new sessions.
        HOST_GUARDS[:] = [item for item in HOST_GUARDS if item is not guard]
        RETIRED_HOST_GUARDS.append(guard)
        guard = None
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
            try:
                if session["error"]:
                    self._report_failure(session)
                else:
                    shutil.rmtree(session["directory"])
            except Exception as error:
                failure = failure or error
                session["error"] = (session["error"] + "; " if session["error"] else "") + str(
                    error
                )
            finally:
                session["guard"].release(self, session)
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
            post(self.ctx, complete)

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
