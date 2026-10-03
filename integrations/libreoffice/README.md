# Editable ReShiki drawings in LibreOffice

This optional UNO extension embeds native ReShiki data, a PNG preview, and its
physical size in ODT, ODS, and ODP documents. The same object class and storage
format are used on Windows, Linux, and macOS. Windows Microsoft Office OLE uses
its existing separate integration.

## Requirements and installation

- LibreOffice 25.8 or later with its Python UNO loader. The official Windows and
  macOS distributions include this runtime. Linux distributions may package it
  separately (for example, `python3-uno`); install the matching distribution
  package. This is a dependency of the optional extension, not of ReShiki itself.
- A ReShiki build with the `--libreoffice-preview`, `--libreoffice-clipboard`, and
  `--libreoffice-copy` workers. Linux also needs the native X11/Wayland clipboard
  backend. An older ReShiki executable cannot serve this extension.
- Both machines need the extension and ReShiki to edit an exchanged object.
  The ODF document contains its preview and drawing independently of the editor's
  temporary directory.

Build the package with Python 3 (standard library only):

```sh
python3 integrations/libreoffice/build_extension.py dist/ReShiki-LibreOffice.oxt
```

In LibreOffice, open **Tools → Extensions**, add the `.oxt`, then restart
LibreOffice. The new **ReShiki** menu contains **Choose ReShiki App** for selecting
the installed executable. On macOS select the executable inside
`ReShiki.app/Contents/MacOS/reshiki` if the application is not in `/Applications`.
The selected path is stored in the LibreOffice user profile, never in documents.

## Use

1. Copy an editable drawing in ReShiki.
2. In Writer, Calc, or Impress, choose **ReShiki → Paste ReShiki Drawing**.
3. Double-click the object, or select it and choose **Edit in ReShiki**.
4. Save in ReShiki. The extension validates the saved native drawing in a bounded
   subprocess and asks LibreOffice to accept the new data and preview.
5. Close the ReShiki edit window and save the LibreOffice document.
6. To recover the native drawing, select the object and choose
   **ReShiki → Copy Editable Drawing**. Wait for **Editable drawing copied —
   Ready to paste in ReShiki**, then paste in ReShiki. Preparing the clipboard
   runs in the background; the copying message remains until it finishes.
   On Linux, keep the LibreOffice drawing window active until it finishes. If
   focus changes during preparation, return to that window and copy again.

Each edited object gets a separate process, directory, and session identity.
Unsaved editor changes do not replace the embedded drawing. LibreOffice refuses
to close the document or its editing window while an editor session remains open.
Multiple objects can be edited from the same LibreOffice window; starting an
editor from another window of that same document is rejected until editing in
the first window finishes. If an update
fails, the extension retains the saved `.rsk` file and shows its path. Only an
accepted update receives a revision acknowledgement; ReShiki's save message asks
the user to check the host document.

If LibreOffice repeatedly rejects the final cleanup callback after the editor
closes, select the object and choose **Edit in ReShiki** to finish the retained session
and show its recovery path. Choose the command again to start another edit.
Automatic cleanup is not guaranteed while that host service is unavailable.
Close-listener vetoes queue a concise notice; the separate frame-lock route to
Start Center may refuse closing without displaying that notice.

If releasing the extension's frame action lock throws, its native lock count may
already have changed. The extension leaves that document protected and does not
retry the removal or reset other locks. Save your work before restarting
LibreOffice; **Edit in ReShiki** cannot repair this separate failure.

The extension menu is the explicit editable paste/copy route. Ordinary platform
paste may choose an image or Windows OLE instead. DOCX, XLSX, PPTX, native
ONLYOFFICE editing, in-place editing, and single-instance editor forwarding are
outside this ODF adapter's contract.

On Linux, the extension reads and publishes clipboard data through LibreOffice's
own desktop clipboard backend. ReShiki's background worker only validates the
captured native drawing and renders its preview. Copy publishes an independent
native drawing and PNG snapshot, limited to 64 MB combined; later document edits
or closure do not change those offered bytes. A preparation failure leaves the
previous clipboard intact. **Copy Image** is a separate static-image route;
**Paste ReShiki Drawing** requires native drawing data and rejects an image-only
clipboard. Windows and macOS retain their native ReShiki clipboard workers.

## Validation status

The installed package has passed headless Writer, Calc, and Impress checks on
macOS LibreOffice 25.8.4.2, Linux LibreOffice 25.8.7, and Windows LibreOffice
26.8.0.3. Each platform used the real ReShiki preview worker and preserved two
distinct objects through two save/close/reopen cycles, with byte-identical native
data and PNG and unchanged intrinsic extent. All six directions of ODF exchange
between the three platforms also passed import, save, close, and reopen checks.
Both persistence and interchange checks verify actual host frame dimensions
within 0.03 mm to allow host-unit rounding. Writer insertion initializes its
frame from the drawing's physical size.
Rendering a drawing again can use different locally installed fonts; exchanging
the document preserves its stored preview and extent.

