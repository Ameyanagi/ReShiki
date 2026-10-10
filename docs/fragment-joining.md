# Move and attach existing fragments

**Join** in the selection row or context menu (Cmd/Ctrl+J) shares two selected
attachment atoms, or fuses the four endpoints of two bonds. These existing routes
move a complete source fragment to a separate destination fragment.

With three or more selected atoms, Join also merges the selected atoms into the
first selected atom at the center of the selected visible label-ink bounds.
An atom with a hidden label contributes its position instead. Its ID, element
label and label color/style survive; surrounding atoms and captions stay in place. Add
atoms with Shift-click to control which atom is selected first. For four atoms
that form two selected bonds, Join retains bond fusion; choose **Merge atoms**
to explicitly merge all four atoms into one instead.
Bond fusion requires four distinct endpoints. Two adjacent bonds among four
selected atoms use the multi-atom merge.

Merging removes internal self-bonds and combines identical duplicate bonds.
Conflicting bond metadata, charge, isotope, fixed hydrogen, radical, aromatic,
map or mark states are rejected with a diagnostic. Abbreviations must be expanded
first. Merging stereocenters or incident stereo bonds, collapsing stereo neighbors,
centroid/attachment target sets or painted rings, and conflicting reaction roles
or depth paint are also rejected. Compatible remote stereo, groups, reaction
coefficients and atom references are remapped. The whole merge is one Undo step.
Projection depth retains the mean of the selected depths; the XY label-bounds
placement does not establish a ChemDraw-compatible 3D rule. Headless model calls
use the selected atom-position bounds center unless supplied a finite position.

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
