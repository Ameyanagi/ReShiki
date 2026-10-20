# PNG frame measurements

Measurements use exact PNG RGB bytes `(62, 133, 117)` in the inclusive canvas region `x208..1959, y364..1619`. Coordinates below are zero-based screenshot pixel indices. Consecutive rows or columns containing **more than 400** exact-color pixels form a stroke. The compact handles do not create qualifying long rows or columns and cannot enlarge the measured frame.

**Geometric dimensions** are `right.first − left.first` and `bottom.first − top.first`, the distance between corresponding edges of the opposite strokes. **Outer footprint** includes every pixel of both strokes, so a frame with 2px strokes has a footprint two pixels larger than its geometric dimensions. The center is the mean of opposite stroke midpoints. To convert raster-index centers to physical point coordinates for a 2px/pt screenshot, add 0.5px to each coordinate and divide by 2.

| Screenshot | Vertical stroke columns | Horizontal stroke rows | Geometric size (px) | Outer footprint (px) | Stroke-center center (px) |
|---|---|---|---:|---:|---:|
| `A00-opening-2d.png` | No qualifying >400px stroke | No qualifying >400px stroke | — | — | — |
| `A01-3d-122.png` | 664..665; 1408..1409 | 603..604; 1347..1348 | 744 × 744 | 746 × 746 | (1036.5, 975.5) |
| `A02-resize-first-attempt.png` | 664..665; 1408..1409 | 603..604; 1347..1348 | 744 × 744 | 746 × 746 | (1036.5, 975.5) |
| `A02-resized.png` | 664..665; 1548..1549 | 603..604; 1487..1488 | 884 × 884 | 886 × 886 | (1106.5, 1045.5) |
| `A03-resized-rotated.png` | 664..665; 1548..1549 | 603..604; 1487..1488 | 884 × 884 | 886 × 886 | (1106.5, 1045.5) |
| `A04-layout-from-3d.png` | 664..665; 1548..1549 | 603..604; 1487..1488 | 884 × 884 | 886 × 886 | (1106.5, 1045.5) |
| `A05-flattened-2d.png` | No qualifying >400px stroke | No qualifying >400px stroke | — | — | — |
| `A06-back-to-3d.png` | 664..665; 1548..1549 | 603..604; 1487..1488 | 884 × 884 | 886 × 886 | (1106.5, 1045.5) |
| `A07-layout-original.png` | 664..665; 1548..1549 | 603..604; 1487..1488 | 884 × 884 | 886 × 886 | (1106.5, 1045.5) |
| `A08-original-2d.png` | 750..751; 1295..1295 | 760..760; 1223..1223 | 545 × 463 | 546 × 464 | (1022.75, 991.5) |
| `A09-undo.png` | 664..665; 1548..1549 | 603..604; 1487..1488 | 884 × 884 | 886 × 886 | (1106.5, 1045.5) |
| `A10-both-122.png` | 750..751; right clipped | 533..534; 1417..1418 | unknown × 884 | unknown × 886 | (unknown, 975.5) |
| `A11-both-97.png` | 812..812; right clipped | 620..621; 1337..1337 | unknown × 717 | unknown × 718 | (unknown, 978.75) |
| `A12-both-deselected.png` | No qualifying >400px stroke | No qualifying >400px stroke | — | — | — |
| `A13-undo-both.png` | 743..744; 1460..1461 | 676..677; 1393..1393 | 717 × 717 | 719 × 718 | (1102.0, 1034.75) |
| `A14-redo-both.png` | 812..812; right clipped | 620..621; 1337..1337 | unknown × 717 | unknown × 718 | (unknown, 978.75) |
| `A15-redo-2d.png` | 743..744; 1460..1461 | 676..677; 1393..1393 | 717 × 717 | 719 × 718 | (1102.0, 1034.75) |
| `A15a-undo-to-3d-original.png` | 743..744; 1460..1461 | 676..677; 1393..1393 | 717 × 717 | 719 × 718 | (1102.0, 1034.75) |
| `A15b-undo-original-restore.png` | 743..744; 1460..1461 | 676..677; 1393..1393 | 717 × 717 | 719 × 718 | (1102.0, 1034.75) |
| `A16-both-after-redo.png` | 812..812; right clipped | 620..621; 1337..1337 | unknown × 717 | unknown × 718 | (unknown, 978.75) |

