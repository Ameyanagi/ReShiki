---
section: Developer
issues: []
---

- Calculated orbitals can be placed on 3D models as vector figures: ReShiki evaluates one molecular orbital from its basis and coefficients, meshes it off the UI thread and draws it hidden correctly behind atoms and bonds. A placed figure keeps its orbital, isovalue and look, the orbital's coefficient row in the saved result, and a vector snapshot, so files print and export in builds without the orbital data. The `RESHIKI_DEV_ORBITAL_SURFACE` developer switch is gone. See [orbital surfaces](plans/xtb-surfaces.md#phase-5-real-orbitals-in-the-drawing).
