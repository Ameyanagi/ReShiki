---
section: Fixes
issues: []
---

- ChemDraw diagrams containing several molecules now open correctly when the
  document uses a non-default bond length. Supported pages are arranged
  vertically on one editable canvas, with a notice about page settings that
  cannot be retained.
- Hidden ChemDraw drawing objects stay absent from visible output, with a
  notice when their visibility settings cannot be retained. Atom labels keep
  inherited bold, italic, underline, and color settings without requiring an
  extra text object. Binary text export also keeps inherited typography and
  Unicode text.
- Supported generic choices and polymer repeats can be saved as editable
  CDXML/CDX drawings without first expanding them into concrete molecules.
  Transformed semantic bracket frames and dashed or dotted semantic brackets
  refuse export with advice to keep the native drawing. Legacy curve arrowheads also import
  through the supported path geometry.
- ChemDraw stationery files (`.cds`) containing artwork can be opened as
  drawings. Importing a stationery file as a drawing style remains a separate
  operation. See the [corpus and repair record](changes/chemdraw-import-corpus-2026-11-17.md).
