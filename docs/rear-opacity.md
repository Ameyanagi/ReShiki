# Rear-side opacity

Available in Nightly builds; not in ReShiki 0.11.0.

Use **Properties → 3D appearance → Rear opacity** to make view-occluded
rear ink lighter while keeping exposed cage outlines solid. The default is
**100%**, which keeps the existing drawing opaque.

1. Open a drawing with retained 3D coordinates. Select part of a molecule to
   target that complete covalent molecule, or clear the selection to target
   the drawing's molecules.
2. Enter a percentage from **0** to **100** in **Rear opacity**.
3. Click **Apply**. **25%** leaves faint rear ink; **0%** makes rear ink
   transparent. Undo restores the previous setting and Redo restores the edit.

![C60 at 25% rear opacity with faint occluded rear ink and a solid exposed rim](images/rear-visibility-20261010/c60-25-after.jpg)

Opacity works against both light and dark backgrounds and in transparent
figure exports. At 0%, the background shows through. Flat 2D molecules retain
their ordinary appearance. Molecular identity and the retained XYZ coordinates
stay unchanged; this control changes the drawing's appearance.

Save the drawing as a `.rsk` file to keep the setting. SVG, PNG and PDF keep
the visible result. Chemical formats such as MOL, SMILES and InChI report that
they do not keep rear opacity. Editable CDXML/CDX cannot keep rear opacity; use
a `.rsk` file or figure output, or set rear opacity to 100% first. EMF for Office
export accepts only 0% or 100% and suggests SVG, PDF or PNG otherwise.

See the [corrected cage visibility, desktop checks and limits](changes/rear-visibility-20261010.md).
