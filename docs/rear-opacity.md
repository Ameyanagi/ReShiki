# Rear-side opacity (under review)

Use **Properties → 3D appearance → Rear opacity (%)** to make the back of a
projected molecule lighter while keeping the front visible. The default is
**100%**, which keeps the existing drawing opaque.

1. Open a drawing with retained 3D coordinates. Select part of a molecule to
   target that complete covalent molecule, or clear the selection to target
   the drawing's molecules.
2. Enter a percentage from **0** to **100** in **Rear opacity (%)**.
3. Click **Apply**. **25%** leaves faint rear ink; **0%** makes rear ink
   transparent. Undo restores the previous setting and Redo restores the edit.

![C60 with rear opacity set to 25%, its front bonds visible, and unchanged molecular properties](images/rear-opacity/c60-rear-opacity-25.jpg)

Opacity works against both light and dark backgrounds and in transparent
figure exports. At 0%, the background shows through. Flat 2D molecules retain
their ordinary appearance. Molecular identity and the retained XYZ coordinates
stay unchanged; this control changes the drawing's appearance.

Save the editable drawing as **ReShiki native** to retain the setting. This
feature saves native document version **22**, which older released versions
cannot open. Keep an earlier-format copy if it is needed elsewhere. SVG, PNG
and PDF preserve the visible result. Chemical formats such as MOL, SMILES and
InChI report that drawing appearance is omitted. Editable CDXML/CDX cannot
retain rear opacity; use native or figure output, or review the reported
conversion when making an external clipboard copy. Partial-opacity EMF
export is refused with alternative formats offered. Windows print and EMF
runtime validation remains pending.

This feature is under review as a follow-up to [PR #271](https://github.com/Ameyanagi/ReShiki/pull/271).
See the [matched desktop views, saved-data checks and limits](rear-opacity-review.md).
