---
section: Changes
issues: [1175]
---

- **Properties under a molecule.** Right-click a molecule and choose
  **Show properties**, or click the pin beside a row in **Molecular
  properties**. The caption shows Formula, Mol. Wt., Exact Mass, cLogP, TPSA or
  elemental analysis, one line each, and updates with every edit in the same
  Undo step. Editing its text makes it ordinary text; deleting the molecule
  deletes it.
- CDXML and CDX save the caption as ChemDraw linked property text, and ChemDraw
  files with linked property text open with the link. A caption with cLogP or
  TPSA is saved as plain text, and so is ChemDraw text over more than one
  molecule when it opens. ChemDraw files that contain linked property text used
  to be refused. `.rsk` files with these captions use document
  version 38.
