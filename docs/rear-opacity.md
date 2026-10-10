# Rear-side opacity (under review)

Use **Properties → 3D appearance → Rear opacity (%)** to make view-occluded
rear ink lighter while keeping exposed cage outlines solid. The default is
**100%**, which keeps the existing drawing opaque.

1. Open a drawing with retained 3D coordinates. Select part of a molecule to
   target that complete covalent molecule, or clear the selection to target
   the drawing's molecules.
2. Enter a percentage from **0** to **100** in **Rear opacity (%)**.
3. Click **Apply**. **25%** leaves faint rear ink; **0%** makes rear ink
   transparent. Undo restores the previous setting and Redo restores the edit.

![C60 at 25% rear opacity with faint occluded rear ink and a solid exposed rim](images/rear-visibility-20261010/c60-25-after.jpg)

Opacity works against both light and dark backgrounds and in transparent
figure exports. At 0%, the background shows through. Flat 2D molecules retain
their ordinary appearance. Molecular identity and the retained XYZ coordinates
stay unchanged; this control changes the drawing's appearance.

Save the editable drawing as **ReShiki native** to retain the setting. This
combined build saves native document version **24**, which older released versions
cannot open. Keep an earlier-format copy if it is needed elsewhere. SVG, PNG
and PDF preserve the visible result. Chemical formats such as MOL, SMILES and
InChI report that drawing appearance is omitted. Editable CDXML/CDX cannot
retain rear opacity; use native or figure output, or review the reported
conversion when making an external clipboard copy. Partial-opacity EMF
export is refused with alternative formats offered. Windows print and EMF
unit tests passed on the reviewed PR #289 head. Combined-tree validation and
native Windows desktop acceptance remain pending. The captured PR #289 native
files remain byte-exact version **22** evidence.

This feature is under review in [PR #289](https://github.com/Ameyanagi/ReShiki/pull/289),
following [PR #271](https://github.com/Ameyanagi/ReShiki/pull/271).
See the [corrected cage visibility, native checks and limits](changes/rear-visibility-20261010.md).
