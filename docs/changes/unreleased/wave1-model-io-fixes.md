---
section: Fixes
issues: [1121]
---

- **Move & attach** no longer refuses a molecule that has a name caption, a
  depth appearance or a 3D model, or an arrow that has condition text. The
  caption, text and appearance move with the fragment.
- Dragging a ring onto a bond no longer fuses it when a ring atom carries a
  radical, a lone pair or other mark, or a stereo group. Fusing used to delete
  them without notice. A bond whose atoms carry any of these, stereo, an atom
  map or a fixed hydrogen count is no longer a target, as with a ring template.
- Saving over a file keeps its permissions, and saving through a symbolic link
  writes to the linked file instead of replacing the link. A newly saved file
  gets the usual permissions instead of owner-only ones; recovery drafts and
  settings stay owner-only. This applies to macOS and Linux.
- **Clean up…** with **Keep orientation** no longer mirrors the drawing. When
  the new layout is the mirror image of the drawing, ReShiki reflects it back
  before matching the rotation.
- Files that start with a UTF-8 byte-order mark now import. An RXN file used to
  fail with "Missing RXN header", and an InChI file was read as SMILES.
- A MOL or RXN file whose `ZCH`, `HYD` or `IMPL_H` data group gives a charge or
  hydrogen count that is out of range now imports without that value, and an
  import warning names the group and the value. The value used to wrap, so a
  charge of 200 imported as -56.
- Recovery drafts made with a newer ReShiki are no longer hidden. The status
  bar shows how many there are, with **Open folder** to find the files.
- SMILES and SMARTS text is trimmed the same way on every platform: characters
  outside ASCII at either end, such as a no-break space after a pasted
  structure, are dropped. Linux on arm64 used to reject such text.
