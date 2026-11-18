---
section: Fixes
issues: []
---

- **Show chemical name** wraps a long name under its molecule again, as in
  0.15, so names of neighboring molecules no longer run into each other. The
  property lines below the name stay on one line each.
- Selecting an atom no longer rewrites linked property text opened from a
  ChemDraw file, or marks the drawing as changed. The text stays as ChemDraw
  wrote it until its molecule changes.
- A custom name stays with its molecule when the caption's last row is hidden,
  the caption is deleted, or another caption line is edited; showing the Name
  row brings it back. The ↺ button still returns to the automatic name.
