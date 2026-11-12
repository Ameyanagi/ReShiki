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

The palette explains each preset when you hover over it:

- **Forward** shows a net reaction.
- **Equilibrium** (⇌) shows a reversible reaction or interconversion of tautomers.
- **Resonance** (↔) links contributing structures of one species, not an equilibrium.
- **Retrosynthesis** (⇒) means the target is made from the indicated precursors,
  following Corey's retrosynthetic arrow convention.
- **Curved / electron pair** shows an electron pair moving.
- **Bent / elbow** is a reaction arrow with a corner.
- **Single electron** shows one electron moving, including homolysis and radical steps.
- **Dipole** marks a bond dipole.
- **No reaction** means the reaction does not take place, following ChemDraw's convention.

Bold, dashed, hollow, angled and half-arrow variants have the meaning of a
Forward arrow; unequal equilibrium has the meaning of an Equilibrium arrow.
Help also finds these meanings by preset name, such as “resonance” or
“retrosynthesis”.

Only Forward, Equilibrium and Bent / elbow arrows accept new reaction roles.
The **Reaction roles** list shows each arrow's kind and explains when roles
cannot be assigned. Older files retain existing roles on other arrow kinds and
show a warning when opened.

### Bend and aim an arrow

1. Select the arrow with **Select** or the arrow tool.
2. Drag a round end handle to move that end. Drag the diamond to bend the arrow.
3. On a curved arrow, drag a square handle to change how that end leaves or
   arrives. The endpoints stay attached; an attached head may move a little
   back or sideways to keep it clear of bond ink.

Each drag is one Undo step; Escape cancels it. In **Properties**, **Reverse**
changes the direction, **Flip bend** mirrors the curve and **Straighten**
removes the bend. **Reset arrow style** restores the preset's look.

### Add nodes to an arrow

An arrow can follow a path with nodes in the middle, such as an S-shaped
mechanism arrow from a lone pair to a distant carbonyl.

1. Select one arrow and choose **Edit points** in **Properties** (or in the
   canvas menu).
2. Click the arrow's shaft to add a node there. The curve keeps its shape, and
   the new node is selected.
3. Drag a node to move it with its direction controls, or drag a square
   control to change a direction. Nodes are **Corner** (diamond),
   **Smooth** (circle) or **Symmetric** (circle with a dot), as on pen paths.
   Double-click a node to cycle its type, or use **Corner**, **Smooth** and
   **Symmetric** in the context row. Option/Alt-drag a direction control to
   make its node a Corner.
4. **Delete node** in **Properties** removes the selected node. Choose
   **Done** in the context row, or press Escape, to finish.

Each added node, drag, type change and deletion is one Undo step. The ends,
arrowheads, kind and style stay the same, and attached ends stay attached when
the molecules move: only the first and last segments follow them. Like a bend,
a node releases condition text as ordinary text. A no-reaction mark and the
shorter shaft of an unequal equilibrium are placed by length along the path. **Smooth** aligns gentle joints and removes redundant nodes;
**Symmetrise** mirrors the arrow across the perpendicular bisector of its ends.
**Straighten** removes every node, and **Flip bend** mirrors the nodes too.
Bent / elbow arrows keep their single corner and do not take nodes.

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

### Write conditions on an arrow

1. Choose **Text** (`T`) and click above or below a straight arrow.
2. Type the conditions and apply them. The text attaches to the arrow, centred
   on that side.
3. Text you drag onto an arrow's upper or lower side attaches the same way.

The arrow grows to fit its widest label, with padding at each end. An arrow
fitted to its text follows when you edit the text, and shrinks back to its
shortest length when the text shrinks. An arrow you lengthened by hand keeps
its length unless the text outgrows it. Dragging an end handle can lengthen the
arrow freely; shortening it stops at the label width. Moving, rotating or
reversing the arrow carries its labels.

Each side holds one label; use line breaks for several lines of conditions.
Drag the text across the arrow to move it to the other side. Dragging
perpendicular to the shaft adjusts its gap; dragging sideways beyond the arrow
span detaches it. Vertical arrows take labels on their left and right.
Copy, Cut and **Delete** of an arrow include its condition text; copying or
deleting the text alone leaves the arrow. The eraser removes only what it
touches.

### Adjust label spacing

Select an attached condition label. In **Properties → ARROW LABEL SPACING**,
enter a nonnegative **Arrow gap** and press Enter. The field uses the current
ruler unit: mm, cm, in or pt. Zero is allowed. The gap is the clearance between
the arrow ink and the text box, measured perpendicular to the shaft, including
on diagonal arrows.

Drag the label perpendicular to the shaft to adjust the gap while keeping it
attached and centred. Drag sideways beyond the arrow span, or choose **Detach**,
to make it ordinary text. **Auto** restores the default spacing. Select several
attached labels to apply one gap to all of them in one Undo step; selecting only
the arrow does not show these label controls.

