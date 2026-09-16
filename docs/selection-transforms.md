# Selection resize and rotation handles

Updated 2026-09-17.

With Select active, selected structures have a teal bounding box with four square corner handles and a circular rotation handle above the box. Double-click an atom to select its connected molecule, or use Cmd/Ctrl+A to select all objects and activate Select. A lone selected atom keeps its circle without a transform box.

Drag a corner to resize proportionally about the opposite corner. Crossing the opposite corner clamps to a small positive scale instead of reflecting the molecule. Drag the circular handle to rotate about the selection center; hold Shift to snap to 15° increments. The rotation box follows the live preview and returns to an axis-aligned bound on release. Escape or window focus loss cancels the drag. Completed drags create one Undo step, and clicking a handle without moving creates none.

The box includes atom labels. Transformations change selected coordinates, preserving whole-molecule connectivity and stereochemistry. JACS font sizes and line widths remain fixed; labels and annotations remain upright. Partial selections retain the existing boundary stereochemistry invalidation behavior. Selection decorations are canvas overlays and are not exported.

## Verification

Desktop checks used a separate app instance and recovery directory, leaving the user's live drawing untouched. Selected the startup aspirin molecule, resized it with the bottom-right corner and rotated it using the circular handle. Inspected the box and labels after each drag. Two Undo operations restored the original drawing, and two Redo operations restored both transforms. Structure checking retained 13 atoms, 13 bonds, formula `C9H8O4`, and the original canonical SMILES. Also verified that Shift+R turns aromatic ring mode on and off.

Automated checks cover handle hit routing, grab offsets at multiple zoom levels, proportional scaling, positive-scale clamping, 15° rotation snapping, cancellation on focus loss, unchanged unrelated objects, exact Undo/Redo snapshots and no-op history behavior. The RDKit integration check confirms that resizing and rotation preserve tetrahedral and alkene stereochemistry. All 39 Rust tests and 10 Python tests pass, together with formatting, Clippy and application signature verification.
