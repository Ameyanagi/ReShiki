# Selection resize and rotation handles

Updated 2026-10-02.

With Select active, selected structures have a teal bounding box with four square corner handles and a circular rotation handle above the box. Double-click an atom to select its connected molecule, or use Cmd/Ctrl+A to select all objects and activate Select. A lone selected atom keeps its circle without a transform box.

Drag a corner to resize proportionally about the opposite corner. Crossing the opposite corner clamps to a small positive scale instead of reflecting the molecule. Drag the circular handle to rotate about the selection center; hold Shift to snap to 15° increments. The rotation box follows the live preview and returns to an axis-aligned bound on release. Escape or window focus loss cancels the drag. Completed drags create one Undo step, and clicking a handle without moving creates none.

Double-click the rotation handle to focus **Properties → Transform → Rotate**,
or a corner to focus **Scale**. Double-click a side handle for **W**, or a top or
bottom handle for **H**. The panel opens and scrolls the field into view with
its value selected, ready to replace. Opening it leaves the drawing and Undo
history unchanged, and preserves any numeric drafts. Enter applies that field
using the existing numeric transform behavior. A second press that moves still
drags the handle. This shortcut follows the numeric-handle interaction checked
in ChemDraw 26 on macOS; it uses ReShiki's existing fields and transform rules.

Rotation uses the mean of the selected visible atom positions, plus one stable reference point for each selected caption, arrow or graphic. The keyboard, numeric Rotate field, menus and rotation handle use the same center. A turn followed by its inverse restores placement, and incremental turns agree with one turn by the total angle, within coordinate roundoff. The center can be offset from the middle of the selection box. Resizing and reflection keep their existing centers.

An upright caption contributes its insertion point, an arrow its endpoint midpoint, and a framed shape its frame center. Arcs use their parent ellipse center; lines and Bézier curves use their endpoint midpoint. Scientific symbols and orbitals use their placement anchor, and custom paths use their local control-coordinate bounds center. A lone caption stays in place when rotated. Hidden abbreviation atoms move with their visible anchor without adding weight to the center; derived ring-centroid markers add no weight. Changing font sizes or stroke widths does not move the rotation center.

The box includes atom labels. Transformations change selected coordinates, preserving whole-molecule connectivity and stereochemistry. JACS font sizes and line widths remain fixed; labels and annotations remain upright. Partial selections retain the existing boundary stereochemistry invalidation behavior. Selection decorations are canvas overlays and are not exported.

## Verification

Desktop checks used a separate app instance and recovery directory, leaving the user's live drawing untouched. Selected the startup aspirin molecule, resized it with the bottom-right corner and rotated it using the circular handle. Inspected the box and labels after each drag. Two Undo operations restored the original drawing, and two Redo operations restored both transforms. Structure checking retained 13 atoms, 13 bonds, formula `C9H8O4`, and the original canonical SMILES. Also verified that Shift+R turns aromatic ring mode on and off.

Automated checks cover handle hit routing, grab offsets at multiple zoom levels, proportional scaling, positive-scale clamping, 15° rotation snapping, cancellation on focus loss, unchanged unrelated objects, exact Undo/Redo snapshots and no-op history behavior. The RDKit integration check confirms that resizing and rotation preserve tetrahedral and alkene stereochemistry. All 39 Rust tests and 10 Python tests pass, together with formatting, Clippy and application signature verification.
