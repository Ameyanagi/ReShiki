# UI cleanup acceptance fixtures

These are synthetic, editable fixtures for [issue #78](https://github.com/Ameyanagi/ReShiki/issues/78),
not captured desktop results. They use document version 17. Open copies for the
[native walkthrough](../../../docs/changes/ui-declutter-validation.md), leaving
these originals unchanged.

## Arrow width

`mixed-arrow-width.rsk` contains two independent molecules, two arrows, a
rectangle and two captions. The upper molecule's atoms are 1–3, the lower
molecule's atoms are 4–6, the upper curved arrow is 10, the lower dashed arrow
is 11, and the rectangle is 20. The upper molecule includes a bold bond so
that both ordinary and bold bond appearance can be checked.

The document bond line width is **0.8 pt**, with **2 pt** bold width. The upper
arrow starts at **1.2 pt**, the lower arrow at **0.45 pt**, and the rectangle
at **0.3 pt**. These deliberately different values expose editing the wrong
property or applying a change too broadly.

1. Select only the upper arrow. In **Properties → Arrow properties**, scroll
   to **Line width (pt)**, enter `1.5`, then press Enter.
2. Save the result under a new name. Undo, save that result separately, then
   Redo and save again.
3. Reopen the original fixture. Rectangle-select the upper molecule, upper
   arrow and rectangle, keeping the captions and lower row outside the
   selection. Repeat the width edit and Undo/Redo.

Only arrow 10's `style.width_pt` may change. Every other arrow property,
coordinate, atom, bond, graphic, caption and document-style value must remain
the same. Save a native baseline from the opened application before the edit
so that native serialization and any completed label refresh have the same
baseline. Compare parsed documents, not JSON whitespace. Record any derived
label refresh separately rather than silently discarding a chemistry change.

The application regression
`arrow_width_in_mixed_selection_preserves_bonds_and_other_objects` loads this
same fixture through the native reader. It exercises arrow-only and mixed
selection through application messages, exact document invariants, one-step
Undo/Redo, selection retention and native save/read. Its result is separate
from actually finding and using the field in the desktop application.

## Rejected ring

`ring-rejection.rsk` contains methane with four explicit hydrogens, centered
at the origin. In the Ring tool, click an empty area to place a valid ring,
then Undo. Try attaching a ring to the methane carbon. The preview/reason
should explain the rejected valence near the pointer; no modal dialog or
persistent panel should appear. The rejected attempt must leave the drawing,
selection and Undo/Redo unchanged. Redo must still restore the valid ring.

`regular_ring_rejection_preserves_selection_history_and_redo` loads this
fixture and checks the atomic rejection and preserved Redo step.

## Existing fixtures to reuse

- [Adjustable arcs](../adjustable-arcs.rsk): the seven-path gallery for arc
  editing and the fresh ChemDraw round trip.
- [TBDPS/OTBDPS gallery](../chemdraw-captions/source.rsk): six molecules,
  four collapsed group definitions and twelve captions. Use its
  [independent expectations](../chemdraw-captions/README.md).
- [Shortcut examples](../../../assets/examples/shortcut-examples.rsk): the
  ordinary editable gallery. Open through **Help → Open shortcut examples**
  when measuring that workflow, rather than substituting direct file opening.

Keep fresh native returns, ChemDraw GUI saves and screenshots with their run
record. Do not overwrite the historical interchange captures or use generated
exports as evidence that an external application accepted them.
