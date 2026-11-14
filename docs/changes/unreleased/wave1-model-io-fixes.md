---
section: Fixes
issues: [1121]
---

- **Move & attach** no longer refuses a molecule that has a name caption, a
  depth appearance or a 3D model, or an arrow that has condition text. The
  caption, text and appearance move with the fragment.
- Dragging a ring onto a bond no longer fuses it when a ring atom carries a
  radical, a lone pair or other mark, or a stereo group. Fusing used to delete
  them without notice.
- Saving over a file keeps its permissions, and saving through a symbolic link
  writes to the linked file instead of replacing the link. New files get the
  usual permissions instead of owner-only ones. This applies to macOS and Linux.
- **Clean up…** with **Keep orientation** no longer mirrors the drawing. When
  the new layout is the mirror image of the drawing, ReShiki reflects it back
  before matching the rotation.
- Files that start with a UTF-8 byte-order mark now import. An RXN file used to
  fail with "Missing RXN header", and an InChI file was read as SMILES.
- A MOL file whose `ZCH`, `HYD` or `IMPL_H` data group gives a charge or
  hydrogen count that is out of range is now an error, as it is in `M  CHG`.
  The value used to wrap, so a charge of 256 imported as 0.
- SMILES and SMARTS text is trimmed the same way on every platform. Non-ASCII
  characters at either end, such as a non-breaking space, are now an error
  everywhere; macOS, Windows and x64 Linux used to drop them.
- Recovery drafts made with a newer ReShiki are no longer hidden. The status
  bar shows how many there are, with **Open folder** to find the files.
