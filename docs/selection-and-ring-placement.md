# Selection and ring placement

Updated 2026-09-17 after reports of missing bond targets, an unwanted persistent bond preview, and a substituent drawn inside a newly attached ring.

## Behavior

- Hovering an atom draws an outline circle. Hovering a bond marks both endpoints and highlights its segment. Selected atoms retain stronger outlines. Clicking a bond with Select selects its two endpoint atoms; their normal move, transform and delete semantics apply. Clicking one of those atoms narrows the selection; dragging a selected atom moves the current selection.
- Bond-order clicks select both endpoints so the edited bond is visibly identified. The single → double → triple → single cycle remains available.
- No extra bond follows the idle pointer. The bond preview appears during a drag and vanishes on release. Leaving the canvas requests a redraw; leaving the window clears the hover position.
- Attaching a ring at a terminal atom puts its center opposite the existing substituent. Other atom attachments use the widest angular gap. New ring bonds match the length of the attachment bond, or the mean incident bond length for an atom attachment. A new standalone ring uses the JACS default length.
- With the ring tool, drag from empty space onto an atom or bond to attach on release. Starting on an attachment locks it while dragging chooses the side/orientation. Escape cancels the gesture. Placement previews use the same graph operation as the committed edit.
- With Select, grab an isolated saturated carbon ring by its interior, or select it and drag one of its edges toward a single bond. The preview rotates and uniformly scales the ring to fit. On release, two shared atoms are merged and the existing target bond is retained. The whole attachment is one Undo step.

Moving an existing ring supports standalone, unlabelled saturated carbon rings of 3–8 atoms. Substituted, fused, heteroatom, unsaturated, aromatic and explicitly stereochemical rings retain ordinary movement; they do not silently merge. The ring-placement tool continues to support its existing aromatic placement. This is local geometric placement, not global structure cleanup. Existing diagrams with inward substituents are not modified automatically.

## Verification

The user's working drawing was preserved in `artifacts/selection-ring-session-20260917.reshiki`. Desktop checks used a separate app instance and recovery directory while the user continued trying the main app.

- Created a bond, selected its middle and inspected circles at both endpoints. Clicking one atom changed the selection count from two to one.
- Attached a six-member ring to an ethane endpoint. The methyl group stayed outside; RDKit returned `CC1CCCCC1`, `C7H14`.
- Created a longer target bond by moving its terminal atom. Drew cyclopentane elsewhere, selected its interior and dragged an edge to the target. The ring enlarged and rotated; the two shared atoms were reused. Counts changed from 14 atoms/13 bonds to 12/12 across the two fragments. Undo/Redo restored and reapplied the attachment.
- Drew another target bond and used the five-member ring tool to drag from empty space onto its midpoint. The new ring snapped to the bond. The resulting three-fragment drawing had 17 atoms/17 bonds, formula `C17H34` and three rings.
- Saved `artifacts/selection-ring-check.reshiki` through the native dialog. Reading the file confirmed the counts and bond lengths of 42.00 and 68.76 world units, demonstrating that the rescaled ring matched the longer target while the other structures retained their dimensions.

Automated checks cover atom and bond pointer targets, click-versus-drag selection, hover clearing, ring drag anchors, 36 terminal orientation/scale combinations, requested fusion sides, rotation/rescaling and atom reuse, exact Undo/Redo restoration, and RDKit identities for methylcyclohexane, methylenecyclohexane and cyclopentane. Existing chemistry, file, rendering and history tests remain in the suite. The update passes 36 Rust tests, 10 Python tests, formatting, Clippy and bundle signature verification.
