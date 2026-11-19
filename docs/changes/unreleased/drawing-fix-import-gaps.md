---
section: Fixes
issues: []
---

- Dashed lines, shapes and arrows now draw as ChemDraw does: dashes and gaps
  2.7 pt long with square ends, whatever the line width. A bold dashed line
  was a few long round-ended strokes. See [Graphics](graphics.md).
- No-go crosses and hashes now reach five line widths either side of the
  shaft, as in ChemDraw; they were a fixed 2 pt and looked small on a ChemDraw
  arrow.
- ChemDraw files: an unbalanced equilibrium arrow keeps its short arrow, the
  forward or the reverse one, and a retrosynthesis arrow's head is as wide as
  ChemDraw draws it, so both shafts run into it. **Reverse length** now goes
  up to 5, which makes the forward arrow the short one, and CDXML and CDX
  saves keep the ratio. See [Arrows](arrows.md).
- ChemDraw files: a doubled curve opens as two grouped parallel curves, the
  Rac, Abs and Rel stereo flags keep their rounded box, and a label written
  hydrogens first, such as H2O, keeps its hydrogens on the left. See
  [ChemDraw styles ReShiki cannot draw yet](graphics.md#chemdraw-styles-reshiki-cannot-draw-yet).
