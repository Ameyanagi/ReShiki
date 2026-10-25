# Chemical symbols and orbitals

Chemical symbols add charges, radicals, lone pairs and other marks to atoms or
to the page. Orbitals are editable drawing shapes that can snap to an atom.

## Use it

### Add a mark to an atom

1. Open the **Chemical symbols** palette button and choose a symbol. It offers
   18 choices: circled and plain charges, radicals and diradicals, lone pairs
   and bars, radical ions, H-dot and H-dash, attachment diamond, star, wave and
   bead, and single and double daggers.
2. With **Attach to atoms** on, click an atom to add the mark. The symbol tool
   stays active for more placements; press Escape or choose another tool to stop. Drag from the
   atom to choose its side: the mark orbits the atom in 15° steps, just
   outside the label with the same gap a click uses. Hold Option/Alt or Shift
   to place it at any offset.
3. To adjust marks later, select the atom and choose **Position atom marks**.
   Drag a mark's handle to move it around the same orbit; Option/Alt or Shift
   frees it. While dragging, a dashed line shows the orbit and dots show the
   15° positions it can take. **Rotate** turns a mark by 45°, and **Remove**
   deletes it.

A lone pair or bar always turns across the line from the atom, so its two
dots sit side by side around the label, also when placed freely. Charges and
radicals stay upright.

Charges and radicals change the atom's chemistry, and ReShiki refreshes its
hydrogen label and unpaired electrons. Lone pairs and bars are annotations
only. Marks move, copy and transform with their atom, take the atom label's
color, and each edit is one Undo step. Rotating or flipping a molecule turns
its lone pairs with it; charges stay upright.

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
should move together. Orbital lobes are always drawn whole. An atom label that
a lobe reaches is drawn on top of the orbital inside a thin halo of the paper
color, so the label stays readable over filled lobes.

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
Pairs beside the label turn vertically. Click-created charges and radicals
keep their usual side when it is clear; otherwise they try the nearest 15°
steps, avoiding bond ink, nearby labels and existing marks after the changed
charge's hydrogens are refreshed. Saved marks keep their exact offsets. A dragged mark's orbit follows the
label at the same gap a click uses, measured from the label's outline: each
glyph counts as a rounded rectangle fitted to its outline (O is nearly round,
while N and H keep the corners their stems reach), and the glyphs together
form one convex shape, so OH reads as one rounded label with its hydrogen.
The mark counts as one shape too, so a lone pair keeps the gap like the box a
click measures. The gap therefore stays even all the way round, and straight
up is exactly where a click puts a pair. Steps where the mark would touch one
of the atom's bonds are skipped: the drag lands on the nearest clear step, and
the guide leaves those steps out. Other marks are not avoided, so dragging one
mark onto another is allowed. A free (Option/Alt or Shift) drag keeps the
exact requested offset, bonds included; existing saved marks do not move when
labels or styles change.

**Snap to atoms** controls orbital node snapping independently of
chemical-symbol attachment. The choice stays selected when switching tools in
the current document; New restores snapping. Option/Alt places freely inside
the snap radius, and the drag preview names the snapped target or free
placement.

Orbital fills and outlines are never cut around atom labels. When a lobe's
fill or outline comes within 1 pt of visible atom-label ink (including
hydrogen, isotope and charge labels), that atom's label and its attached marks
are drawn above the orbital inside a halo in the paper color. The halo follows
the glyphs: each glyph is filled and stroked 1 pt wider, and the middle of each
glyph's ink box is covered too, so the inside of an O stays clear. The rule is
the same for the orbital's own atom and for neighboring atoms, and for
orbitals behind molecular content (the default) or in front of it. A label
reached only by orbitals behind the molecule keeps its usual place above the
bonds. Otherwise each reached label is drawn right above the topmost front
orbital that reaches it, so graphics stacked above that orbital still cover
it, and a click on the label selects what is drawn on top. Clicking these visible labels selects
their atom through a foreground orbital. The atom positions, orbital node,
axes and layer remain exactly as placed, and the halo follows later label
edits. Bonds, text captions and other annotations keep their ordinary
layering against orbitals.

The canvas and exports with a background (SVG, PDF, PNG and EMF files) paint
the halo in the canvas background color. Transparent output, such as copied
SVG, PDF and PNG, paints no paper: the halo shape is knocked out of everything
drawn beneath the label (an SVG mask, a soft mask in PDF), so whatever lies
behind the pasted drawing shows through and the surround stays transparent.
EMF has no soft masks, so a transparent EMF paints the halo in the canvas
background color instead. SVG readers that ignore masks show the label over
the whole lobe without a halo.

### Persistence and exchange

- Document format 8 keeps orbital parameters, phases and atom-owned marks;
  earlier document versions still open.
- Canvas, SVG, PDF and PNG use shared vector geometry, including differently
  filled orbital lobes and rotated marks.
- SMILES/MOL/CDXML keep supported one/two-unpaired-electron chemistry;
  molecular formats omit drawing annotations.
- Free symbols and orbitals export to CDXML/CDX as grouped editable vector
  curves with whole lobes. A front orbital that reaches an atom label is
  stacked just behind the molecule, so the reading application draws the
  labels (and, unlike the canvas, the bonds) over it, and a re-import places
  the curves behind the molecule too. No halo shapes are written: they would
  become stray editable objects. Importing those vectors keeps their
  appearance, but not their parametric orbital/symbol controls or the halo.
- CDXML plain/solid orbital and supported free-symbol objects import as
  editable ReShiki objects. Gradient-shaded orbitals are rejected explicitly.
- CDXML atom-owned single charges, radical dots and supported lone pairs use
  `represent` ownership links. Carbon lone-pair annotation export is rejected
  because an external drawing application interprets it as diradical
  chemistry. Positioned multiple-charge marks, combined radical-ion marks, bars
  and colored attached marks require `.rsk`/SVG/PDF/PNG; unsupported per-symbol
  CDXML overrides are rejected. External saves can discard ownership of distant
  marks, so export requires marks within 1.05 label sizes of their owner's
  label ink (of the atom itself when it has no label), so orbited marks beside
  hydrogens still export, and closer to that atom than any other atom. A visible atom label and hydrogen
  count are written explicitly.
- Template attachment rejects marked or radical anchor atoms; free insertion of
  these templates keeps their full drawing.

This is a bounded drawing/chemistry subset, not complete chemical-symbol or
orbital parity. Arbitrary mark colors/sizes in the inspector, gradient shading,
orbital fixed-length constraints and 3D orbital semantics remain incomplete.

### Records

The [original symbol checks](changes/scientific-symbols-20260920.md) record the
first build's fixtures, desktop checks and tests.
