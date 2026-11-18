# Editable graphics

Shapes, brackets, lines, curves, pen paths and arcs are editable drawing
objects. They can sit behind or in front of the chemistry and keep their exact
geometry in `.rsk` files.

## Use it

### Draw a shape

1. Choose rectangles, ellipses, brackets, graphic lines, Bézier curves or arcs
   in the main palette. The shape selector in the context row also offers
   rounded rectangles, parentheses and braces. Arcs offer 90°, 120°, 180°, 270°
   and 360° sweeps.
2. Drag to size the object. Shift makes a square or circle, or keeps a line or
   curve at 45° steps. Escape cancels.
3. Set stroke color, fill color, line width and solid, dashed, dotted or wavy
   strokes in **Properties**. Number and hex fields apply when you press Enter.
   A rounded rectangle also has **Corner (pt)**; leave it empty to round 18% of
   the shorter side.

### Shade or shadow a shape

Select a rectangle, rounded rectangle, ellipse or closed path. In
**Properties**, below the fill color:

- **Shaded** fills the shape from white at the upper left to its fill color at
  the far edge, as ChemDraw's Shaded style does. The highlight stays light on
  the dark canvas too, so a shaded ball still looks like a ball. Rectangles shade toward their
  right and bottom edges; ellipses and paths shade around a highlight. A shape
  without a fill takes its stroke color.
- **Shadow** adds a grey drop shadow 4 pt below and to the right. A tilted
  rectangle or ellipse casts it along its own axes, as ChemDraw does. The
  inside of a shadowed outline is painted white, so the shadow shows only at
  the edge.

Each change is one Undo step and applies to every selected shape. With nothing
selected, the settings apply to the next shape you draw. Shading and shadows
are drawn on the canvas and in PNG, SVG and PDF. Clicks and snapping use the
shape itself, not its shadow.

### Wavy lines

Choose **Wavy** as the stroke pattern of a line, curve or path, or of an arrow
in its **Properties**. The wave is a row of half circles, about 3.6 pt long
for lines up to 1 pt wide, and longer for bolder lines. A wavy line runs
straight under each arrowhead and into it, and the head stays solid.

Select a graphic to move, copy, duplicate, rotate, reflect or resize it.
**Send to back** and **Bring to front** place it behind or above the chemistry.

### Draw a pen path

1. Choose **Pen path** and drag the first segment.
2. Click to add a straight segment, or drag to add a curved one. The drag sets
   the new node's direction.
3. Click the first node to close the path, or choose **Finish**. **New path**
   starts a separate object.

**New path** and **Finish** show while the **Pen path** tool is active. Each
segment is one Undo step; Escape cancels the segment you are drawing.

Select a graphic line, open pen path or Bézier curve and use **Properties → ARROWHEADS**
to set **End**, **Start**, shape and size. Number fields apply on Enter.
The context row's **Start head** and **End head** add a full head; **No start
head** and **No end head** remove it. Each change applies to all selected open
paths and curves in one Undo step. Heads follow the end directions, and the
line stops under each head. These paths remain free drawing objects and do
not attach to atoms.
On a multi-segment path, head sizing uses the whole path when an end segment
is too short.

Select pen paths, Bézier curves or curved arrows and choose **Smooth** or
**Symmetrize** from the canvas menu or command palette. Both commands apply to
the whole selection in one Undo step. They are also buttons in the
inspector's Pen path panel (as **Smooth path** and **Symmetrize**) and the
curved arrow panel.

