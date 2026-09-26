# Exporting figures

Open **Export → Figure**, choose PDF, SVG or PNG, then click **Export** and choose a destination. File export includes the whole drawing. **Copy image** uses the selected objects, or the whole drawing when nothing is selected.

| Format | Output                                                                      |
| ------ | --------------------------------------------------------------------------- |
| PDF    | Vector artwork at the drawing's physical publication size.                  |
| SVG    | Vector artwork for illustration and layout applications. Text remains text. |
| PNG    | An opaque image, normally at 1200 dpi, with physical resolution metadata.   |

File exports in all three formats include the document’s canvas colors and background. **Copy image** instead uses a transparent background, preserving the visible ink for slides and other documents. **Canvas: Dark** exports white bonds and labels on black paper; **View → Interface** changes only the surrounding UI.

Canvas zoom, rulers, grid and crosshair do not affect the exported figure. **Export pages as PDF** uses an explicitly configured publication layout; ordinary figure export crops to the artwork. See [publication pages](publication-pages.md).

## Large PNG drawings

Large drawings, including the complete shortcut gallery, may exceed the image memory limit at 1200 dpi. File export automatically chooses the highest resolution that fits, from 1200, 600, 300, 150, 96 and 72 dpi, with a limit of 80 million pixels. This changes pixel density, not the physical dimensions, drawing content or native document.

The saved-file status reports the actual width, height and DPI. Use SVG or PDF if you need vector quality at any magnification. Exceptionally large drawings that exceed the limit even at 72 dpi still require a vector format. Clipboard PNG images retain their fixed 1200-dpi sizing and can be omitted for an oversized drawing; see [clipboard support](clipboard.md).

## Drawings with unresolved chemistry

Molecular analysis and figure rendering have different requirements. A visible aromatic ring, variable group or unfinished structure can be drawn even when the chemistry engine cannot assign a molecular identity.

Figure export refreshes computed labels when possible. If chemistry analysis fails, it exports the existing drawing and shows a review notice after saving. It does not infer a charge, replace aromatic bonds, remove objects or modify the open document. Invalid drawing geometry or references still prevent export. Chemical data exports such as SMILES and InChI retain their own validation requirements.

[Before/after evidence and desktop verification](changes/figure-export.md).
