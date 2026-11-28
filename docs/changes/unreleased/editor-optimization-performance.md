---
section: Changes
issues: []
---

- Bond drags and ring previews respond faster on large drawings, including pages with many captions. During an xTB geometry optimization, the canvas reuses the surrounding artwork as the molecule moves.
- Creating a 3D model can generate and minimize conformers in parallel. Live force-field relaxation takes longer uninterrupted steps while pins and drag targets stay in place. Selecting the active force field keeps the current calculation running.
- On macOS and Linux, geometry optimization can start the next xTB step as soon as the worker returns a gradient.
