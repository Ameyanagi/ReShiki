# Editable graphics

Implemented and desktop-checked on 2026-09-20. Supported graphics workflows and interchange limits are listed below.

## Drawing and editing

- The main palette includes rectangles, ellipses, brackets, graphic lines, Bézier curves, pen paths and arcs. The contextual shape selector also offers rounded rectangles, parentheses and braces. Arc presets still offer 90°, 120°, 180°, 270° and 360° sweeps.
- Drag to preview and size an object. Shift constrains boxes/ellipses to a square/circle and lines/curves to 45° directions. Escape cancels. Single-shape tools return to Select after committing.
- Properties controls stroke color, separate fill color, line width in points, and solid/dashed/dotted strokes. Stroke swatches are the palette's Ink and Strong row, and fill swatches its Tint row, where the crossed-out swatch removes the fill; RGB hex entry is available, and numeric/hex fields apply with Enter. Brackets offer both, left or right sides.
- Select to move, copy, duplicate, rotate, reflect or proportionally resize. Background graphics leave molecular atoms and bonds selectable. Send to back / Bring to front places graphics behind or above the chemical drawing.
- Choose **Pen path** and drag its first segment. Keep drawing the same object by clicking for a line or dragging a new node to set its tangent. Click its first node after three or more nodes to close, or choose **Finish**. **New path** starts a separate object; **Continue drawing** extends a selected open path. Each completed segment commits one history step; Escape cancels the active gesture.
- Select a curve or path and choose **Edit curve points**. Round nodes carry their adjacent tangent controls when moved; square controls adjust each tangent independently. Properties lets you choose a node, insert after it, delete it, change its following segment between straight and curved, and open or close the path. Inserting a cubic node exactly splits the segment without reshaping it; deletion joins its neighbors while retaining their outer tangent controls. Undo/Redo retains the selected object.
- General node operations support one continuous subpath. Imported compound paths retain their generic point editor and native geometry, while node insertion/deletion and pen continuation require a single connected path.

Native format version 4 adds graphics with an affine coordinate frame, so transformations preserve editable shapes. Versions 1–3 still open with no graphics by default. Cleanup and analysis preserve graphic objects. Undo of graphic or typography changes keeps the current chemistry analysis. The tool palette scrolls when the window is short. The scene used for canvas preview also drives SVG, PDF and PNG export.

## Interchange boundary

CDXML exports solid/dashed graphics as editable Bézier paths. Separate fill and stroke colors use grouped paths. It imports supported plain curves, curve-only groups, basic rectangles/lines/bracket pairs, and ovals with explicit axes. Curves nested in molecular fragments are also retained on import.

CDXML does not retain ReShiki's parametric shape type. External saves may split grouped bracket strokes, change stacking values and quantize colors/coordinates. A subsequent import keeps the supported visible paths but may have a different number of graphic objects. Native saves preserve ReShiki's exact model.

Dotted CDXML export is explicitly rejected; native/SVG/PDF/PNG retain dots. Chemical polymer semantics, unsupported legacy shapes, arrowed/doubled/shaded curves, transparency, shadows and arbitrary external styling are not implemented. Continuous freehand stroke tracing, nonproportional resize handles and shape-specific corner radii remain gaps. Graphical brackets do not define polymers or repeating units. Pen paths reuse the existing native path commands and need no additional serialized fields.

Format references: the [published CDXML DTD](https://static.chemistry.revvitycloud.com/cdxml/CDXML.dtd), and the published format documentation for [curves](https://bobhanson.github.io/IUPAC-FAIRSpec/cdx_sdk/Curve.htm), [graphics](https://bobhanson.github.io/IUPAC-FAIRSpec/cdx_sdk/Graphic.htm) and [curve flags](https://bobhanson.github.io/IUPAC-FAIRSpec/cdx_sdk/properties/Curve_Type.htm).

Mixed-object and nested groups were subsequently added; see [selection and groups](selection-and-groups.md) for membership, integral groups, fitted frames and remaining exchange limits. Version 5 extends the native format with this group model.

## Verification

The desktop check drew a filled rectangle behind aspirin, selected its bond through the background, added a separately colored ellipse, dragged a Bézier control point, and bracketed the aromatic ring. The drawing was saved and reopened as `.reshiki`, exported through the CDXML save dialog, opened and saved by an external drawing application, and reopened in the standalone ReShiki app. The externally saved fixture is under `tests/fixtures/`; local screenshots and native files are under ignored `artifacts/graphics-qa-20260920/`.

This graphics update passed 59 Rust and 18 Python tests. New coverage exercises every shape's transforms, hit testing, point edits, copy/remapping, native history, color pixels in raster export, vector export, cleanup preservation and CDXML geometry/layers. It also checks graphics-only files, supported legacy objects, an externally saved interchange file, and explicit errors for unsupported styles and object types. Formatting and Clippy pass. At that milestone, the standalone bundle passed signature verification and imported the externally saved fixture from `/tmp` with `RESHIKI_ROOT=/nonexistent`, using its bundled worker.
