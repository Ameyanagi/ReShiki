---
section: Changes
issues: []
---

- A ChemDraw file (CDX or CDXML) now opens even when it uses a drawing style
  or object ReShiki cannot draw yet. Shaded orbitals, doubled curves,
  see-through objects, block arrows, rotated captions and similar styles import as the
  nearest style, and one message lists what changed, such as "2 doubled curves
  as single lines; 1 rotated caption upright". BioDraw shapes, gel and TLC
  plates are left out and named. Before, one such object refused the whole
  file. See
  [ChemDraw styles ReShiki cannot draw yet](graphics.md#chemdraw-styles-reshiki-cannot-draw-yet).
- ChemDraw's arcs and arc arrows, drawn bracket pairs, daggers, the lone pair
  bar and its hybrid and filled orbitals now import as drawn.
- CDX files written by ChemDraw: no-go arrows keep their cross or hash, lone
  pair symbols open, and clip art with four-byte colours, unknown properties
  and repeated object identifiers no longer refuse the file. ReShiki now writes
  no-go marks, lone pair symbols and arc angles in CDX with ChemDraw's values.
  A CDX file that an earlier ReShiki version saved with a hash no-go mark or a
  lone pair symbol shows a cross or a lone pair bar.
