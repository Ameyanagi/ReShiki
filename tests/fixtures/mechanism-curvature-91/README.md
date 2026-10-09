# Mechanism curvature regression examples (#91)

`before.rsk` retains two methoxide/carbonyl drawings. The upper electron-pair arrow needs a tight departure bend beside the nucleophile; the lower fishhook is the same fixed-endpoint geometry used to assess independent arrival/departure editing. Arrows are drawing examples, not an electron-accounting assertion.

Baseline `51fa0991` has endpoints and one on-curve midpoint handle. Select arrow 101 or 102, drag its square midpoint, then drag an endpoint. Its single quadratic control ties the two endpoint tangents together. Match zoom, framing, drawing style and these same input rows for before/after screenshots. Selecting an arrow shows the controls that are under review; capture a second deselected view for the finished drawing.

`independent-controls.cdxml` contains a single editable cubic with opposite endpoint control directions. Baseline rejects this curve because the two recovered quadratic controls differ. The intended replacement retains both original cubic controls exactly, without approximation.

After implementation: drag the departure handle on arrow 101 while leaving its endpoints and arrival control fixed; drag the arrival handle on arrow 102 independently. Test Escape during a drag, one Undo/Redo, save/reopen, reverse, flip, straighten, duplication and export. SVG/PDF/PNG use the common app geometry. CDX coordinates quantize to 1/65536 pt; CDXML arrowhead dimensions quantize to 1/100 pt.
