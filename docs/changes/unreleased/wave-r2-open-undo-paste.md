---
section: Changes
issues: []
---

- **Open** shows a ChemDraw (`.cdxml`, `.cdx`) or MOL file as itself: the tab
  reads `aspirin · MOL`, with no unsaved dot until you edit. The first **Save**
  asks once whether to **Keep as MOL** (or CDXML, CDX) or **Save as ReShiki**,
  and remembers the answer while the file is open.
- **Save as…** lists **ReShiki (.rsk)**, **ChemDraw (.cdxml)**,
  **ChemDraw (.cdx)** and **MOL**. In the browser it still saves `.rsk` only.
- Switching the canvas between light and dark is no longer an Undo step and no
  longer marks the drawing as changed, like zoom, the grid and the panels. The
  canvas is still saved with the drawing.
- A pasted drawing wider than a quarter of the view lands one bond length to
  the right of the drawing and scrolls into view, instead of on top of it.
  Smaller pieces still land near the middle of the view.
