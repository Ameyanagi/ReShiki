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
   **ReShiki → Copy Editable Drawing**, then paste in ReShiki.

Each edited object gets a separate process, directory, and session identity.
Unsaved editor changes do not replace the embedded drawing. LibreOffice refuses
to close an active object while its editor session remains open. If an update
fails, the extension retains the saved `.rsk` file and shows its path. Only an
accepted update receives a revision acknowledgement; ReShiki's save message asks
the user to check the host document.

The extension menu is the explicit editable paste/copy route. Ordinary platform
paste may choose an image or Windows OLE instead. DOCX, XLSX, PPTX, native
ONLYOFFICE editing, in-place editing, and single-instance editor forwarding are
outside this ODF adapter's contract.

## Validation status

Development probes on macOS LibreOffice 25.8.4.2 have exercised real Writer,
Calc, and Impress custom UNO factory creation, ODF save/close/reopen, and native
data/preview/extent restoration. Those probes were headless. Packaged extension
installation, external editor save-back, clipboard exchange, desktop interaction,
and Windows/Linux interchange are separate acceptance checks; a successful
headless factory probe alone is not a support certification.

The immutable object class is `8E86A932-EBBE-4E9F-8D26-CA2D82096856`. Its ODF
substorage media type is `application/vnd.reshiki.embedded-drawing`; entries are
`drawing.rsk`, `preview.png`, and versioned `metadata.json`. No executable paths,
macros, or external file links are stored in that object.
