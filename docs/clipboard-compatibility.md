# Clipboard compatibility

This page lists what other apps receive when you paste a ReShiki copy, and the
exact clipboard formats behind it. ReShiki puts several formats on the clipboard
at once, and the receiving app chooses one of them. [Clipboard](clipboard.md)
explains the commands themselves.

- **Copy** (**Cmd/Ctrl+C**) carries the editable drawing plus pictures.
- **Copy Image** (**Cmd/Ctrl+Shift+C**, or **Export → Copy image**) carries
  pictures only.

Each result is marked:

- **Tested**: checked on 2026-11-12 with ReShiki 0.14.0 (main at `11df00d5`).
- **Docs/code**: taken from ReShiki's code or the other app's documented
  behavior, not tested.
- **Unknown**: not tested, and the behavior is not documented well enough to say.

The test machines were macOS 27.0.1 arm64, Windows 11 x64 25H2 and Arch Linux
x64. Dumps, scripts and screenshots are in the
[evidence folder](https://github.com/Ameyanagi/ReShiki-evidence/tree/35d36522504604bc275cfc51154da61f09b2a1b7/issues/1146).

## Quick advice

- **Office on Windows (Word, PowerPoint, Excel):** use **Copy** for an object you
  can double-click to edit in ReShiki. Use **Copy Image** for a static figure.
- **ChemDraw:** use **Copy** for editable chemistry.
- **Web and WebView apps on Windows:** this covers new Outlook, Outlook on the
  web, Gmail, Google Docs, Teams, Slack and Notion. Both **Copy** and **Copy
  Image** paste a bitmap picture. Before
  [#1148](https://github.com/Ameyanagi/ReShiki/pull/1148), **Copy** pasted
  nothing into them ([#1146](https://github.com/Ameyanagi/ReShiki/issues/1146)).
- **Mac apps:** **Copy** and **Copy Image** both paste a picture. Only ChemDraw
  and ReShiki read the editable drawing. Mac Office has no editable ReShiki
  object, so keep the `.rsk` original.
- **Web apps:** the picture has 1200 dpi resolution metadata, which browsers
  ignore, so it pastes large. A 3 cm drawing appears about 35 cm wide. Resize it
  after pasting.

## What you get when you paste

"Editable" means the atoms and bonds survive and can be edited, or copied back
into ReShiki. "Object" means the Windows editable ReShiki object: Office shows
its picture, and a double-click opens the drawing in ReShiki. "Vector" and
"bitmap" are pictures without editable chemistry.

### Windows

| App                                                             | Copy                                                        | Copy Image               | Status                                                                                                  |
| --------------------------------------------------------------- | ----------------------------------------------------------- | ------------------------ | ------------------------------------------------------------------------------------------------------- |
| Word, PowerPoint, Excel (Microsoft 365)                         | Object; double-click edits in ReShiki                       | Vector (SVG graphic)     | Tested                                                                                                  |
| Classic Outlook (Win32, Word editor)                            | Probably an object, like Word; the recipient gets a picture | Vector or bitmap picture | Unknown: no mail profile on the test machine                                                            |
| New Outlook for Windows (WebView2)                              | Bitmap (since #1148)                                        | Bitmap (PNG)             | Tested in Edge 157, the same engine as the test machine's WebView2 157; the app itself needs an account |
| Outlook on the web, Gmail, Google Docs/Slides in Edge or Chrome | Bitmap (since #1148)                                        | Bitmap (PNG)             | Tested in Edge; the sites need accounts                                                                 |
| Teams, Slack, Notion desktop apps (Chromium-based)              | Bitmap (since #1148)                                        | Bitmap (PNG)             | Docs/code: same clipboard reader as Edge                                                                |
| OneNote desktop                                                 | Probably an object or picture                               | Picture                  | Unknown                                                                                                 |
| Paint                                                           | Nothing or metafile                                         | Bitmap                   | Unknown; Paint reads bitmaps                                                                            |
| LibreOffice                                                     | Unknown                                                     | Picture                  | Unknown: not installed on the test machine                                                              |
| ChemDraw                                                        | Editable (CDX)                                              | Picture                  | Docs/code                                                                                               |
| ReShiki                                                         | Editable                                                    | Picture                  | Docs/code; covered by Windows integration tests                                                         |

Tip: if Word pastes the object but you want a plain picture, use **Paste
Special → Picture (Enhanced Metafile)**, or **Copy Image** in ReShiki.

### macOS

| App                                             | Copy                      | Copy Image                | Status                                                                       |
| ----------------------------------------------- | ------------------------- | ------------------------- | ---------------------------------------------------------------------------- |
| Word, PowerPoint, Excel 16.115                  | Picture                   | Picture                   | Tested; the pasted size (2.74 × 2.92 cm) matches the PDF, so probably vector |
| TextEdit                                        | Picture attachment        | Picture attachment        | Tested                                                                       |
| Preview (**File → New from Clipboard**)         | Vector PDF                | Vector PDF                | Tested                                                                       |
| Notes, Google account folder                    | **Nothing**               | Not tested                | Tested; such folders hold text only. iCloud notes: unknown                   |
| Mail                                            | Picture                   | Picture                   | Docs/code; Mail wanted a setup choice first                                  |
| Outlook for Mac, OneNote                        | Unknown                   | Unknown                   | Unknown: both need an account                                                |
| Pages, Keynote, Numbers                         | Picture                   | Picture                   | Docs/code; not installed on the test machine                                 |
| Chrome (Gmail, Google Docs, Outlook on the web) | Bitmap (PNG)              | Bitmap (PNG)              | Tested with a Chrome 156 paste event; the sites need accounts                |
| Safari                                          | Bitmap (PNG)              | Bitmap (PNG)              | Docs/code                                                                    |
| ChemDraw 26.1                                   | **Editable**              | Picture                   | Tested                                                                       |
| ReShiki                                         | **Editable**              | Picture                   | Tested                                                                       |
| Illustrator                                     | Vector paths from the PDF | Vector paths from the PDF | Docs/code                                                                    |

### Linux

| App                                                             | Copy                      | Copy Image                | Status                                        |
| --------------------------------------------------------------- | ------------------------- | ------------------------- | --------------------------------------------- |
| LibreOffice 25.8 Writer and Impress                             | Vector (PDF graphic)      | Vector (PDF graphic)      | Tested on X11                                 |
| Chromium and Chrome-based apps (web apps, Slack, Teams, Notion) | Bitmap (PNG)              | Bitmap (PNG)              | Tested with a Chromium 149 paste event on X11 |
| Inkscape                                                        | Vector paths from the SVG | Vector paths from the SVG | Docs/code                                     |
| GIMP                                                            | Bitmap (PNG)              | Bitmap (PNG)              | Docs/code                                     |
| ReShiki                                                         | Editable                  | Picture                   | Docs/code                                     |

The Linux tests published the Copy and Copy Image bytes through ReShiki's own X11
clipboard worker under Xvfb. Wayland offers the same MIME types; it is
docs/code only.

### The browser editor

| Paste into              | Copy                                                   | Copy Image | Status    |
| ----------------------- | ------------------------------------------------------ | ---------- | --------- |
| Word, Google Docs, mail | Bitmap (PNG, from the HTML or PNG flavor)              | Bitmap     | Docs/code |
| Another browser ReShiki | Editable (the drawing travels inside the HTML picture) | Picture    | Docs/code |
| Desktop ReShiki         | Editable molecule from the SMILES text, re-laid out    | Picture    | Docs/code |
| Text fields             | SMILES                                                 | Nothing    | Docs/code |

## Developer reference

The portable names below are what `src/clipboard.rs` prepares
(`prepare_copy_for` and `copy_images`). Each platform maps them to native names
in `native/macos/src/clipboard.rs`, `native/windows/src/clipboard.rs` and
`native/windows/src/ole.rs`, `native/linux/src/protocol.rs`, and
`src/clipboard/browser.rs`. The Copy PNG is 1200 dpi with transparency and
`pHYs` metadata. On Windows it is limited to 16 megapixels.

### macOS: NSPasteboard types

There is one pasteboard item. Types are in item order, from a dump after each
command:

| #   | Copy                                                  | Copy Image                                                                   |
| --- | ----------------------------------------------------- | ---------------------------------------------------------------------------- |
| 1   | `dev.reshiki.drawing` (native JSON)                   | `com.adobe.pdf`                                                              |
| 2   | `com.revvity.chemdraw.cdx-clipboard` (CDX)            | `com.revvity.chemdraw.cdx-clipboard` (CDX holding only the PNG and its size) |
| 3   | `com.perkinelmer.chemdraw.cdx-clipboard` (same bytes) | `com.perkinelmer.chemdraw.cdx-clipboard`                                     |
| 4   | `com.cambridgesoft.cdx` (same bytes)                  | `com.cambridgesoft.cdx`                                                      |
| 5   | `com.adobe.pdf`                                       | `public.png`                                                                 |
| 6   | `public.png`                                          | `public.svg-image`                                                           |
| 7   | `public.svg-image`                                    | `dev.reshiki.drawing` (a drawing holding the picture)                        |

Copy writes no `public.tiff`, `public.html`, `public.rtf` or text type. When
editable CDX export fails, Copy writes the picture-only CDX instead, just before
`public.png`.

Observed picks: ChemDraw reads the CDX and ReShiki reads `dev.reshiki.drawing`.
Preview uses `com.adobe.pdf`. Chrome exposes `public.png` as an `image/png` file
and `public.svg-image` as `image/svg+xml`. Office and TextEdit paste a picture,
but their choice between PDF and PNG was not determined.

### Windows: clipboard formats

Copy goes through OLE: `OleSetClipboard`, then `OleFlushClipboard`. This is the
`EnumClipboardFormats` order after a Copy:

| #   | Copy                          | Notes                                                             |
| --- | ----------------------------- | ----------------------------------------------------------------- |
| 0   | `DataObject`                  | Added by OLE                                                      |
| 1   | `Embed Source`                | Compound file: the native drawing plus the cached PNG/EMF preview |
| 2   | `Native`                      | Added by OLE                                                      |
| 3   | `OwnerLink`                   | Added by OLE; class `ReShiki.EmbeddedDrawing.1`                   |
| 4   | `Object Descriptor`           |                                                                   |
| 5   | `CF_ENHMETAFILE`              | EMF+ dual vector preview                                          |
| 6   | `PDF`                         |                                                                   |
| 7   | `ChemDraw Interchange Format` | CDX                                                               |
| 8   | `dev.reshiki.drawing`         | Native JSON                                                       |
| 9   | `Ole Private Data`            | Added by OLE                                                      |
| 10  | `CF_METAFILEPICT`             | Synthesized by Windows                                            |

`ole::copy` removes `PNG` and `image/svg+xml` from Copy but keeps `CF_DIB`
([#1148](https://github.com/Ameyanagi/ReShiki/pull/1148)), so browser and WebView2
apps receive a bitmap while Office still takes `Embed Source`. Copy has no
`HTML Format`, `CF_UNICODETEXT` or `PNG`.

Copy Image uses plain `SetClipboardData`, in format-ID order: `CF_DIB` (24-bit, on
white), `PDF`, `PNG`, `image/svg+xml` (text outlined), `ChemDraw Interchange
Format` (picture only) and `dev.reshiki.drawing` (a picture drawing). Windows
synthesizes `CF_BITMAP` and `CF_DIBV5`.

Observed picks:

- **Word, PowerPoint and Excel** take `Embed Source` from Copy, and
  `image/svg+xml` from Copy Image.
- **Chromium and WebView2** (Edge, new Outlook, Teams and other web apps) read
  only `HTML Format`, `PNG`, `CF_DIB`/`CF_DIBV5`/`CF_BITMAP`, `CF_UNICODETEXT`,
  `image/svg+xml` and file lists. They take `CF_DIB` from Copy (before #1148 a
  Copy gave them an empty paste event).

Experiment from [#1146](https://github.com/Ameyanagi/ReShiki/issues/1146): the Copy
formats were re-published with one extra format.

| Extra format                      | Word    | PowerPoint | Excel   | Edge    |
| --------------------------------- | ------- | ---------- | ------- | ------- |
| None                              | Object  | Object     | Object  | Nothing |
| `HTML Format` with a PNG data URI | Picture | Picture    | Picture | Picture |
| `PNG`                             | Picture | Object     | Picture | Picture |
| `CF_DIB`                          | Object  | Object     | Object  | Picture |

### Linux: X11 and Wayland MIME types

Copy and Copy Image offer the same MIME types in sorted order. Only the bytes
differ: Copy Image's CDX and native drawing hold the picture.

```text
application/pdf
application/x-chemdraw
application/x-reshiki-drawing+json
chemical/x-cdx
com.cambridgesoft.cdx
com.perkinelmer.cdx
com.perkinelmer.chemdraw.cdx-clipboard
com.revvity.cdx
com.revvity.chemdraw.cdx-clipboard
dev.reshiki.drawing
image/png
image/svg+xml
```

X11 adds `TARGETS`, `TIMESTAMP` and `MULTIPLE`. Wayland adds a private random
receipt type. There is no `text/plain`, `UTF8_STRING` or `text/html`.

Observed picks: LibreOffice lists `application/pdf`, `application/x-chemdraw`,
`application/x-reshiki-drawing+json`, `chemical/x-cdx`, `image/png` and
`image/svg+xml`, and pastes the PDF. Chromium exposes `image/png` and
`image/svg+xml`.

### Browser: Clipboard API types

The browser editor writes one `ClipboardItem`:

- **Copy:** `text/html`, `image/png`, `image/svg+xml` (only where
  `ClipboardItem.supports` allows it, as in Chromium) and `text/plain` with
  SMILES when the selection has molecules. The HTML is
  `<img src="data:image/png…" data-reshiki="…">`, sized in CSS pixels from the
  PNG resolution. `data-reshiki` holds the base64 native drawing.
- **Copy Image:** the same without `text/plain`. Its `data-reshiki` holds a
  picture drawing.

### What ReShiki reads on Paste

Paste tries these portable formats in order and stops at the first one that
imports (`src/clipboard/priority.rs`):

1. `dev.reshiki.drawing` and the legacy `dev.moruno.drawing`.
2. CDX: `com.revvity.chemdraw.cdx-clipboard`,
   `com.perkinelmer.chemdraw.cdx-clipboard`, `com.cambridgesoft.cdx`,
   `com.revvity.cdx`, `com.perkinelmer.cdx`, `chemical/x-cdx` and
   `application/x-chemdraw`.
3. CDXML: `com.revvity.cdxml`, `com.perkinelmer.cdxml`,
   `com.cambridgesoft.cdxml`, `com.perkinelmer.chemdraw.cdxml`,
   `chemical/x-cdxml` and `public.cdxml`.
4. `com.mdli.molfile`, then `org.opensmiles.smiles`.
5. Plain text.
6. Pictures: `public.png`, `public.tiff`, `public.jpeg`, then WebP.
7. `com.adobe.pdf` and `public.svg-image`. These are only recognized, so that
   Paste can explain why a PDF or SVG cannot be pasted.

Native names for these:

- **Windows:** `ChemDraw Interchange Format`, `ChemDraw XML`, `MDLCT`,
  `chemical/x-mdl-molfile`, `SMILES`, `PNG`/`image/png`, `CF_DIBV5`/`CF_DIB`,
  `JFIF`/`JPEG`, `TIFF` and `CF_UNICODETEXT`. Windows first reads `Embed Source`
  or `Embedded Object` storage. A ReShiki object gives back its drawing; a
  foreign object can give ChemDraw CDX from its `CONTENTS` stream.
- **Linux:** `application/x-reshiki-drawing+json`, `chemical/x-mdl-molfile`,
  `chemical/x-daylight-smiles`, `text/plain;charset=utf-8`, `text/plain`,
  `UTF8_STRING` and `image/*`.
- **macOS:** the UTIs above, read from the first pasteboard item only.
- **Browser:** `data-reshiki` in `text/html`, then `text/plain`, `image/png` and
  `image/svg+xml`.

## Open questions

- The web picture size: an `HTML Format` or `text/html` flavor with CSS
  `width`/`height` would fix it, but on Windows it makes Office paste a picture
  instead of the object.
- Classic Outlook, Outlook for Mac, OneNote, Pages/Keynote and iCloud Notes need
  testing on machines with accounts.
