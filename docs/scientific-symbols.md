# Chemical symbols and orbitals

JACS / ACS remains the default: Arial 10 pt, black, 14.4 pt bonds and 0.6 pt lines. New restores those settings even after changing tool styles.

## Drawing and editing

The Chemical symbols palette button opens 18 choices: circled/plain charges, radicals/diradicals, lone pairs/bars, radical ions, H-dot/H-dash, attachment diamond/star/wave/bead and single/double daggers. Properties previews the actual styled geometry.

With **Attach to atoms** enabled, click an atom to add a charge, radical or lone pair. Drag from an atom to place its mark at a specific offset. Charges and radicals change the molecular graph's chemical state; lone pairs/bars are annotations. A check refreshes implicit hydrogen labels and reports unpaired electrons. Charge changes release imported implicit-label hydrogen counts so ammonium can become ammonia again.

Click-created lone pairs and bars sit just outside the visible label ink, with a modest gap scaled to its font size. New placements try the top, left, bottom and right sides, avoiding nearby label ink, bonds and existing atom marks. Pairs beside the label turn vertically. Dragging from an atom keeps the exact requested offset; existing saved marks do not move when labels or styles change. Very crowded drawings may still need manual positioning.

Select the atom and use **Position atom marks** to drag individual mark handles. Rotate turns a mark 45°. Remove deletes the mark and clears its represented charge/radical state. Movement, duplication and transforms carry marks with their owner; native undo/redo includes each edit. Atom-label color also colors its marks. Lone-pair annotations do not infer valence, electron count or orbital occupancy.

Free symbols are ordinary selectable graphics. H-dot/H-dash and attachment markers currently use free placement only; they do not define stereochemical hydrogen bonds, R-groups, polymers or reaction attachment semantics. Use an explicit H atom and stereobond for chemical hydrogen stereochemistry.

The Orbitals palette offers s, sigma, single lobe, p, sp³ hybrid, dxy and dz². Drag from the node to choose size/direction; Shift constrains to 15°. Click uses one default bond length. Outline, solid and flat-gray phases have live previews; multi-lobe orbitals support phase reversal. **Snap to atoms** controls node snapping independently of chemical-symbol attachment. The choice stays selected when switching tools in the current document; New restores snapping. Hold Option/Alt for temporary free placement inside the snap radius. A green ring identifies the chosen atom; the drag preview names the snapped target or free placement. An orbital remains a drawing object. Group it with the molecule when they should move together. Gray fill is not gradient shading.

Orbital fills and outlines leave a transparent gap around visible atom-label ink, including hydrogen, isotope and charge labels. This applies to orbitals both behind and in front of molecular content. Clicking these visible labels selects their atom through a foreground orbital. The atom positions, orbital node, axes and layer remain exactly as placed. The gap follows subsequent label edits; it is rendering clearance rather than a change to orbital size or attachment. Overlap with bonds, text captions and other annotations still follows ordinary object layering.

## Persistence and exchange

- Native format 8 retains orbital parameters, phases and atom-owned marks; earlier document versions still open.
- Canvas, SVG, PDF and PNG use shared vector geometry, including differently filled orbital lobes and rotated marks.
- SMILES/MOL/CDXML retain supported one/two-unpaired-electron chemistry; molecular formats omit drawing annotations.
- Free symbols and orbitals export to CDXML/CDX as grouped editable vector curves, including orbital label-clearance gaps. Importing those vectors retains their appearance, but not their parametric orbital/symbol controls or dynamic clearance after later label edits.
- ReShiki's external-curve importer currently applies a minimum 0.1 pt outline to fill-only curves. Orbital clearance includes padding that keeps these re-imported pieces clear of label ink; this can expand a filled vector edge by up to 0.05 pt.
- CDXML plain/solid orbital and supported free-symbol objects import as editable ReShiki objects. Gradient-shaded orbitals are rejected explicitly.
- CDXML atom-owned single charges, radical dots and supported lone pairs use `represent` ownership links. Carbon lone-pair annotation export is rejected because an external drawing application interprets it as diradical chemistry. Positioned multiple-charge marks, combined radical-ion marks, bars and colored attached marks require native/SVG/PDF/PNG; unsupported per-symbol CDXML overrides are rejected. External saves can discard ownership of distant marks, so export requires marks within 1.05 label sizes of their owner and closer to that atom than any other atom. A visible atom label and hydrogen count are written explicitly.
- Template attachment rejects marked or radical anchor atoms; free insertion of these templates retains their full drawing.

This is a bounded drawing/chemistry subset, not complete chemical-symbol or orbital parity. Arbitrary mark colors/sizes in the inspector, gradient shading, orbital fixed-length constraints and 3D orbital semantics remain incomplete.

## Verification

Interchange fixtures in `tests/fixtures/` cover a p orbital, a free circled plus and an atom-owned circled charge. The charge changes ammonia to ammonium and stores its owner with a `represent` child.

Tests cover all shapes/phases, native and vector persistence, atom ownership through transforms/copy, atomic rejection, history, radical/charge chemistry, supported exchange and explicit exchange failures. The complete suite passes 102 Rust and 30 Python tests.

Desktop checks used a standalone app launched from `/tmp`: ammonia → circled ammonium; mark repositioning with Undo/Redo; a rotated p orbital with reversed gray/blue phases; native save/reopen; vector/raster export; and NH₄⁺ → NH₃⁺ with one unpaired electron. New was exercised after setting 18 pt bold red text, a 3 pt graphic line and reversed phases; it restored JACS defaults. The final mark position was adjusted to clear the label while meeting the interchange proximity constraint.

A ReShiki-exported attached charge was opened and saved in an external drawing application, then re-imported and checked as `[NH4+]`; the saved result is retained under `tests/fixtures/`. A distant-position counterexample exposed loss of the charge/ownership link, and now causes an explicit export error. Desktop artifacts, drawings, screenshots and standalone checks are under `artifacts/symbols-qa-20260920/`. The exported PNG was visually inspected; the orbital phase and atom mark match the canvas.
