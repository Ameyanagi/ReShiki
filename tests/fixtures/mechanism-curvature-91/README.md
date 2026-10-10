# Mechanism curvature regression examples (#91)

`before.rsk` retains two methoxide/carbonyl drawings. The upper electron-pair arrow needs a tight departure bend beside the nucleophile; the lower fishhook is the same fixed-endpoint geometry used to assess independent arrival/departure editing. Arrows are drawing examples, not an electron-accounting assertion.

Baseline `51fa0991` has endpoints and one on-curve midpoint handle. Select arrow 101 or 102, drag its square midpoint, then drag an endpoint. Its single quadratic control ties the two endpoint tangents together. Match zoom, framing, drawing style and these same input rows for before/after screenshots. Selecting an arrow shows the controls that are under review; capture a second deselected view for the finished drawing.

`independent-controls.cdxml` contains a single editable cubic with opposite endpoint control directions. Baseline rejects this curve because the two recovered quadratic controls differ. The intended replacement retains both original cubic controls exactly, without approximation.

After implementation: drag the departure handle on arrow 101 while leaving its endpoints and arrival control fixed; drag the arrival handle on arrow 102 independently. Test Escape during a drag, one Undo/Redo, save/reopen, reverse, flip, straighten, duplication and export. SVG/PDF/PNG use the common app geometry. CDX coordinates quantize to 1/65536 pt; CDXML arrowhead dimensions quantize to 1/100 pt.

`after.rsk` keeps every atom, bond and arrow endpoint from `before.rsk`. Starting from the before fixture, select arrow 101 and drag departure control (40, -45) to (42, -55), then arrival control (72.3333, -43) to (139, -55). For arrow 102 drag departure control (74.3333, 111.6667) to (42, 140), then arrival control (106.6667, 113.6667) to (139, 208). Coordinates are drawing-world units. The upper curve approaches both ends vertically; the lower curve changes concavity with independent endpoint directions. Controls are square on direction lines; the on-curve diamond adjusts the whole bend.

## Actual desktop-saved result

`desktop-edited.rsk` was saved through **Save As** in the fresh native default-feature debug application at source `477b97f4da391c628d4e7fe5a7998f5b335b9ff3`, macOS 26.5.1 arm64. It is the actual pointer-edited result used for the published [matched desktop review](../../../docs/changes/mechanism-curvature.md), rather than the programmatically prepared `after.rsk` renderer examples.

At 175% zoom with F8 off, drag arrow 101's departure control from screen (798, 494) to (805, 459), Undo once, Redo, then drag its arrival control from (911, 502) to (1145, 459). Both endpoints remain unchanged and each drag changes only its chosen tangent. Reverse, Flip bend and Straighten through their native accessibility actions each work and each single Undo restores the edited cubic. Deselect for the output screenshot, then Save As. The lower fishhook is untouched.

A field-by-field JSON comparison verifies the same ten atom IDs/elements/positions/charges and six bonds/orders, unchanged arrow endpoints, an exactly unchanged lower quadratic arrow, and no new annotations/graphics/groups. Newly written atom/bond fields are serialization defaults; `label_h` stores derived hydrogen-label counts (3 on atoms 1/5/11/15, 1 on 3/13, 0 elsewhere), with explicit hydrogens and connectivity unchanged. Arrow 101 now has independent cubic controls (41.83599, -55.118538) and (139.12682, -55.118538), reflecting actual mouse precision. They cannot be expressed as one quadratic control. The native version is 20.

SHA256: `before.rsk` is `227c377dd72f54ac471b5d52410efeb67cf55669e8edc41af1d699535047fc0c`; `desktop-edited.rsk` is `2afc5cb2b91c67405f34ab9b8f7d3976e09d0003dd3d548e8f37b97173d39ed7`. The original input remains byte-identical to the retained fixture.
