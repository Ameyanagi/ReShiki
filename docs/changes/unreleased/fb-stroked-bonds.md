---
section: Fixes
issues: []
---

- SVG, PDF and EMF pictures, copied or exported, draw plain bonds as stroked
  lines in the drawing's line width. Changing the stroke width in Adobe
  Illustrator or Inkscape now changes every bond, not only the inner lines of
  ring double bonds. Corners and ends look as before; wedges and bold bonds
  stay filled ([Figure export](figure-export.md#bonds-in-vector-files)).
