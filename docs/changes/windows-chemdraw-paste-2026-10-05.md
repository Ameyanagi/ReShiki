# Embedded ChemDraw paste from Windows Office

Under review · [PR #150](https://github.com/Ameyanagi/ReShiki/pull/150) · @Ameyanagi.

Selecting a whole embedded ChemDraw object in PowerPoint and pressing Ctrl+C
can offer its OLE storage alongside a presentation image, without a standalone
CDX clipboard format. Previously, ReShiki recognized only its own OLE class and
could therefore insert the presentation as a picture. Normal Paste now reads
supported ChemDraw CDX from the foreign object's root `CONTENTS` stream and
uses the existing chemical importer.

## Scope

- Existing native ReShiki and explicit chemical formats retain priority.
- Each offered OLE storage format is retrieved at most once per paste. Foreign
  storage stays on the same COM apartment while priority is decided, avoiding
  a second delayed-rendering request before chemical import or image fallback.
- Both `Embed Source` and `Embedded Object` storage media are supported.
- Only the exact root stream with the complete 12-byte CDX signature is read.
  Nested objects, unrelated streams and binary substrings are not searched.
- An unrecognized or inaccessible stream leaves ordinary image fallback intact.
  Once CDX is recognized, oversized, corrupt or unsupported chemistry reports
  an error rather than silently becoming a picture. The payload limit is 16 MB.
- Import → Paste picture still chooses the preview. This does not recover
  chemical data from an object that contains only an image.
- No dependency, Python runtime, ChemDraw activation or extra executable is added.

The storage convention is documented by ChemScanner's original
[DOCX extractor](https://github.com/ComPlat/chem_scanner/blob/master/lib/chem_scanner/docx.rb).
Microsoft documents the OLE formats and storage handling in
[OleCreateFromData](https://learn.microsoft.com/en-us/windows/win32/api/ole2/nf-ole2-olecreatefromdata)
and [OleFlushClipboard](https://learn.microsoft.com/en-us/windows/win32/api/ole2/nf-ole2-oleflushclipboard).
The implementation is original Rust; no third-party implementation is copied.

## Interchange evidence

Input: the existing genuine ChemDraw capture
[`native-ethyl-clipboard.cdx`](../../tests/fixtures/native-ethyl-clipboard.cdx).
The application-level regression compares a valid PNG-only packet with the
same preview accompanied by that CDX through the normal paste adapter.

| Paste input                                               |         Atoms | Bonds |          Picture objects |
| --------------------------------------------------------- | ------------: | ----: | -----------------------: |
| PNG preview only                                          |             0 |     0 |                        1 |
| Embedded CDX returned by the native reader                | 11 (9 C, 2 O) |    11 |                        0 |
| Recognized CDX truncated to 32 bytes plus a valid preview |  Import error |     — | No silent image fallback |

The native Windows tests create a foreign COM storage object containing the
same CDX, publish and flush it to the actual Windows clipboard, then invoke
the production read protocol. They check both OLE formats, exact CDX bytes,
chemical/native priority, explicit picture paste, PNG/DIB fallback, unrelated
and nested streams, partial or misplaced signatures, and recognized oversize
data. A separate storage test covers a root stream held open exclusively.
Clipboard-mutating tests share a lock to avoid parallel-test interference.
Counted COM data-object tests call the production reader and assert one
retrieval per format, including a foreign first format followed by native
ReShiki data. They also check that an unclassified acquisition failure does
not hide valid later native, standalone or embedded chemical data. If no
usable representation wins, the existing acquisition diagnostic is retained.

These are synthetic Office storage tests, not a captured PowerPoint clipboard
object. Local compilation checks target Windows x64 and ARM64; the existing
Windows x64 CI workspace tests execute the native COM tests. The macOS host
cannot establish live PowerPoint compatibility. No screenshot is presented as
Windows acceptance evidence: the meaningful change is editable chemical data,
which an identical-looking image would not demonstrate.

## Desktop review checklist

Use a Windows build of this branch and a real embedded ChemDraw object:

1. Select the whole object on the PowerPoint slide and press Ctrl+C.
2. Focus ReShiki and press Ctrl+V. Select an atom or bond and confirm it can be
   edited; the result must not be one picture object.
3. Save as `.rsk`, reopen it, and confirm the atoms and bonds remain editable.
   Undo should remove the paste as one edit; Redo should restore it.
4. Copy the same object again and choose Import → Paste picture. Confirm that
   this inserts its preview as a picture.
5. Copy an ordinary slide image and an embedded ReShiki object separately.
   Confirm that each retains its existing picture/native behavior.

Live Office acceptance remains to be checked on Windows. An object containing
only a rendered picture, an unsupported storage layout, or unsupported CDX
features remains outside the tested editable import path.

Release caption: **Copy a supported embedded ChemDraw object from PowerPoint
into ReShiki as editable atoms and bonds on Windows.**
