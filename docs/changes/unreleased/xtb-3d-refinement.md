---
section: Changes
issues: []
---

- In builds with xTB, 3D models the force field can't be trusted for (O₂, CO, UF₆, metal complexes, radicals) are refined with GFN2-xTB or g-xTB while the model is built, with progress and Cancel. Shapes the force field gets wrong, such as ThCl₄ and XeF₄, now get a rough start refined with xTB instead of a refusal.
- The xTB Calculate card keeps the 3D model's notes (UFF used, radical parent, refinement) with its result, and says why a 3D model could not be built when a calculation can't start.
