---
section: Fixes
issues: []
---

- CDXML/CDX export keeps the holes and overlaps of opaque compound paths, including imported SVG artwork. Original Bézier curves and outlines remain editable, and holes reveal the artwork behind them. Faded and shaded compound fills retain their existing export behavior.
- Removing fill from an imported borderless path gives it a normal outline and applies the edit to the full selection.
