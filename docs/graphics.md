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
3. Set stroke color, fill color, line width and solid, dashed or dotted strokes
   in **Properties**. Number and hex fields apply when you press Enter.

Select a graphic to move, copy, duplicate, rotate, reflect or resize it.
**Send to back** and **Bring to front** place it behind or above the chemistry.

### Draw a pen path

Available in Nightly builds; not in ReShiki 0.11.0.

1. Choose **Pen path** and drag the first segment.
2. Click to add a straight segment, or drag to add a curved one. The drag sets
   the new node's direction.
3. Click the first node to close the path, or choose **Finish**. **New path**
   starts a separate object.

**New path** and **Finish** show while the **Pen path** tool is active. Each
segment is one Undo step; Escape cancels the segment you are drawing.

### Edit points

1. Select a curve or path and choose **Edit curve points**.
2. Drag a round node to move it with its direction controls, or drag a square
   control to change one direction.
3. In **Properties**, choose the node under **Selected node**. **Next segment**
   switches the following segment between **Straight** and **Curved**.
   **Insert after** and **Delete node** add and remove nodes. **Close path** or
   **Open path** closes or opens the path, and **Continue drawing** extends an
   open path.
4. Choose **Done** in the context row to finish.

## Limits

- Node editing and continued drawing work on one continuous path. Imported
  compound paths keep a simpler point editor.
- CDXML does not keep the shape type. External saves may split or reorder
  objects and round colors and coordinates.
- Dotted strokes cannot be exported to CDXML.
- Freehand tracing, non-proportional resizing and per-shape corner radii are
  not available.
- Brackets are graphics. To define a repeating unit, see
  [generic structures](generic-structures.md).

## Implementation notes

Single-shape tools return to Select after committing. Stroke swatches are the
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
require a single connected path. Pen paths reuse the existing path commands and
need no additional serialized fields.

Document format version 4 adds graphics with an affine coordinate frame, so
transformations preserve editable shapes. Versions 1–3 still open with no
graphics by default. Cleanup and analysis preserve graphic objects. Undo of
graphic or typography changes keeps the current chemistry analysis. The tool
palette scrolls when the window is short. The scene used for canvas preview
also drives SVG, PDF and PNG export.

### Interchange boundary

CDXML exports solid/dashed graphics as editable Bézier paths. Separate fill and
stroke colors use grouped paths. It imports supported plain curves, curve-only
groups, basic rectangles/lines/bracket pairs, and ovals with explicit axes.
Curves nested in molecular fragments are also kept on import.

CDXML does not keep ReShiki's parametric shape type. External saves may split
grouped bracket strokes, change stacking values and quantize colors/coordinates.
A subsequent import keeps the supported visible paths but may have a different
number of graphic objects. `.rsk` files keep ReShiki's exact model.

`.rsk`/SVG/PDF/PNG keep dots. Chemical polymer semantics, unsupported legacy
shapes, arrowed/doubled/shaded curves, transparency, shadows and arbitrary
external styling are not implemented.

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
