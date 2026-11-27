---
section: Fixes
issues: []
---

- 3D models now build molecules MMFF94s cannot type, such as O₂, ozone and CO, with UFF instead of stopping, and say so in the status bar.
- The 3D model view now shows radicals and six-coordinate compounds of elements such as uranium and cerium as approximate UFF models with a note, where it used to refuse them. UF₆ now comes out octahedral; before, two fluorines collapsed to 1.19 Å apart. Shapes UFF gets wrong (3- and 4-coordinate lanthanides and actinides such as CeF₃ or ThCl₄, and noble-gas compounds such as XeF₂) are refused with an explanation.
