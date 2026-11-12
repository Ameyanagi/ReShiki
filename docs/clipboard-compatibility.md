# Clipboard compatibility

This page lists what other apps receive when you paste a ReShiki copy, and the
exact clipboard formats behind it. ReShiki puts several formats on the clipboard
at once, and the receiving app chooses one of them. [Clipboard](clipboard.md)
explains the commands themselves.

- **Copy** (**Cmd/Ctrl+C**) carries the editable drawing plus pictures.
- **Copy Image** (**Cmd/Ctrl+Shift+C**, or **Export → Copy image**) carries
  pictures only.
- Both also hide the ReShiki drawing inside their PNG, SVG and PDF pictures.
  [Does the editable drawing survive?](#does-the-editable-drawing-survive)
  shows which apps hand it back.

Each result is marked:

- **Tested**: checked on 2026-11-12 with ReShiki 0.14.0 (main at `11df00d5`,
  or `fb508e23` for the plain-text rows, Paint and the browser editor).
- **Docs/code**: taken from ReShiki's code or the other app's documented
  behavior, not tested.
- **Unknown**: not tested, and the behavior is not documented well enough to say.

The test machines were macOS 27.0.1 arm64, Windows 11 x64 25H2 and Arch Linux
x64. Dumps, scripts and screenshots are in the [evidence
folder](https://github.com/Ameyanagi/ReShiki-evidence/tree/35d36522504604bc275cfc51154da61f09b2a1b7/issues/1146);
the plain-text rows, Paint and the browser editor results are in
[another](https://github.com/Ameyanagi/ReShiki-evidence/tree/5998c1d1eddc0640a912a1822e61195684aa8c33/pr-reviews/pr1157).

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
  object, but it keeps ReShiki's PDF: copy the picture in Word, PowerPoint or
  Excel and paste it into ReShiki to get the editable drawing back.
- **Text-only places** (plain-text boxes, TextEdit in plain-text mode, Notes in a
  Google account): in the desktop app, **Copy** and **Copy Image** put no text on
  the clipboard, so use **Copy SMILES** (**Cmd/Ctrl+Alt+C**). The browser
  editor's **Copy** does include SMILES. [Why Copy has no text](#why-copy-has-no-text)
  explains why.
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
| Classic Outlook (Win32, Word editor)                            | Probably an object, like Word; the recipient gets a picture | Vector or bitmap picture | Unknown: installed on the test machine, but it has no mail profile                                      |
| New Outlook for Windows (WebView2)                              | Bitmap (since #1148)                                        | Bitmap (PNG)             | Tested in Edge 157, the same engine as the test machine's WebView2 157; the app itself needs an account |
| Outlook on the web, Gmail, Google Docs/Slides in Edge or Chrome | Bitmap (since #1148)                                        | Bitmap (PNG)             | Tested in Edge; the sites need accounts                                                                 |
| Teams, Slack, Notion desktop apps (Chromium-based)              | Bitmap (since #1148)                                        | Bitmap (PNG)             | Docs/code: same clipboard reader as Edge                                                                |
| OneNote desktop                                                 | Probably an object or picture                               | Picture                  | Unknown: OneNote 16 asks to sign in when it starts                                                      |
| Paint                                                           | Bitmap (since #1148)                                        | Bitmap                   | Tested with Paint 11.2609                                                                               |
| Plain-text boxes                                                | No text: use **Copy SMILES** (**Ctrl+Alt+C**)               | No text                  | Tested in an Edge 157 text box                                                                          |
| LibreOffice                                                     | Unknown                                                     | Picture                  | Unknown: not installed on the test machine                                                              |
| ChemDraw                                                        | Editable (CDX)                                              | Picture                  | Docs/code                                                                                               |
| ReShiki                                                         | Editable                                                    | Editable                 | Tested                                                                                                  |

Tip: if Word pastes the object but you want a plain picture, use **Paste
Special → Picture (Enhanced Metafile)**, or **Copy Image** in ReShiki.

### macOS

| App                                             | Copy                                            | Copy Image                | Status                                                                                                                            |
| ----------------------------------------------- | ----------------------------------------------- | ------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| Word, PowerPoint, Excel 16.115                  | Picture                                         | Picture                   | Tested; the pasted size (2.74 × 2.92 cm) matches the PDF, so probably vector                                                      |
| TextEdit                                        | Picture attachment                              | Picture attachment        | Tested                                                                                                                            |
| TextEdit in plain-text mode                     | No text: use **Copy SMILES** (**Cmd+Option+C**) | No text                   | Tested for Copy; Copy Image: docs/code                                                                                            |
| Preview (**File → New from Clipboard**)         | Vector PDF                                      | Vector PDF                | Tested                                                                                                                            |
| Notes, Google account folder                    | No text: use **Copy SMILES** (**Cmd+Option+C**) | No text                   | Tested; such folders hold text only, and ignored a SMILES text flavor next to the pictures. No iCloud account on the test machine |
| Mail                                            | Picture                                         | Picture                   | Docs/code; Mail asked for a privacy setup choice before it would open a message                                                   |
| Outlook for Mac, OneNote                        | Unknown                                         | Unknown                   | Unknown: both need an account                                                                                                     |
| Pages, Keynote, Numbers                         | Picture                                         | Picture                   | Docs/code; not installed on the test machine                                                                                      |
| Chrome (Gmail, Google Docs, Outlook on the web) | Bitmap (PNG)                                    | Bitmap (PNG)              | Tested with a Chrome 156 paste event; the sites need accounts                                                                     |
| Safari                                          | Bitmap (PNG)                                    | Bitmap (PNG)              | Docs/code                                                                                                                         |
| ChemDraw 26.1                                   | **Editable**                                    | Picture                   | Tested                                                                                                                            |
| ReShiki                                         | **Editable**                                    | **Editable**              | Tested                                                                                                                            |
| Illustrator                                     | Vector paths from the PDF                       | Vector paths from the PDF | Docs/code                                                                                                                         |

### Linux

| App                                                             | Copy                                          | Copy Image                | Status                                        |
| --------------------------------------------------------------- | --------------------------------------------- | ------------------------- | --------------------------------------------- |
| LibreOffice 25.8 Writer and Impress                             | Vector (PDF graphic)                          | Vector (PDF graphic)      | Tested on X11                                 |
| Chromium and Chrome-based apps (web apps, Slack, Teams, Notion) | Bitmap (PNG)                                  | Bitmap (PNG)              | Tested with a Chromium 149 paste event on X11 |
| Inkscape                                                        | Vector paths from the SVG                     | Vector paths from the SVG | Docs/code                                     |
| GIMP                                                            | Bitmap (PNG)                                  | Bitmap (PNG)              | Docs/code                                     |
| Plain-text boxes                                                | No text: use **Copy SMILES** (**Ctrl+Alt+C**) | No text                   | Tested in a Chromium 149 text box on X11      |
| ReShiki                                                         | Editable                                      | Editable                  | Docs/code                                     |

The Linux tests published the Copy and Copy Image bytes through ReShiki's own X11
clipboard worker under Xvfb. Wayland offers the same MIME types; it is
docs/code only.

### The browser editor

| Paste into                             | Copy                                                   | Copy Image                                                | Status                          |
| -------------------------------------- | ------------------------------------------------------ | --------------------------------------------------------- | ------------------------------- |
| Word, Excel (Microsoft 365 on Windows) | Bitmap                                                 | Bitmap                                                    | Tested with Edge 157            |
| Google Docs, mail                      | Bitmap (PNG, from the HTML or PNG flavor)              | Bitmap                                                    | Docs/code                       |
| Another browser ReShiki                | Editable (the drawing travels inside the HTML picture) | Editable                                                  | Docs/code                       |
| Desktop ReShiki                        | Editable molecule from the SMILES text, re-laid out    | Picture                                                   | Tested on Windows with Edge 157 |
| Text fields                            | SMILES                                                 | No text: **Copy Image** is pictures only, so use **Copy** | Tested in an Edge 157 text box  |

## Does the editable drawing survive?

ReShiki puts the drawing inside its PNG, SVG and PDF pictures (see
[Keep the drawing editable inside a picture](figure-export.md#keep-the-drawing-editable-inside-a-picture)).
This table shows whether it comes back when you paste a ReShiki **Copy Image**
into the app, copy the picture there, and paste it into ReShiki, and whether it
survives in files the app saves. "Yes" means ReShiki pasted the editable atoms
and bonds again. Tested on 2026-11-12 with builds of
[#1153](https://github.com/Ameyanagi/ReShiki/pull/1153) and
[#1154](https://github.com/Ameyanagi/ReShiki/pull/1154) on macOS 27 arm64 and
Windows 11 x64; dumps and scripts are in the
[evidence folder](https://github.com/Ameyanagi/ReShiki-evidence/tree/a0ef99368019c802456a33d3d977bb2956667d50/pr-reviews/pr1154).

| Platform | App                                                                                        | Copy back into ReShiki                                 | Saved files                                                                                                 | Status                                                                       |
| -------- | ------------------------------------------------------------------------------------------ | ------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| Windows  | Word, Excel (Microsoft 365)                                                                | **Yes**: Office hands back ReShiki's SVG unchanged     | **No**: `.docx`/`.xlsx` rewrite the SVG                                                                     | Tested                                                                       |
| Windows  | PowerPoint (Microsoft 365)                                                                 | Probably yes, like Word and Excel                      | Probably no                                                                                                 | Not tested: the test machine's PowerPoint was in use                         |
| Windows  | Word with **Copy** (the ReShiki object)                                                    | **Yes**, through the object itself                     | The object keeps the drawing; its saved preview picture does not                                            | Tested                                                                       |
| Windows  | Edge 157 and WebView2 apps (new Outlook)                                                   | **No**: Edge keeps the PNG but copies back only HTML   | n/a                                                                                                         | Tested in Edge 157; ReShiki does not read a picture from HTML                |
| macOS    | Word, PowerPoint, Excel 16.115                                                             | **Yes**: Office hands back ReShiki's PDF byte for byte | `.docx`/`.pptx` keep it inside their EMF, which ReShiki cannot read yet; **Save as Picture** (PNG) does not | Tested                                                                       |
| macOS    | TextEdit                                                                                   | **No**: it copies back rich text                       | Not tested                                                                                                  | Tested                                                                       |
| macOS    | Chrome 157                                                                                 | **No**: Chrome keeps the PNG but copies back only HTML | n/a                                                                                                         | Tested                                                                       |
| macOS    | Preview, Keynote, Pages                                                                    | Unknown                                                | Unknown                                                                                                     | Not tested: Preview could not be driven; Keynote and Pages are not installed |
| Any      | Apps that re-encode or compress pictures (web mail, chat, **Compress Pictures** in Office) | **No**                                                 | **No**                                                                                                      | Docs/code                                                                    |
| Any      | ReShiki itself (**Copy Image** → **Paste**)                                                | **Yes**                                                | Exported PNG, SVG and PDF with **Include editable drawing**: **Yes**                                        | Tested                                                                       |

Measured payload sizes: 628 bytes for caffeine, 902 bytes for an
esterification scheme and 4.5 KB for five atorvastatin molecules (205 atoms).
Each picture grows by about 4/3 of that. A drawing over 2 MB compressed, such
as one with a large photo, is not put in pictures.

## Developer reference

The portable names below are what `src/clipboard.rs` prepares
(`prepare_copy_for` and `copy_images`). Each platform maps them to native names
in `native/macos/src/clipboard.rs`, `native/windows/src/clipboard.rs` and
`native/windows/src/ole.rs`, `native/linux/src/protocol.rs`, and
`src/clipboard/browser.rs`. The Copy PNG is 1200 dpi with transparency and
`pHYs` metadata. On Windows it is limited to 16 megapixels. Every clipboard SVG
(Copy, Copy Image and Copy as → SVG, on every platform and in the browser) is
the outlined `svg-office` figure: text drawn as paths, width and height in
points with a matching `viewBox`, a transparent background and the drawing
inside it.

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
| 6   | `public.png`                                          | `public.svg-image` (text outlined)                                           |
| 7   | `public.svg-image` (text outlined)                    | `dev.reshiki.drawing` (the drawing, when it fits the 2 MB limit)             |

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
Format` (picture only) and `dev.reshiki.drawing` (the drawing, when it fits the
2 MB limit). Windows synthesizes `CF_BITMAP` and `CF_DIBV5`. The `PDF`, `PNG` and
`image/svg+xml` pictures carry the drawing; `CF_DIB` cannot.

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
differ: Copy Image's CDX holds the picture.

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

`image/svg+xml` draws its text as outlines. X11 adds `TARGETS`, `TIMESTAMP` and `MULTIPLE`. Wayland adds a private random
receipt type. There is no `text/plain`, `UTF8_STRING` or `text/html`.

Observed picks: LibreOffice lists `application/pdf`, `application/x-chemdraw`,
`application/x-reshiki-drawing+json`, `chemical/x-cdx`, `image/png` and
`image/svg+xml`, and pastes the PDF. Chromium exposes `image/png` and
`image/svg+xml`.

### Browser: Clipboard API types

The browser editor writes one `ClipboardItem`:

- **Copy:** `text/html`, `image/png`, `image/svg+xml` (text outlined, only where
  `ClipboardItem.supports` allows it, as in Chromium) and `text/plain` with
  SMILES when the selection has molecules. The HTML is
  `<img src="data:image/png…" data-reshiki="…">`, sized in CSS pixels from the
  PNG resolution. `data-reshiki` holds the base64 native drawing.
- **Copy Image:** the same without `text/plain`. Its `data-reshiki` holds the
  drawing too.

### Why Copy has no text

Copy and Copy Image carry no plain-text flavor on the desktop, and the browser's
Copy Image has none either. Text-only places get nothing from them; **Copy
SMILES** (**Cmd/Ctrl+Alt+C**) is the command for those. Many apps prefer text
over pictures, so a SMILES text flavor would replace the picture or object in
them. ReShiki removed one from Copy in 0.12 after PowerPoint for Mac pasted a
SMILES text box.

On 2026-11-12 the Copy and Copy Image formats were re-published with SMILES
added, to check whether some placement avoids that. On macOS the text was added
to Copy, either in the same pasteboard item as the pictures or into a second
item. On Windows it was `CF_UNICODETEXT`. On Linux it was
`text/plain;charset=utf-8`, `text/plain` and `UTF8_STRING`, through ReShiki's
X11 worker. For the browser, Edge wrote Copy Image's `ClipboardItem` with and
without `text/plain`. Dumps, scripts and screenshots are in the [evidence
folder](https://github.com/Ameyanagi/ReShiki-evidence/tree/5998c1d1eddc0640a912a1822e61195684aa8c33/pr-reviews/pr1157).

| App                                              | Without text | With text, same item / second item        |
| ------------------------------------------------ | ------------ | ----------------------------------------- |
| macOS Word 16.115                                | Picture      | Picture / picture                         |
| macOS PowerPoint 16.115                          | Picture      | **Text box** / **nothing**                |
| macOS Excel 16.115                               | Picture      | **Text in a cell** / **text in a cell**   |
| macOS TextEdit, rich text                        | Picture      | Picture / picture                         |
| macOS TextEdit, plain text                       | Nothing      | SMILES / SMILES                           |
| macOS Notes, Google folder                       | Nothing      | Nothing / nothing                         |
| macOS Chrome 157 page                            | Picture      | Picture (same item)                       |
| macOS ChemDraw 26.1, ReShiki                     | Editable     | Editable / editable                       |
| Windows Word (Copy)                              | Object       | Object                                    |
| Windows Excel (Copy)                             | Object       | **Text in a cell**                        |
| Windows Word (Copy Image)                        | SVG graphic  | SVG graphic                               |
| Windows Excel (Copy Image)                       | SVG graphic  | **Text in a cell**                        |
| Windows Edge 157 page                            | Bitmap       | Bitmap                                    |
| Windows Edge 157 text box                        | Nothing      | SMILES                                    |
| Linux LibreOffice 25.8 Writer, Impress           | PDF picture  | PDF picture                               |
| Linux Chromium 149 page                          | Picture      | Picture                                   |
| Linux Chromium 149 text box                      | Nothing      | SMILES                                    |
| Browser Copy Image → Word, Excel, Edge (Windows) | Picture      | Picture                                   |
| Browser Copy Image → desktop ReShiki (Windows)   | Picture      | **Molecule from the SMILES, re-laid out** |

So a text flavor would break Excel on both systems and PowerPoint on macOS, and
it would not even reach Notes. On Linux, LibreOffice and a Chromium page kept
the picture, and only text boxes changed, but the web apps that read a paste
themselves could not be tested there, and Copy stays the same on every platform.
Browser Copy Image would hand desktop ReShiki a re-laid-out molecule instead of
the picture that was asked for.

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
5. `com.adobe.pdf`, then `public.svg-image`, read only for a ReShiki drawing
   inside them. Office hands back ReShiki's own PDF or SVG next to a PNG it drew
   itself, so these come before pictures. Without a drawing, Paste moves on.
6. Plain text.
7. Pictures: `public.png` (its drawing, if it has one, else the picture),
   `public.tiff`, `public.jpeg`, then WebP.

Native names for these:

- **Windows:** `ChemDraw Interchange Format`, `ChemDraw XML`, `MDLCT`,
  `chemical/x-mdl-molfile`, `SMILES`, `PDF`/`Portable Document Format`/`application/pdf`,
  `image/svg+xml`, `PNG`/`image/png`, `CF_DIBV5`/`CF_DIB`, `JFIF`/`JPEG`, `TIFF`
  and `CF_UNICODETEXT`. Windows first reads `Embed Source`
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
  testing on machines with accounts. Classic Outlook and OneNote are installed
  on the Windows test machine, but Outlook has no mail profile and OneNote asks
  to sign in.
- Linux could offer a text flavor: LibreOffice and a Chromium page kept the
  picture with it. Web apps such as Slack or Google Docs, which read a paste
  themselves, may prefer text over a picture file and need accounts to test.
