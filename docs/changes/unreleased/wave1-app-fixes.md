---
section: Fixes
issues: []
---

- Pasting a MOL file, SMILES or a ChemDraw structure, and **Insert** on the
  Import tab, no longer turn **Show stereodescriptors** off for the new atoms.
  A structure from another program follows the drawing's label settings. A
  drawing copied in ReShiki still keeps the look it was copied with. The paste
  is one Undo step (fixes
  [issue #1151](https://github.com/Ameyanagi/ReShiki/issues/1151)).
- On macOS, closing the Print panel returns keyboard focus to the ReShiki
  window, so the next key acts on the drawing without a click (fixes
  [issue #1149](https://github.com/Ameyanagi/ReShiki/issues/1149)).
- On Windows, several drawings opened from the same Office application get
  numbered tabs: `Drawing in PowerPoint.rsk`, then
  `Drawing in PowerPoint (2).rsk`, and so on (fixes
  [issue #1119](https://github.com/Ameyanagi/ReShiki/issues/1119)).
- A newer ReShiki's interface or assistant preferences that this version cannot
  read are no longer lost. ReShiki starts with the default settings and leaves
  the file in place. Before the first changed setting replaces it, ReShiki keeps
  a copy as `appearance.json.bad` or `assistant-preferences.json.bad` in its data
  folder. If that copy cannot be written, the setting is not saved and ReShiki
  says why.
- **Expand generic structures** is available again after an expansion finishes
  while the recovery drafts dialog is open. The status bar says that the
  expansion was discarded.
- A drawing opened from Word, PowerPoint, Excel or LibreOffice no longer stays
  busy when its document closes while dropped files are still loading.
- After an edit that ReShiki cancels, such as a paste that would join separate
  reaction participants, the objects that were removed again are no longer
  selected.
