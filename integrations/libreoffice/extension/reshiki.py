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
from com.sun.star.frame import XDispatch, XDispatchProvider
from com.sun.star.io import IOException
from com.sun.star.lang import XComponent, XInitialization, XServiceInfo
from com.sun.star.uno import RuntimeException as UnoRuntimeException
from com.sun.star.util import CloseVetoException, XCloseable

CLASS_ID = "8E86A932-EBBE-4E9F-8D26-CA2D82096856"
MIME = "application/vnd.reshiki.embedded-drawing"
NATIVE_MIME = "application/x-reshiki-drawing+json"
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

    def _set_state(self, value):
        old = self.state
        if old == value:
            return
        event = uno.createUnoStruct("com.sun.star.lang.EventObject")
        event.Source = self
        try:
            for listener in tuple(self.states):
                try:
                    listener.changingState(event, old, value)
                except (WrongStateException, UnoRuntimeException):
                    pass
        finally:
            # Notifications cannot veto a launched editor or its completed exit.
            self.state = value
        if self.client and (old == ACTIVE or value == ACTIVE):
            try:
                self.client.visibilityChanged(value == ACTIVE)
            except (WrongStateException, UnoRuntimeException):
                pass
        for listener in tuple(self.states):
            try:
                listener.stateChanged(event, old, value)
            except UnoRuntimeException:
                pass
        # Impress caches no replacement image for an in-place UI state. Tell
        # the host to rebuild its view after every actual lifecycle transition.
        self.event("OnVisAreaChanged")

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
            post(self.ctx, lambda: self._accept(session, data, completed))

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
            return
        try:
            program = executable(self.ctx)
            directory = Path(tempfile.mkdtemp(prefix="reshiki-libreoffice-"))
            path = directory / "drawing.rsk"
            path.write_bytes(self.native)
            process = subprocess.Popen(
                [program, "--open", str(path), "--libreoffice-edit"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            session = {
                "token": uuid.uuid4().hex,
                "directory": directory,
                "path": path,
                "process": process,
                "accepted": self.native,
                "error": None,
            }
            self.session = session
            try:
                self._set_state(ACTIVE)
            finally:
                # Even an unexpected notification failure must leave the
                # launched editor watched so its saves and exit are handled.
                threading.Thread(target=self._watch, args=(session, program), daemon=True).start()
        except Exception as error:
            show_error(self.ctx, error)

    def _watch(self, session, program):
        seen = hashlib.sha256(session["accepted"]).digest()
        try:
            while True:
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
                    completed = threading.Event()
                    post(
                        self.ctx,
                        lambda data=rendered, done=completed: self._accept(session, data, done),
                    )
                    completed.wait()
                    if session["error"]:
                        break
                    seen = digest
                if exited:
                    break
                time.sleep(0.3)
        except Exception as error:
            session["error"] = str(error)
        post(self.ctx, lambda: self._finish(session))

    def _accept(self, session, data, completed):
        if self.pending is not None:
            # XEmbedPersist's HandsOff interval must not write the old storage.
            # The watcher serializes saves, so there can be only one waiter.
            self.deferred_update = (session, data, completed)
            return
        old = self.native, self.png, self.extent
        save_attempted = False
        frame, frame_size = None, None
        try:
            if self.disposed or self.session is not session or not self.client:
                raise RuntimeError("The host drawing is no longer available.")
            self.native, self.png, self.extent = data
            save_attempted = True
            self.persisted, self.persist_error = None, None
            self.client.saveObject()
            # Native clients may catch a storeOwn exception and return normally.
            # Acknowledgement requires our exact drawing to have been committed.
            if self.persisted != data:
                raise IOException(
                    self.persist_error or "LibreOffice did not store the edited drawing.", self
                )
            host = self.client.getComponent()
            frame = self._drawing_frame(host)
            if frame is not None:
                frame_size = frame.Size.Width, frame.Size.Height
            if hasattr(host, "setModified"):
                host.setModified(True)
            self.event("OnVisAreaChanged")
            if frame is not None:
                # Calc/Impress full clients only repaint an external editor's
                # changed view. Preserve the user's scale when sizing its frame.
                frame.Size = size(
                    max(1, round(frame_size[0] * self.extent[0] / old[2][0])),
                    max(1, round(frame_size[1] * self.extent[1] / old[2][1])),
                )
            self.event("OnSaveDone")
            acknowledgement = {
                "version": 1,
                "accepted_sha256": hashlib.sha256(self.native).hexdigest(),
            }
            (session["directory"] / "accepted.json").write_text(
                json.dumps(acknowledgement), encoding="utf-8"
            )
            session["accepted"] = self.native
        except Exception as error:
            self.native, self.png, self.extent = old
            if save_attempted:
                try:
                    try:
                        self._write(self.parent, self.entry)
                        self.event("OnVisAreaChanged")
                    finally:
                        if frame_size is not None:
                            frame.Size = size(*frame_size)
                except Exception as rollback_error:
                    error = RuntimeError(
                        str(error)
                        + "; could not restore the previous stored drawing: "
                        + str(rollback_error)
                    )
            session["error"] = str(error)
        finally:
            completed.set()

    def _drawing_frame(self, host):
        if not hasattr(host, "supportsService"):
            return None
        spreadsheet = host.supportsService("com.sun.star.sheet.SpreadsheetDocument")
        if not spreadsheet and not any(
            host.supportsService(name)
            for name in (
                "com.sun.star.presentation.PresentationDocument",
                "com.sun.star.drawing.DrawingDocument",
            )
        ):
            return None

        def find(shapes):
            for index in range(shapes.getCount()):
                shape = shapes.getByIndex(index)
                if getattr(shape, "PersistName", None) == self.entry:
                    return shape
                if hasattr(shape, "getCount"):
                    found = find(shape)
                    if found is not None:
                        return found
            return None

        if spreadsheet:
            sheets = host.getSheets()
            pages = [sheets.getByIndex(index).getDrawPage() for index in range(sheets.getCount())]
        else:
            collections = [host.getDrawPages()]
            if hasattr(host, "getMasterPages"):
                collections.append(host.getMasterPages())
            pages = [
                collection.getByIndex(index)
                for collection in collections
                for index in range(collection.getCount())
            ]
            if hasattr(host, "getHandoutMasterPage"):
                pages.append(host.getHandoutMasterPage())
        for page in pages:
            frame = find(page)
            if frame is None and hasattr(page, "getNotesPage"):
                # Inspect each notes page once; never follow notes recursively.
                frame = find(page.getNotesPage())
            if frame is not None:
                return frame
        raise IOException("Cannot locate the embedded drawing's host frame.", self)

    def _finish(self, session):
        if self.session is not session:
            return
        if session["error"]:
            show_error(
                self.ctx,
                "The update was not accepted by LibreOffice. Your saved drawing remains at:\n"
                + str(session["path"])
                + "\n\n"
                + session["error"],
            )
        else:
            shutil.rmtree(session["directory"])
        self.session = None
        self._set_state(RUNNING)


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


def insert(ctx, document, value):
    initial = packet(value)
    storage = document.getDocumentStorage()
    entry = "ReShiki-" + uuid.uuid4().hex
    # Import through the public existing-storage path. Every object has its own
    # immutable entry; no shared factory seed can cross concurrent insertions.
    Embedded(ctx, initial)._write(storage, entry)
    obj, page, attached = None, None, False
    try:
        if document.supportsService("com.sun.star.text.TextDocument"):
            obj = document.createInstance("com.sun.star.text.TextEmbeddedObject")
            obj.StreamName = entry
            obj.AnchorType = uno.Enum("com.sun.star.text.TextContentAnchorType", "AS_CHARACTER")
            obj.Width, obj.Height = initial[2]
            cursor = document.CurrentController.getViewCursor()
            document.Text.insertTextContent(cursor, obj, False)
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
                document.Text.removeTextContent(obj)
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
            raw = (
                selected(document).getComponent().getTransferData(flavor(NATIVE_MIME)).value
                if url.Path == "copy"
                else b""
            )

            def execute():
                try:
                    if url.Path == "paste":
                        value = worker(program, "--libreoffice-clipboard")

                        def apply():
                            try:
                                insert(self.ctx, document, value)
                            except Exception as error:
                                show_error(self.ctx, error)

                        post(self.ctx, apply)
                    elif url.Path == "copy":
                        worker(program, "--libreoffice-copy", raw)
                except Exception as error:
                    post(self.ctx, lambda message=str(error): show_error(self.ctx, message))

            threading.Thread(target=execute, daemon=True).start()
        except Exception as error:
            show_error(self.ctx, error)


g_ImplementationHelper = unohelper.ImplementationHelper()
g_ImplementationHelper.addImplementation(Factory, FACTORY, (FACTORY,))
g_ImplementationHelper.addImplementation(Handler, HANDLER, ("com.sun.star.frame.ProtocolHandler",))
