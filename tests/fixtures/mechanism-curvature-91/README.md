# Mechanism curvature regression examples (#91)

`before.rsk` retains two methoxide/carbonyl drawings. The upper electron-pair arrow needs a tight departure bend beside the nucleophile; the lower fishhook is the same fixed-endpoint geometry used to assess independent arrival/departure editing. Arrows are drawing examples, not an electron-accounting assertion.

Baseline `51fa0991` has endpoints and one on-curve midpoint handle. Select arrow 101 or 102, drag its square midpoint, then drag an endpoint. Its single quadratic control ties the two endpoint tangents together. Match zoom, framing, drawing style and these same input rows for before/after screenshots. Selecting an arrow shows the controls that are under review; capture a second deselected view for the finished drawing.

`independent-controls.cdxml` contains a single editable cubic with opposite endpoint control directions. Baseline rejects this curve because the two recovered quadratic controls differ. The intended replacement retains both original cubic controls exactly, without approximation.

After implementation: drag the departure handle on arrow 101 while leaving its endpoints and arrival control fixed; drag the arrival handle on arrow 102 independently. Test Escape during a drag, one Undo/Redo, save/reopen, reverse, flip, straighten, duplication and export. SVG/PDF/PNG use the common app geometry. CDX coordinates quantize to 1/65536 pt; CDXML arrowhead dimensions quantize to 1/100 pt.

`after.rsk` keeps every atom, bond and arrow endpoint from `before.rsk`. Starting from the before fixture, select arrow 101 and drag departure control (40, -45) to (42, -55), then arrival control (72.3333, -43) to (139, -55). For arrow 102 drag departure control (74.3333, 111.6667) to (42, 140), then arrival control (106.6667, 113.6667) to (139, 208). Coordinates are drawing-world units. The upper curve approaches both ends vertically; the lower curve changes concavity with independent endpoint directions. Controls are square on direction lines; the on-curve diamond adjusts the whole bend.