These captures have one or more exact-color strokes whose thickness differs from 2px: `A08-original-2d.png`, `A13-undo-both.png`, `A15-redo-2d.png`, `A15a-undo-to-3d-original.png`, `A15b-undo-original-restore.png`. Their dimensions use the same corresponding-first-edge formula shown above; the JSON preserves every observed stroke thickness. No antialiased pixels or inferred stroke edges are added.

The selected Both captures `A10-both-122.png`, `A11-both-97.png`, `A14-redo-both.png`, `A16-both-after-redo.png` have a visible left edge and two horizontal strokes reaching `x1959`, the canvas/scan right boundary. Their right edges are clipped by the properties sidebar. Only observed strokes and the height are reported. Full width, horizontal center, and missing right-edge position are **unknown** and are not inferred.

## A03 → A06 → A07 comparison

- `A06-back-to-3d.png` versus `A03-resized-rotated.png`: geometric width Δ0px, height Δ0px, center Δ(0.0, 0.0)px. All recorded frame geometry identical: **True**.
- `A07-layout-original.png` versus `A03-resized-rotated.png`: geometric width Δ0px, height Δ0px, center Δ(0.0, 0.0)px. All recorded frame geometry identical: **True**.

The first A02 attempt is retained as setup evidence: the capture owner reported that keyboard drawing was initially enabled and that the first drag did not resize the frame. The subsequent `A02-resized.png` is the successful resize capture.

The numeric measurements compare frame geometry only. They do not establish whether the molecule orientation or other screenshot content is identical. 'No qualifying >400px stroke' does not mean no visible selection frame: a smaller visible frame can fall below this threshold, as in A05. Such captures are not assigned inferred dimensions.

## Capture-state notes

- A15-redo-2d.png actually shows 3D with Original drawing layout; its filename does not describe its final displayed state. This is independently visible in its selected 3D display control and Original drawing layout control.
- The capture owner reports this history path: Undo Both → A15a-undo-to-3d-original.png (3D, Original drawing); additional Undo Original restore → A15b-undo-original-restore.png (3D, From the 3D view); Redo Original restore → A15-redo-2d.png (3D, Original drawing); Both → A16-both-after-redo.png. The former display-2D redo branch was discarded when Both was selected.

## Independent visual review

- Read-only view_image inspection covered A05-flattened-2d.png, A08-original-2d.png, A10-both-122.png, A11-both-97.png, A12-both-deselected.png, A14-redo-both.png, A15-redo-2d.png, A15a-undo-to-3d-original.png, A15b-undo-original-restore.png, and A16-both-after-redo.png. No UI or application interaction was performed for this review.
- A05 visibly contains a compact selected 2D projection with a diagonally slanted, flattened molecular layout. Its bottom status reads 'Shown as 2D from the 3D view'. Its visible selection frame is too small to produce rows or columns containing more than 400 exact-color pixels, so the numeric method intentionally assigns no frame dimensions.
- A08 shows the original, more upright drawn arrangement: a left-pointing carbonyl on the left side, a downward carbonyl below the six-member ring, a five-member ring on the right, and N-substituent bonds extending upper-left and lower-left. These visible relative directions distinguish it from the flattened diagonal projection in A05.
- The left-hand 2D drawing in A11, A12, A14, and A16 retains the same distinctive original arrangement seen in A08: leftward and downward carbonyl directions, the upright fused ring on the right, and upper-left/lower-left N-substituent bonds. It is visually consistent with the Original drawing layout, rather than the diagonal projection in A05. A11, A14, and A16 also show Original drawing selected in the properties sidebar.
- A12 removes the selection overlay and shows the clean original line drawing alongside the ball-and-stick model, making the original-layout evidence easiest to inspect. A11, A14, and A16 show the same two presentations together with selection overlays. The right 3D presentation is visibly present in all four reviewed Both captures.
- A15 and A15a visibly select 3D plus Original drawing. A15b visibly selects 3D plus From the 3D view. The 3D frame and model remain in the same visible position across these three screenshots. A10 independently confirms that the selected Both frame and part of the 3D model extend past the visible right canvas edge behind the properties sidebar.
- These are visual layout comparisons at different zoom levels and with different selection overlays, not a pixel-identity claim for the molecule or the whole screenshot.

## Reproduce

```sh
python3 tools/measure_png_frames.py 'A*.png' --json frame-measurements.json --markdown frame-measurements.md --notes tools/frame_review_notes.json
```

The tool uses only the Python standard library and reads stored PNG color bytes without display color conversion. The JSON includes image dimensions, exact match counts, each qualifying stroke's full interval and per-coordinate match counts, inclusive outer bounds, geometric dimensions, outer footprint, and center.