The latest real-worker persistence checks used these immutable executables:

| Platform | ReShiki source | Executable SHA-256                                                 |
| -------- | -------------- | ------------------------------------------------------------------ |
| macOS    | `ca4c52e`      | `36eab2b3da9ee879f01aac8d31aec38e2358b8b771655af1a9d0af38fbcfb507` |
| Linux    | `ca4c52e`      | `2c526a98eab0c6e2ad6a5931caaf3eae59a50f2b43dc24cbfba5cdcfe6fc2b31` |
| Windows  | `ca4c52e`      | `8f4badabc41c5d623165ed5f7f47227e463e4e622f99ea91370455440d3c385c` |

The persistence and interchange adapter source SHA-256 was
`4c7d9cacec62eaa1b59175f462376c0b376bbd9dcde99c00fc0b954107d47531`.

The subsequent Save As completion fix was checked with adapter SHA-256
`bd2c358d4f2040acbd79790b432ac80df86eefc164a3160f5b7b46d0117ace4c`
and the same `ca4c52e` preview workers. On all three operating systems, a
scripted editor exercised save-back after saving a new host document and
keeping it open. Writer, Calc, and Impress passed. These checks verified
the host's acceptance receipt, updated native data, PNG, intrinsic extent and
host frame dimensions, isolation from a second object, temporary-file cleanup,
and persistence after reopening. The Windows surrogate retries atomic
replacement while a watcher read handle is open; its result does not establish
the behavior of the actual ReShiki Save command. Desktop double-click activation, editing in
the actual ReShiki window, and system clipboard exchange remain separate
acceptance checks.

`tests/test_session.py` runs with matching Python UNO bindings and covers the
final-save/process-exit race, a host failure after committing storage, deferred
acceptance during Save As including the host's `NO_INIT` completion order,
stale session rejection, and intrinsic size
preservation across host rounding. `tests/roundtrip.py`
connects to an isolated headless LibreOffice profile with the extension installed
and checks the actual package's source hash, object identities, data, PNG,
intrinsic extent, host frame dimensions, save/reopen, and transfer-data copy-back.
For example:

```sh
python3 integrations/libreoffice/tests/roundtrip.py \
  --uno-url 'uno:pipe,name=reshiki-test;urp;StarOffice.ComponentContext' \
  --reshiki /path/to/reshiki --drawing tests/fixtures/ui-drawn-ethanol.reshiki \
  --output /tmp/reshiki-roundtrip
```

`--packet` accepts a saved preview-worker JSON response instead of running the
renderer; reports identify that limitation. `--incoming` also loads and resaves
the `roundtrip-2.odt`, `.ods`, and `.odp` files from another test host. These are
headless integration checks, not substitutes for the required desktop workflow.

`tests/test_host_guard.py` exercises model/frame close broadcasts, shared session
tokens, balanced frame locks, source-specific close ownership, Cancel/error
retry limits, live-child failures, and disposal before or during deferred
saveback. It also checks a new close request after Cancel, retained duties across
later views, immediate failure notices, and bounded final-callback recovery.
These controlled tests do not establish native close-dispatch or
Save/Discard/Cancel behavior; the Writer edit → Save As → close while editing
sequence must also be checked in the desktop application.

`tests/test_copy_feedback.py` checks that copy readiness is announced only after
the worker succeeds, failures retain their error message, and hosts without
infobars use a temporary status indicator without interrupting clipboard work.

`tests/test_host_clipboard.py` checks Linux host clipboard snapshots, type and
size limits, focus loss, validation and publication failures, and original
document identity. Linux copy completion waits until a later host callback
verifies the retained native and PNG bytes. This host readback does not by
itself prove compositor acceptance: native X11 and standard Wayland tests must
also retrieve the data in a separate application. These method tests do not
access a desktop clipboard.

`tests/saveback.py` checks the save-new-document → keep open → edit → accept →
save/close/reopen sequence. Compile `tests/editor_surrogate.c` into a dedicated
test directory, then pass its path with `--editor-surrogate` along with the
same `--uno-url`, `--reshiki`, `--drawing`, and `--output` options. Use only an
empty isolated LibreOffice profile; the driver restores its previous editor
setting after the check. The surrogate writes test fixture data and must not be
selected as a user's editor.

The immutable object class is `8E86A932-EBBE-4E9F-8D26-CA2D82096856`. Its ODF
substorage media type is `application/vnd.reshiki.embedded-drawing`; entries are
`drawing.rsk`, `preview.png`, and versioned `metadata.json`. No executable paths,
macros, or external file links are stored in that object.
