# Move and attach existing fragments

Select an atom or molecule and choose **Move & attach…** above the canvas.
The entire connected fragment moves, so selecting one endpoint never cuts its
other bonds. Explicitly selected captions/graphics and integral groups move too.

Use the inspector preview to choose the exact source atom or bond:

- **Connect with a bond** joins the chosen source atom to a destination atom with
  a new single bond, aligning both attachment directions.
- **Share an atom** merges compatible source and destination atoms.
- **Fuse along a bond** shares the chosen source bond's atoms with a compatible
  destination bond. Choosing a source bond switches to this mode.

Hover a destination to preview. Click to join, or drag from the destination to
choose direction/side. Shift/Ctrl snaps the direction; Alt releases it. The
preview and final join use the same placement geometry. Escape, Cancel or
switching tools leaves the original drawing intact. Joining is one Undo step.

Moved objects retain their IDs; shared atoms retain the destination IDs. Groups
and captions are reconciled, unrelated objects stay fixed, and remote stereo
references survive remapping. A document revision/file guard prevents an old
preview from overwriting newer edits. Assistant application waits until an
active joining gesture finishes or is cancelled.

The destination must be a separate existing fragment. The same chemistry
restrictions as template attachment apply: compatible element, bond order,
valence and stereo at the attachment site. Expand abbreviations before using
their anchors. Common five/six-member aromatic fusion can reassign Kekulé bonds;
this is not an unrestricted graph merge or general stereocenter-merging tool.

`tests/joining.rs` checks connect/share/fuse identities, stable IDs, captions,
groups, single-atom/edge merging, remote tetrahedral stereo and rejected input.
App tests check cancellation, mode selection, stale previews and atomic Undo/Redo.
A 2026-09-20 desktop check selected an existing furan edge, fused it onto benzene,
verified Undo/Redo and saved the resulting nine-atom, ten-bond drawing. Reopening
and chemistry analysis produced benzofuran (C8H6O).
