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

Native documents store the optional layout in document version 12. Existing
version 1–11 documents open without pages. Save, recovery, Undo/Redo and chemistry
analysis/cleanup retain page settings. Removing pages restores an unbounded
canvas without deleting objects.

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

## Remaining work

A native print dialog, printer-specific printable areas, calibrated screen
actual-size view, automatic pagination, headers/footers and independent sizes
for different sheets are not implemented. Pages share a uniform paper size and
margins. Molecular interchange formats do not preserve this page-layout metadata;
use the native document to retain it.
