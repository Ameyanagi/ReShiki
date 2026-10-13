# Chemical symbols and orbitals

Chemical symbols add charges, radicals, lone pairs and other marks to atoms or
to the page. Orbitals are editable drawing shapes that can snap to an atom.

## Use it

### Add a mark to an atom

1. Open the **Chemical symbols** palette button and choose a symbol. It offers
   18 choices: circled and plain charges, radicals and diradicals, lone pairs
   and bars, radical ions, H-dot and H-dash, attachment diamond, star, wave and
   bead, and single and double daggers.
2. With **Attach to atoms** on, click an atom to add the mark. Drag from the
   atom to place the mark at a chosen offset.
3. To adjust marks later, select the atom and choose **Position atom marks**.
   Drag a mark's handle to move it. **Rotate** turns a mark by 45°, and
   **Remove** deletes it.

Charges and radicals change the atom's chemistry, and ReShiki refreshes its
hydrogen label and unpaired electrons. Lone pairs and bars are annotations
only. Marks move, copy and transform with their atom, take the atom label's
color, and each edit is one Undo step.

### Draw an orbital

1. Open the **Orbitals** palette button and choose s, sigma, single lobe, p,
   sp³ hybrid, dxy or dz².
2. Drag from the node to set size and direction; Shift keeps the direction at
   15° steps. A click uses one default bond length.
3. Choose outline, solid or flat-gray phases in **Properties**. Multi-lobe
   orbitals can reverse their phases.

**Snap to atoms** in the context row snaps the orbital's node to the nearest
atom. A green ring marks the chosen atom. Hold Option/Alt to place an orbital
freely. An orbital stays a drawing object; group it with the molecule when they
should move together. Orbitals leave a small gap around atom labels so the
labels stay readable.

**Snap to atoms**, the gap around labels and the closer placement of new lone
pairs are available in Nightly builds; not in ReShiki 0.11.0.

## Limits

- Lone pairs do not change valence, electron counts or orbital occupancy.
- H-dot, H-dash and attachment markers are free symbols. They do not define
  hydrogen stereo, R-groups, polymers or reaction attachment points. Draw an
  explicit H atom and a stereo bond for hydrogen stereochemistry.
- Very crowded drawings may still need lone pairs placed by hand.
- Gray fill is flat, not gradient shading. Gradient-shaded orbitals cannot be
  imported.
- Some marks need a `.rsk` file or SVG, PDF or PNG; CDXML export rejects them.
  See [Persistence and exchange](#persistence-and-exchange).

## Implementation notes

JACS / ACS remains the default: Arial 10 pt, black, 14.4 pt bonds and 0.6 pt
lines. New restores those settings even after changing tool styles. Properties
previews the actual styled geometry. Free symbols are ordinary selectable
graphics. Charge changes release imported implicit-label hydrogen counts so
ammonium can become ammonia again. Removing a mark clears its represented
charge or radical state.

Click-created lone pairs and bars sit just outside the visible label ink, with
a modest gap scaled to its font size. New placements try the top, left, bottom
and right sides, avoiding nearby label ink, bonds and existing atom marks.
Pairs beside the label turn vertically. Dragging from an atom keeps the exact
requested offset; existing saved marks do not move when labels or styles change.

**Snap to atoms** controls orbital node snapping independently of
chemical-symbol attachment. The choice stays selected when switching tools in
the current document; New restores snapping. Option/Alt places freely inside
the snap radius, and the drag preview names the snapped target or free
placement.

Orbital fills and outlines leave a transparent gap around visible atom-label
ink, including hydrogen, isotope and charge labels. This applies to orbitals
both behind and in front of molecular content. Clicking these visible labels
selects their atom through a foreground orbital. The atom positions, orbital
node, axes and layer remain exactly as placed. The gap follows subsequent label
edits; it is rendering clearance rather than a change to orbital size or
attachment. Overlap with bonds, text captions and other annotations still
follows ordinary object layering.

### Persistence and exchange

- Document format 8 keeps orbital parameters, phases and atom-owned marks;
  earlier document versions still open.
- Canvas, SVG, PDF and PNG use shared vector geometry, including differently
  filled orbital lobes and rotated marks.
- SMILES/MOL/CDXML keep supported one/two-unpaired-electron chemistry;
  molecular formats omit drawing annotations.
- Free symbols and orbitals export to CDXML/CDX as grouped editable vector
  curves, including orbital label-clearance gaps. Importing those vectors keeps
  their appearance, but not their parametric orbital/symbol controls or dynamic
  clearance after later label edits.
- ReShiki's external-curve importer currently applies a minimum 0.1 pt outline
  to fill-only curves. Orbital clearance includes padding that keeps these
  re-imported pieces clear of label ink; this can expand a filled vector edge
  by up to 0.05 pt.
- CDXML plain/solid orbital and supported free-symbol objects import as
  editable ReShiki objects. Gradient-shaded orbitals are rejected explicitly.
- CDXML atom-owned single charges, radical dots and supported lone pairs use
  `represent` ownership links. Carbon lone-pair annotation export is rejected
  because an external drawing application interprets it as diradical
  chemistry. Positioned multiple-charge marks, combined radical-ion marks, bars
  and colored attached marks require `.rsk`/SVG/PDF/PNG; unsupported per-symbol
  CDXML overrides are rejected. External saves can discard ownership of distant
  marks, so export requires marks within 1.05 label sizes of their owner and
  closer to that atom than any other atom. A visible atom label and hydrogen
  count are written explicitly.
- Template attachment rejects marked or radical anchor atoms; free insertion of
  these templates keeps their full drawing.

This is a bounded drawing/chemistry subset, not complete chemical-symbol or
orbital parity. Arbitrary mark colors/sizes in the inspector, gradient shading,
orbital fixed-length constraints and 3D orbital semantics remain incomplete.

### Records

The [original symbol checks](changes/scientific-symbols-20260920.md) record the
first build's fixtures, desktop checks and tests.
