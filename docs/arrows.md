# Reaction and mechanism arrows

Arrows are editable drawing objects for reactions and mechanisms. Curved arrows
can attach their ends to atoms, bonds and lone pairs and follow them when they
move. Arrows never change the chemistry of the drawing.

## Use it

### Draw an arrow

1. Choose **Arrow** (`A`). Hold the tool or click its lower-right triangle to
   choose another preset.
2. Click empty space to place a fixed-length arrow pointing right, or drag to
   set its length and direction. Option/Alt allows any angle.
3. With an arrow tool active, click an existing arrow to give it that style.
   Repeated clicks cycle its direction, half-head side or equilibrium variant.

The nine presets are forward, equilibrium, resonance, retrosynthesis, electron
pair, bent/elbow, single electron, dipole and no reaction. The palette also
offers bold, dashed, hollow and unequal-equilibrium variants.

### Bend and aim an arrow

1. Select the arrow with **Select** or the arrow tool.
2. Drag a round end handle to move that end. Drag the diamond to bend the arrow.
3. On a curved arrow, drag a square handle to change how that end leaves or
   arrives. Both ends stay where they are.

Each drag is one Undo step; Escape cancels it. In **Properties**, **Reverse**
changes the direction, **Flip bend** mirrors the curve and **Straighten**
removes the bend. **Reset arrow style** restores the preset's look.

### Optional target attachments

1. Choose **Curved / electron pair** or **Single electron**. **Attach ends to
   atoms and bonds** in **Properties** is on by default.
2. Click the source: an atom, a bond or a lone pair. It is highlighted, and the
   drawing does not change yet.
3. Click the destination. ReShiki adds one curved arrow in one Undo step.

For a free arrow, Option/Alt-click, drag, or turn off **Attach ends to atoms
and bonds**. Escape cancels a picked source. An attached arrow follows its
targets when they move. **Detach start** and **Detach end** in **Properties**
release one end. Dragging an end also detaches it; dragging a square handle
keeps the link.

### Style an arrow

**Properties** shows a preview and sets each end's head (full, half or none),
head shape, color, line style and width, head length, half-width and notch.
Equilibrium arrows also set **Reverse length (0.2–1)** and **Shaft separation
(pt)**. Number and color fields apply when you press Enter.

## Limits

- Arrows do not check electron counts, map atoms or generate mechanisms.
- Only `.rsk` files keep the links between an arrow and its targets. Other
  formats keep the arrow's shape.
- Multi-segment arrow splines, circular or elliptical arcs, broad hollow arrow
  bodies and automatic routing are not available.
- Some arrow styles cannot be exported to CDXML and give an export error; see
  [CDXML exchange](#cdxml-exchange).

## Implementation notes

Every new document restores **JACS / ACS** defaults: Arial 10 pt, black drawing
colors, 14.4 pt bonds and 0.6 pt lines. Arrow overrides stay with the drawing.
Selecting a template or storing a mixed-object template preserves arrow bends
and styling. Arrow placement does not inherit the fixed bond length; the
fixed-angles setting snaps arrows to 15°. The live preview uses the same
geometry and style as the committed object. Cross/hash no-reaction marks and a
dipole cross are available. **Reverse** keeps the curve's shape. The diamond
lies on the curve, and moving a cubic endpoint carries its adjacent direction
control with it.

Attachment targets are a source atom, a visible bond or a positioned lone pair.
Moving a target carries its endpoint and adjacent control. Target deletion
detaches at the last resolved position, and Undo restores the reference. The
[attachment record](changes/mechanism-attachments.md) has the desktop
comparison, saved files and limits.

The assistant can lay out branching schemes with one shared reactant; this does
not add chemical electron-flow semantics. See the
[feature status](feature-status.md).

### Geometry and persistence

`.rsk` files keep arrow data from earlier versions, optional target references
and stable lone-pair IDs. An arrow keeps its endpoints, either an optional
quadratic control or two independent cubic controls, and appearance. Existing
free quadratic curves keep their exact saved and rendered shape until a
direction control is edited. Older ReShiki builds report that an update is
required instead of discarding the links. Legacy curved arrows materialize
their implicit bend before reflection, copying or transformation. Translation,
rotation, resizing, grouping and template placement carry the controls with the
endpoints. Stroke widths and head dimensions keep their publication point sizes
when the geometry is resized. Attached endpoints use final-ink clearance;
shrinking into fixed-size ink can require the bounded outward correction
documented in the attachment record.

The same paths draw the canvas, the placement preview, SVG, PDF and PNG. Hit
testing follows the curve and heads; selection bounds, lasso and alignment
include the visible arrow paths and stroke extent. Hollow full heads leave an
opening in the shaft instead of drawing through the hollow center.

### CDXML exchange

Supported exchange includes straight full/half-headed arrows, head
shape/dimensions/notches, color, solid/dashed strokes, resonance, dipole/no-go
marks, equal-length equilibrium with half heads, and single cubic or quadratic
Bézier arrows with solid heads. Cubic arrows keep both independently editable
controls; quadratic arrows use their exact equivalent cubic control points.
Mixed-object groups keep supported arrow children.

SVG/PDF/PNG and supported CDXML/CDX export the resolved attached curve and
report that editing attachment links are omitted. `.rsk` files keep those links.
This drawing metadata does not change molecular connectivity or electron counts.

The importer rejects circular/elliptical arrow arcs, multi-segment arrow
splines, unknown head/stroke types and unsupported decoration. The exporter
rejects dotted arrows, retrosynthesis arrows, bent/unequal equilibrium,
equilibrium with full/absent heads, dipoles with a tail head, and curved arrows
with hollow/angled heads or dipole/no-go decoration. Those combinations remain
editable and exportable in `.rsk`/SVG/PDF/PNG. Interchange checks found that
external saves can remove a dipole tail head or convert an angled Bézier head
to solid; those combinations therefore return an explicit export error. Head
dimensions are quantized to CDXML's hundredths of a point; external
applications can render strokes differently.

### Records

The [original arrow checks](changes/arrow-tools-20260920.md) record the first
build's fixtures, desktop checks and tests.
