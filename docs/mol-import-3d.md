# MOL import coordinates and hydrogen presentation

MOL V2000 and V3000 import retain the source coordinates without layout,
minimization or a generated conformer. The existing canvas conversion is
uniform: X is multiplied by 28, Y is inverted and multiplied by 28, and Z is
multiplied by 28. Native drawing storage uses `f32`, so only the corresponding
numeric rounding is expected. Import retains nonzero Z even when it is below
the parser's threshold for assigning stereo from a 3D conformer. Retained
depth allows the existing selection tilt controls to rotate the original
conformation. The source molecular tags and double-bond controls remain
authoritative when the projection becomes edge-on.
Explicit unknown double stereo is stored as native `any`; a source wavy single
bond remains a chemical unknown rather than projection paint. Both survive
native rotation and reopen without inventing an assigned configuration.

Ordinary terminal neutral, nonisotopic H attached by a single bond to a
supported main-group atom becomes an explicit-H count on its parent atom.
It contributes to the same chemical formula without crowding the drawing.
The shared graph operation adjusts neighbor parity and stereo controls;
the import adapter checks composition and CIP descriptors before publishing
the result. Kept source indices remap stable IDs, XYZ and annotations together.
The lossless raw MOL parser is unchanged.

Isotopic, charged, radical, isolated, bridging, metal-bound, mapped and
annotation-bearing H remain explicit. Unknown H or bond stereochemistry
is protected. Unsupported non-tetrahedral removal cases retain their H.
Any graph, coordinate, annotation or stereo validation failure returns an
error before publishing a partial drawing. Existing 2D inputs without
removable explicit H retain their current drawing.

Native `.rsk` save/reopen retains XYZ and the stereo controls. MOL export
continues its existing policy of producing a detached stereo-safe 2D
interchange depiction for projected drawings. Use native saves when the
original 3D conformation must survive a round trip; this change does not
alter MOL writer policy or claim conformation accuracy.
The existing projected MOL writer can omit a wavy single-bond unknown marker
while leaving that center unassigned; native storage retains the marker.
Explicit unknown double stereo is checked through MOL export as well.

Regression inputs and independent executable checks are described in
[the fixture README](../tests/fixtures/mol-import-3d/README.md).
