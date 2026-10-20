# Issue #389 evidence: 3D tilt drag keeps turning

Branch `fix/tilt-full-rotation` (20d78345, 519c5ff1) vs `origin/main` (036f0c82).

## Harness

`harness/tilt_drag_389.rs` is a throwaway Cargo example. It was copied into
`examples/` for each run and then deleted, so it was never committed. It
compiles the checkout's own `src/canvas/tilt.rs` (`#[path]`), so it runs the
real `TiltDrag::angles` and `tilt::apply`, which calls
`reshiki::projection::tilt`, from that checkout. It builds a benzene with Cl
on the upper-right vertex and O at the bottom. Without them the hexagon looks
the same after a 180° turn. It runs `projection::depth_bonds` so the near ring
edges are drawn bold. It then applies a horizontal drag of N px from (0, 0)
with Shift off and saves `.rsk`.

The "measured Y rotation" is computed from the Cl atom's retained XYZ:
θ = atan2(−z′, x′) about the ring centre. It is printed in −180…+180°.

`harness/render.py` renders each `.rsk` with the repo CLI: `reshiki --cli
render FILE -o FILE.png --width 600 --height 400` and `-o FILE.svg`. The
binary was built from the branch. The render path is unchanged by the fix.

## Results

| Drag | main `angles()` Y | main measured | branch `angles()` Y | branch readout | branch measured |
| ---- | ----------------- | ------------- | ------------------- | -------------- | --------------- |
| 120 px | +60° | +60° | +60° | +60° | +60° |
| 240 px | **+75° (clamped)** | +75° | +120° | +120° | +120° |
| 360 px | **+75°** | +75° | +180° | +180° | ±180° |
| 400 px | **+75°** | +75° | **+200°** | −160° | −160° (= 200°) |
| 480 px | **+75°** | +75° | +240° | −120° | −120° (= 240°) |
| 720 px | n/a | n/a | +360° | +0° | 0°, document unchanged (`changed=false`) |

Raw harness output: `before.txt` and `after.txt`.

- Before vs after, 400 px drag: `before/before-400px.png` stays at 75° (the
  ring is nearly edge-on). In `after/after-400px.png` the ring has turned past
  edge-on to 200°: the drawing is mirrored (Cl now upper left) and the bold
  near edges are on the other side.
- Branch sequence: `after/after-120px` (60°), `-240px` (120°), `-360px`
  (180°) and `-480px` (240°), as `.png`, `.svg` and `.rsk`, with the flat
  reference `after/after-0px.*`. On main, drags of 240 px and more all render
  the same 75° drawing (`before/before-{240,360,480}px.*`).

## Files

- `README.md`, `before.txt`, `after.txt`
- `harness/tilt_drag_389.rs`, `harness/render.py`
- `before/before-{120,240,360,400,480}px.{rsk,png,svg}`
- `after/after-{0,120,240,360,400,480,720}px.{rsk,png,svg}`
