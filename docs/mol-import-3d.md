# MOL import coordinates and hydrogen presentation

Available in Nightly builds; not in ReShiki 0.11.0.

When you open a MOL file with 3D coordinates, ReShiki keeps them, so you can
tilt the molecule. Ordinary hydrogens become H counts; hydrogens with an
isotope, charge, map number or stereo stay as atoms. Save as a `.rsk` file to
keep the 3D shape; MOL export writes a flat drawing.

## Use it

1. Open a V2000 or V3000 MOL file, or paste MOL text into **Import**.
2. Select the molecule and use **3D tilt** (right-click menu or tool palette)
   to turn it. The atoms keep their original 3D positions.
3. Save as a `.rsk` file to keep the coordinates and stereo for later.

## Limits

- Import keeps the coordinates exactly; it does not clean up, minimize or
  generate a conformer.
- Hydrogens that are charged, isotope-labeled, radical, isolated, bridging,
  bound to a metal, mapped or annotated stay as drawn atoms. So do hydrogens
  whose removal would change stereo.
- MOL export writes a flat 2D drawing that keeps the stereo. It does not keep
  the 3D shape.
- If anything in the file fails validation, nothing is imported.

## Implementation notes

The canvas conversion is uniform: X is multiplied by 28, Y is inverted and
multiplied by 28, and Z is multiplied by 28. `.rsk` drawing storage uses `f32`,
so only the corresponding numeric rounding is expected. Import retains nonzero
Z even when it is below the parser's threshold for assigning stereo from a 3D
conformer. Retained depth lets the selection tilt controls rotate the original
conformation. The source molecular tags and double-bond controls remain
authoritative when the projection becomes edge-on. Explicit unknown double
stereo is stored as `any`; a source wavy single bond remains a chemical unknown
rather than projection paint. Both survive rotation and reopen without
inventing an assigned configuration.

Ordinary terminal neutral, nonisotopic H attached by a single bond to a
supported main-group atom becomes an explicit-H count on its parent atom. It
contributes to the same chemical formula without crowding the drawing. The
shared graph operation adjusts neighbor parity and stereo controls; the import
adapter checks composition and CIP descriptors before publishing the result.
Kept source indices remap stable IDs, XYZ and annotations together. The
lossless raw MOL parser is unchanged.

Unknown H or bond stereochemistry is protected. Unsupported non-tetrahedral
removal cases retain their H. Any graph, coordinate, annotation or stereo
validation failure returns an error before publishing a partial drawing.
Existing 2D inputs without removable explicit H retain their current drawing.

`.rsk` save/reopen retains XYZ and the stereo controls. MOL export continues
its existing policy of producing a detached stereo-safe 2D interchange
depiction for projected drawings; it does not claim conformation accuracy. The
projected MOL writer can omit a wavy single-bond unknown marker while leaving
that center unassigned; `.rsk` files keep the marker. Explicit unknown double
stereo is checked through MOL export as well.

Regression inputs and independent executable checks are described in
[the fixture README](../tests/fixtures/mol-import-3d/README.md).