**Smooth** aligns cubic handles at joints bent less than 20°, following the
straight segment at mixed joints and preserving intended corners. It removes
redundant nodes within a quarter of the line width of the original path.
It runs only when commanded. For curved arrows it evens the
angle and length of a lopsided bow when the end angles differ by less than 20°. On arrows with
nodes ([Add nodes to an arrow](arrows.md#add-nodes-to-an-arrow)) it applies
the path rules to the interior nodes; the ends stay in place.

**Symmetrize** finds the best mirror axis, including tilted axes, and averages
both sides. Hold Shift when clicking it to use a vertical or horizontal axis;
the palette also has **Symmetrize upright**. Shapes too far from symmetric are
left unchanged with a reason in the status bar. Curved arrows, including
arrows with nodes, use the perpendicular bisector of their endpoints; Shift has
no effect on them. Arrow
endpoints, attachments and arrowheads stay in place.

### Edit points

1. Select a curve or path and choose **Edit curve points**.
2. Drag a node to move it with its direction controls, or drag a square
   control to change a direction. **Corner** nodes (diamonds) have independent
   directions. **Smooth** nodes (circles) keep directions in line, with separate
   lengths. **Symmetric** nodes (circles with a dot) keep equal, opposite
   directions. These constraints apply when both adjacent segments are curved.
   Dragged pen nodes start Symmetric; clicked nodes start Corner.
3. In **Properties**, choose the node under **Selected node**. **Next segment**
   switches the following segment between **Straight** and **Curved**.
   **Insert after** and **Delete node** add and remove nodes. **Close path** or
   **Open path** closes or opens the path, and **Continue drawing** extends an
   open path.
4. Double-click a node to cycle Corner → Smooth → Symmetric → Corner, or use
   **Corner**, **Smooth** and **Symmetric** in the context row for the selected
   node (including the owner of a selected direction control). Switching to
   Smooth or Symmetric aligns the directions. Option/Alt-drag a direction
   control to make its node Corner and move that direction independently.
   Each type change or drag is one Undo step.
5. Choose **Done** in the context row to finish.

### Put a figure on a panel

1. Select what the panel should hold, such as a scheme and its captions.
2. Choose **Wrap in panel** from the canvas menu, the command palette or
   **Properties → Grouping & frames**. A tinted, rounded panel appears behind
   the selection with 10 pt of padding, grouped with it so they move together.
   Option/Alt-click edits one object inside.
3. In **Properties → Panel**, type a title and press Enter. **Title bar**
   puts it on a band in the outline color; **In border** sets it into the top
   edge. A title band grows the panel upward, so it never covers the contents.
4. **Split columns** and **Split rows** add a divider in the widest gap
   between the contents. Each column between vertical dividers takes its own
   title. Type a position in percent to move a divider; **×** removes it.
5. **Rounded** or **Square** sets the corners (a rounded panel's radius is
   under **Graphic style**) and **Outline** turns the border on or off. Fill and outline colors and dashes are under **Graphic
   style**. **Fit to contents** refits the panel around what it is grouped with.

Each change is one Undo step. Panels are artwork: reaction detection, atom
mapping and analysis never read them.

### Keep rows, columns and grids

1. Select two or more molecules, groups or reactions.
2. Choose **Lasting layout…** in **Properties → Grouping & frames**, or
   **Row with even gutters**, **Column with even gutters** or **Grid with even
   gutters** in the **Distribute** menu. A whole reaction (its arrow,
   participants and conditions) moves as one item.
3. **Properties → Layout** switches between **Row**, **Column** and **Grid**,
   and sets **Gutter (pt)**, **Columns** and **Start**, **Center** or **End**
   alignment. Switching keeps the reading order.

Items only move: bond lengths, labels and reaction roles stay as drawn. When an
item changes size, for example after adding a bond, its neighbors move to keep
the gutters. Moving one item by itself leaves it where you put it.
**Ungroup** ends the layout.

### Start from a figure layout

**Templates → Figure layouts** has a single-row TOC with a title bar, previous
work beside this work, a scheme beside an evidence panel, and two matched
scheme rows. Their benzene rings are placeholders to draw over or replace. To
see a TOC graphic at its printed size, choose **ACS TOC graphic · 8.25 × 4.45
cm**, **RSC graphical abstract · 8 × 4 cm** or **Elsevier graphical abstract ·
13 × 5 cm** in [Page setup](publication-pages.md).

## Limits

- Node editing and continued drawing work on one continuous path. Imported
  compound paths keep a simpler point editor.
- Closed and compound paths draw no arrowheads. Closing a path keeps its head
  settings, which reappear when you open it.
- CDXML does not keep the shape type. External saves may split or reorder
  objects and round colors and coordinates.
- A single-segment curve with solid heads returns from CDXML as a curved
  arrow.
- Dotted strokes cannot be exported to CDXML.
- ChemDraw draws waves only on straight lines without arrowheads. Other wavy
  lines, curves and arrows open in ChemDraw as solid strokes.
- ChemDraw has shadows only on rectangles and ovals, so a shadow on a rotated
  rectangle or a path is not written to CDXML or CDX.
- Shading paints each part of a compound path on its own, so a hole in it is
  shaded over. A path too detailed to shade keeps a flat fill.
- Freehand tracing and non-proportional resizing are not available.
- ChemDraw has no panel object. CDXML and CDX get a group of filled curves,
  lines and text; opening it again gives those shapes, not a panel. A panel's
  shadow or shading is not written there. Layouts are not written to ChemDraw
  files.
- A panel's title uses the drawing's font and size in bold. Title text is
  edited in **Properties**, not on the canvas.
- Rotating a whole layout changes its items' sizes, so the next size change
  lays them out upright again.
- Brackets are graphics. To define a repeating unit, see
  [generic structures](generic-structures.md).

## Implementation notes

Single-shape and orbital tools return to Select after committing. Chemical
symbol tools keep placing, attached or free, until Escape or another tool. Stroke swatches are the
palette's Ink and Strong row, and fill swatches its Tint row, where the
crossed-out swatch removes the fill; RGB hex entry is available. Brackets offer
both, left or right sides. Background graphics leave molecular atoms and bonds
selectable.

Pen paths: click the first node after three or more nodes to close. Closing
adds a straight segment from the last node to the first; opening removes that
closing segment. Choose the last node and **Curved** to reshape the closing
edge. Inserting a cubic node exactly splits the segment without reshaping it;
deletion joins its neighbors while keeping their outer tangent controls.
Undo/Redo keeps the selected object. Imported compound paths keep their generic
point editor and geometry, while node insertion/deletion and pen continuation
require a single connected path. Pen paths store an optional `node_types` entry
for each node. Older drawings detect types from their direction controls until
the path is next edited; saved types survive reopening, copying and pasting.

Document format version 4 adds graphics with an affine coordinate frame, so
transformations preserve editable shapes. Versions 1–3 still open with no
graphics by default. Cleanup and analysis preserve graphic objects. Undo of
graphic or typography changes keeps the current chemistry analysis. The tool
palette scrolls when the window is short. The scene used for canvas preview
also drives SVG, PDF and PNG export.

Panels are rectangles or rounded rectangles with a `panel` entry: titles,
title place, outline and dividers at fractions of the width or height. A lasting
layout is a `layout` entry on a group: flow, gutter and alignment. Its items
are the group's connected molecules, nested groups and whole reactions. After
each edit the layout is laid out again if an item changed size or number.
`graphics::panel::contents` lists what lies wholly inside a panel. Both entries
need document version 40.

### Interchange boundary

CDXML exports solid/dashed graphics as editable Bézier paths. Separate fill and
stroke colors use grouped paths. It imports supported plain curves, curve-only
groups, basic rectangles/lines/bracket pairs, and ovals with explicit axes.

Shaded or shadowed rectangles, rounded rectangles and ellipses are written as
ChemDraw's own Rectangle and Oval graphics with their Shaded, Shadow or
Shadowed flags and ShadowSize (hundredths of a point), so ChemDraw can restyle
them. Upright rectangles only: a rotated one stays a path. A shaded path is a
ChemDraw shaded curve (CurveType 256, FillType Shaded). A wavy line without
heads is ChemDraw's line graphic with LineType Wavy, and a straight wavy arrow
keeps LineType Wavy. On import, ChemDraw's Shaded paints the shape's one color
and Shadow keeps its ShadowSize. ChemDraw shades an open curve as if closed,
and draws a wavy curve or a wavy line with heads straight; ReShiki imports them
the same way. ReShiki draws the shading as flat bands, as ChemDraw's own
renders do, three bands per point of size (32 to 256), so canvas, PNG, SVG
and PDF match and the steps stay invisible when zoomed in. Drawings with shading,
shadows, wavy strokes, arrowheads on graphic lines or a set corner radius are
document version 40. ChemDraw's CornerRadius (hundredths of a point) becomes
**Corner (pt)** on import and is written back for native rounded rectangles.
Curves nested in molecular fragments are also kept on import.

CDXML does not keep ReShiki's parametric shape type. External saves may split
grouped bracket strokes, change stacking values and quantize colors/coordinates.
A subsequent import keeps the supported visible paths but may have a different
number of graphic objects. `.rsk` files keep ReShiki's exact model.

`.rsk`/SVG/PDF/PNG keep dots. SVG/PDF/EMF/PNG draw path arrowheads, and
CDXML/CDX keep their editable head settings on open curves.
Unsupported legacy shapes, doubled curves, transparency and arbitrary external
styling are not implemented. A bracket becomes a polymer
repeating unit, with a movable repeat label, through **Define repeating unit**;
see [generic structures](generic-structures.md#define-a-repeating-unit).

Format references: the
[published CDXML DTD](https://static.chemistry.revvitycloud.com/cdxml/CDXML.dtd),
and the published format documentation for
[curves](https://bobhanson.github.io/IUPAC-FAIRSpec/cdx_sdk/Curve.htm),
[graphics](https://bobhanson.github.io/IUPAC-FAIRSpec/cdx_sdk/Graphic.htm) and
[curve flags](https://bobhanson.github.io/IUPAC-FAIRSpec/cdx_sdk/properties/Curve_Type.htm).

Mixed-object and nested groups were added later; see
[selection and groups](selection-and-groups.md) for membership, integral groups,
fitted frames and remaining exchange limits. Version 5 extends the format with
this group model.

### Records

The [original graphics checks](changes/editable-graphics-20260920.md) record
the first build's desktop, file-exchange and test runs.
