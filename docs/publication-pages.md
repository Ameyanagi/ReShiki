# Publication pages

Open **View → Page setup…** or **Export → Page setup…**. Choose A4, A5,
US Letter, US Legal or a custom size. Width, height and the four margins use
millimetres. Portrait/landscape and a 1–10 by 1–10 page grid are available.
Apply creates one Undo step; editing the fields alone does not change the drawing.

The canvas shows white sheets, margin guides and page numbers on a gray background.
Atom selection markers shrink at page-fit zoom to keep small structures readable.
Page order runs left to right, then down. The page panel offers Previous/Next,
Fit page and Fit all pages. The ordinary Fit command still fits the artwork.
Opening a saved paged document fits the first page.

Changing paper size or page count leaves object positions and sizes intact.
**Center selection on page** moves complete connected molecules, integral groups
and selected captions/graphics to the center of that page's margins. With nothing
selected, **Center drawing on page** moves the full drawing. Centering is undoable
and preserves bond lengths, labels, stereo and object IDs.

**Export pages as PDF…** writes one vector PDF page per sheet, including empty
sheets, at the stored physical dimensions. It retains the JACS/ACS bond and font
scale. Marks beyond paper edges are clipped; the panel reports marks that cross
an edge or lie outside the sheets. Margins are guides, not clipping boundaries.
Page numbers, margin lines and the gray canvas do not print. Ordinary drawing
PDF/SVG/PNG exports and clipboard images remain cropped to the artwork.

Native documents have stored the optional layout since document version 12 (current version: 13). Existing
version 1–11 documents open without pages. Save, recovery, Undo/Redo and chemistry
analysis/cleanup retain page settings. Removing pages restores an unbounded
canvas without deleting objects.

## Printing on macOS

Press **⌘P** or choose **Export → Print…**. The native dialog previews all
publication pages at **100%** scale and provides printer, paper, orientation,
page-range and PDF controls. Physical placement matches the page PDF. Printer
hardware may clip content near a paper edge; the drawing is not shifted to fit
that hardware margin. Changing the dialog's scale intentionally changes output
size and anchors the artwork to the top-left of the sheet.

**Export → Print selection…** prints only selected objects, centered within a
single sheet using the document's paper size and margins. Visible hydrogen and
stereo labels are retained. Without page settings, printing uses a centered A4
sheet. Drawings or selections too large for that sheet are rejected with guidance
to choose a larger paper or a page grid; printing never silently shrinks them.

Printing uses a frozen copy of the drawing and runs independently of the canvas.
You can keep editing while the dialog is open. Cancel adds no Undo step and
changes no drawing or saved page settings. Valid fields still open in Page setup
are used for that print without applying them to the document. Finish an inline
text draft before printing to include the latest text; ⌘P commits that draft.
Use the dialog's PDF button to save the print result as a PDF.

## Validation

`tests/pages.rs` checks physical sizes, page ordering, native serialization,
legacy loading, invalid dimensions, selection centering, clipping warnings,
PDF page counts/media boxes and retention through chemistry operations.
App tests check explicit Apply/Cancel, dirty/recovery state, one-step Undo,
concurrent drawing edits, stale-file rejection and view-only page navigation.

Desktop checks on 2026-09-20 used an isolated app and document: set up two A4
pages, saved/reopened the layout, duplicated a molecule and Japanese caption,
centered only the duplicate on page 2, fitted the spread and exported both pages.
A subsequent landscape-only edit showed the unsaved indicator and edge warnings;
Undo restored the saved portrait layout. PDFKit reported two 595.2756 × 841.8898 pt pages, extracted the Japanese text
from each, and rendered the expected vector drawing on both sheets.

`tests/printing.rs` checks nonmutating print copies, default/custom paper,
selection isolation, preserved labels and indicator positions, oversize rejection
and physical PDF dimensions. App tests cover cancellation, failures, duplicate
jobs, stale results and Page setup drafts. On macOS, `tests/test_native_print.py`
compiles the production Swift renderer and verifies Save-to-PDF output for
portrait, landscape and custom sizes, 100% and 50% scales, page order and ranges.
It also rejects malformed requests before opening native UI.

Desktop print checks on 2026-09-20 opened the two-page A4 document with ⌘P,
changed drawing colors while the dialog stayed open, saved the original print
copy as PDF and cancelled a subsequent selection print. The corrected native
PDF has matching artwork bounds and 595.2756 × 841.8898 pt media boxes on both
pages; Japanese text remains extractable. A standalone bundle launched from
`/tmp` also printed only one of two ethanol molecules, retaining its OH label
on a single A4 page. Its bundled helper and chemistry worker were used. No
physical print job was submitted.

## Remaining work

Native printing is available on Windows and macOS. On Windows use Ctrl+P; Page setup previews the drawing, and the system print dialog selects the printer, pages and copies. Choose Microsoft Print to PDF for a local PDF. See the [Windows guide](windows.md). Physical printer output has not been
verified; Save to PDF and cancellation have. Printer-specific printable-area
guides on the canvas, calibrated screen actual-size view, automatic pagination,
headers/footers and independent sizes for different sheets are not implemented.
Pages share a uniform paper size and margins. Molecular interchange formats do not preserve this page-layout metadata;
use the native document to retain it.
