---
section: Fixes
issues: []
---

- When a 3D model can't be built, the message now names the atom and its element, for example "Can't build a starting 3D structure for Ce (cerium)", rings that atom on the canvas, and offers **Open a 3D file…** in Properties → Structure. Approximate models no longer end with an internal "(strict: …)" element list.
- Radicals now get a 3D model built from their closed-shell parent (propane for a propyl radical) with MMFF94s, and the added hydrogens are left out of the model, instead of an approximate UFF model.
