---
section: Developer
issues: []
---

- Drawings can keep calculation results with their molecules: exact frames and results are stored beside the drawing, linked to its atoms, saved in their light form (heavy arrays only as compact binary), shared through undo history and left out of the clipboard. Any chemical edit marks a molecule's results out of date and Undo restores them. Files with results use document version 42, so older ReShiki asks to be updated instead of dropping them. See [the data design](plans/xtb-data.md#documents).
