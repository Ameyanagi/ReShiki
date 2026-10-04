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
4. Save in ReShiki. The extension validates the saved native drawing and asks
   LibreOffice to accept the new data and preview.
5. Close the ReShiki edit window and save the LibreOffice document.
6. To recover the native drawing, select the object and choose
   **ReShiki → Copy Editable Drawing**. Wait for **Editable drawing copied —
   Ready to paste in ReShiki**, then paste in ReShiki. Preparing the clipboard
   runs in the background; the copying message remains until it finishes.
   On Linux, keep the LibreOffice drawing window active until it finishes. If
   focus changes during preparation, return to that window and copy again.

Each edited object opens in its own ReShiki window. Unsaved editor changes do
not replace the embedded drawing. Finish editing and close the ReShiki window
before closing the LibreOffice document. Multiple objects can be edited from
the same LibreOffice window; starting an editor from another window of that same
document is rejected until editing in the first window finishes. Check the
LibreOffice document after saving in ReShiki. If an update fails, the extension
retains the saved `.rsk` file and shows its recovery path.

After a blocked close, finish editing and try closing again when ready. You may
see a fresh **Save and Close**, **Don't Save and Close**, or **Cancel** confirmation.
It names whether you are closing this window or the document and all of its
windows. Saving applies to the document; only the named windows are closed.
Cancel, including cancellation of Save As, keeps the document open. LibreOffice
may also show its own save confirmation; its Cancel choice keeps the document
open too. The extension does not repeat an earlier application-wide Quit request.
Choose Quit again afterward if you still want to exit LibreOffice.

After an interrupted close, a saved `.rsk` draft can remain after the ReShiki
window closes, including when a later close is cancelled or saving fails. Saving
the LibreOffice document alone does not necessarily remove this recovery copy.
If an error is reported, use the displayed recovery path to find the drawing.

If the ReShiki window has closed but LibreOffice says editing has not finished,
select the object and choose **Edit in ReShiki** to finish recovery and show the
saved drawing's path. Choose the command again to start another edit. If
LibreOffice instead reports that it could not safely protect or release the
document, save your work before restarting LibreOffice; **Edit in ReShiki**
cannot clear that separate failure.

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

The earlier adapter versions identified below passed headless Writer, Calc, and
Impress checks on macOS LibreOffice 25.8.4.2, Linux LibreOffice 25.8.7, and
Windows LibreOffice 26.8.0.3. Each platform used the real ReShiki preview worker
and preserved two distinct objects through two save/close/reopen cycles, with
byte-identical native data and PNG and unchanged intrinsic extent. All six
directions of ODF exchange between the three platforms also passed import, save,
close, and reopen checks.
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

`tests/test_close_dispatch.py` checks that normal close commands are blocked
while an editor is open. `tests/test_deferred_close.py` covers fresh Save/Discard/Cancel
choices after a blocked close, native Quit ordering, Save As cancellation,
window versus document scope, additional or replaced views, changes during a
confirmation, one close attempt per choice, and retained recovery drafts.

The close/save/discard update, adapter SHA-256
`a7330ece19f98a7d25ce89023e40f575ecf7022ab669e022657bd7a0cc877096`,
passed all 150 controlled tests without skips using the installed Python UNO
runtimes on macOS LibreOffice 25.8.4.2, Linux LibreOffice 26.2.6.3, and Windows
LibreOffice 26.8.0.3. Native macOS Writer checks also verified that closing while
editing is blocked before save preparation. After a second accepted edit,
fresh Cancel kept the document open, fresh Save persisted that edit through
reopening, and fresh Don't Save left the file byte-identical to its saved
baseline. The second object and the first object's original scale were preserved.
These checks cover the ordinary window-close path. Full desktop acceptance
across all three platforms remains pending; application Quit, Save As
cancellation, and multiple document windows require their own native checks.
The earlier platform checks above apply to the source hashes recorded there.

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
