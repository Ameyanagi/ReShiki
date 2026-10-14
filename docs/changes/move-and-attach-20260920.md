# Move & attach: original checks (2026-09-20)

These checks describe the first Move & attach build. The
[joining page](../fragment-joining.md) describes how joining works today.

## Tests

`tests/joining.rs` checks connect/share/fuse identities, stable IDs, captions,
groups, single-atom/edge merging, remote tetrahedral stereo and rejected input.
App tests check cancellation, mode selection, stale previews and atomic
Undo/Redo.

## Desktop check

A 2026-09-20 desktop check selected an existing furan edge, fused it onto
benzene, verified Undo/Redo and saved the resulting nine-atom, ten-bond
drawing. Reopening and chemistry analysis produced benzofuran (C8H6O).