Magenta dimensions show the actual gap of selected labels, including the final
position during a drag. They are editing guides and do not appear in SVG, PDF or
PNG exports. See the [native examples and saved drawing](changes/arrow-label-spacing.md).

Custom spacing is stored in native `.rsk` format version 31. Drawings using only
**Auto** continue to save as version 30. Older builds that cannot read the file's
version report that an update is required.

### Style an arrow

**Properties** shows a preview and sets each end's head (full, half or none),
head shape, color, line style and width, head length, half-width and notch.
Equilibrium arrows also set **Reverse length (0.2–1)** and **Shaft separation
(pt)**. Number and color fields apply when you press Enter.

## Limits

- Arrows do not check electron counts, map atoms or generate mechanisms.
- **Check structure** adds informational hints for a resonance or equilibrium arrow with one complete molecule at each end: a resonance arrow between structures whose atoms or H differ suggests ⇌ for tautomers, and an equilibrium arrow between structures that differ only in π bonds or charge placement suggests ↔. Abbreviations, partial structures and multi-molecule sides are skipped.
- Only `.rsk` files keep the links between an arrow and its targets. Other
  formats keep the arrow's shape.
- Circular or elliptical arcs, broad hollow arrow bodies and automatic routing
  are not available.
- A multi-segment arrowed curve read from CDXML/CDX opens as a pen path with
  arrowheads, not as an arrow.
- Some arrow styles cannot be exported to CDXML and give an export error; see
  [CDXML exchange](#cdxml-exchange).
- Only straight arrows without target attachments carry condition text.
  Bending an arrow or attaching its ends releases its labels as ordinary
  text. Text that was already beside an arrow attaches only when you move it;
  editing it in place leaves it unattached.
- Only `.rsk` files keep the link between an arrow and its condition text.
  CDXML/CDX, SVG, PDF and PNG keep the text and the arrow where they are.

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

### Condition labels

A document lists each label as an arrow ID, a text ID and a side (`above` or
`below`); document version 27 added the list. An optional gap, stored in world
units, requires version 31. The text stays an ordinary caption. Every edit
re-checks the labels in the same Undo step:

1. Text dragged on its own re-centres in its slot. Perpendicular movement
   changes the saved gap or side; sideways movement beyond the arrow span
   detaches it.
2. New or moved text attaches when its centre lies within the arrow span (or
   the text spans the arrow midpoint) and it is no more than one bond length
   beyond the attached position. The nearest free side wins. Arrows that the
   same edit created or moved, such as pasted schemes, do not capture text.
3. The arrow is lengthened to at least the widest label plus 0.2 bond lengths
   at each end. A dragged end handle keeps the other end fixed; otherwise the
   midpoint stays. An arrow fitted to its text follows text edits to
   `max(label width, 2.8 bond lengths) + 0.4 bond lengths`, the spacing that
   assistant schemes use.
4. **Auto** places labels 0.45 bond lengths beyond the arrow ink, including
   heads and equilibrium shafts. A custom gap overrides that default and stays
   with the label when the arrow or its style changes.

Copying or duplicating an arrow copies its labels, and **Delete** or Cut
removes them with it; copying the text alone gives ordinary text. Dragging one
end onto the other keeps the arrow's direction and stops at the label width. Molecule-name captions never attach. CDXML/CDX export
reports that the links are omitted. ChemDraw records similar links as
`ReactionStepObjectsAboveArrow`/`BelowArrow` in reaction steps; ReShiki does not
yet read or write them.

### Geometry and persistence

`.rsk` files keep arrow data from earlier versions, optional target references
and stable lone-pair IDs. An arrow keeps its endpoints, either an optional
quadratic control or two independent cubic controls, and appearance. Existing
free quadratic curves keep their exact saved and rendered shape until a
direction control is edited. Older ReShiki builds report that an update is
required instead of discarding the links. Interior arrow nodes are stored in
format version 35 as a list of points, controls and node types; arrows without
nodes save exactly as before, so their files keep their earlier version. Legacy curved arrows materialize
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

Supported exchange includes straight full/half-headed arrows, straight
retrosynthesis arrows with editable heads and style, head shape/dimensions/notches,
color, solid/dashed strokes, resonance, dipole/no-go marks, equal-length
equilibrium with half heads, single cubic or quadratic Bézier arrows with
solid heads, and arrows with interior nodes, which are written as one
multi-segment ChemDraw curve with the same heads. Cubic arrows keep both independently editable
controls; quadratic arrows use their exact equivalent cubic control points.
Mixed-object groups keep supported arrow children.

SVG/PDF/PNG and supported CDXML/CDX export the resolved attached curve and
report that editing attachment links are omitted. `.rsk` files keep those links.
This drawing metadata does not change molecular connectivity or electron counts.

The importer reads multi-segment arrowed splines as pen paths with arrowheads,
preserving head size even when an end segment is short. It rejects
circular/elliptical arrow arcs, unknown head/stroke types and unsupported decoration. The exporter
rejects dotted arrows, curved retrosynthesis arrows, bent/unequal equilibrium,
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
