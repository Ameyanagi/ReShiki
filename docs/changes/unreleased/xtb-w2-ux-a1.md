---
section: Changes
issues: []
---

- A failed xTB run shows only its own failure: the last successful run stays
  collapsed, greyed and timestamped, without Copy. When a lowered Max SCF
  iterations caused the failure, Reset to 250 and retry fixes it in one click.
- The xTB card and results palette show only the selected molecule's own
  result, and new 3D models are placed clear of reaction arrows, plus signs and
  other species.
- An outdated 3D model is greyed with an "Outdated" badge on the canvas, the
  open status counts outdated results, the Export panel says when a 3D model
  does not match the drawing, and one Recalculate (rebuilds the 3D model) button
  replaces Update model when the molecule has an xTB result.
