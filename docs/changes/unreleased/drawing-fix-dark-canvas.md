---
section: Changes
issues: []
---

- **Dark canvas and colored cards.** Structures, arrows and text on a filled
  shape take the ink that reads on the fill: black on a light grey card even
  on the dark canvas, and white on a dark card on the light one. Panel titles
  in the border sit on the panel's own fill instead of a black or white box.
  The **Previous work vs this work** layout uses theme colors, so it reads on
  both canvases. A new gradient starts from **Paper**, the canvas color, so a
  default gradient keeps its range on the dark canvas. ChemDraw files (CDXML,
  CDX) are always written with the light canvas's look and no page rectangle.
  Imported ChemDraw and MOL files open on the canvas you chose last, as new
  drawings do. See [Drawing styles](drawing-styles.md#palette-colors).
