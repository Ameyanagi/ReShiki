# Loaded canvas profiling

The bundled `assets/examples/shortcut-examples.rsk` is the regression workload:
483 atoms, 429 bonds, 132 captions, 16 abbreviations, and one arrow. A second
workload appends four copies horizontally (1,932 atoms, 1,716 bonds, 528 captions).
Both use the same 1,066 × 570 viewport and initial 50% zoom; the larger workload tests the cost of
content outside the viewport as well as visible content.

## Reproduction

Use a release build, an otherwise idle machine, and the same renderer on both
revisions. The canvas benchmark warms each operation three times, then reports median,
p95, and mean over 20 iterations. Set `RESHIKI_PERF_ITERATIONS` to change that
count. Run benchmarks separately from tests, builds, and sampling profilers.

```sh
cargo test --release --locked --bin reshiki \
  canvas::performance::manual_loaded_canvas_workloads -- --ignored --exact --nocapture
cargo run --release --locked --example canvas_performance
```

The canvas benchmark calls the application's actual `Program::draw` and
`mouse_interaction` paths. It measures CPU preparation of geometry and text, not
GPU presentation, input-to-display latency, or display FPS. The library benchmark
separately measures document copying, scene generation, selection bounds, and
translation over 30 measured iterations. Its `--profile` argument repeats scene construction for 30 seconds
and prints the process ID for a sampling profiler.

The model scaling benchmarks need no renderer. Each row gives the median, 95th
percentile and mean of the warmed runs, then the time of the very first call
(`first_ms`):

```sh
cargo test --release --locked -p reshiki-model \
  scene::performance -- --ignored --nocapture --test-threads=1
```

- `model_scaling_workloads` repeats the workload 1, 4 and 16 times side by side
  (483, 1,932 and 7,728 atoms), then 4 and 16 times stacked one above the
  other. For each size it reports scene generation, bond-crossing gaps,
  selection bounds, **Align left** and document copying. With one
  two-attachment abbreviation added, it reports the drawn copy that an
  unchanged drawing shares (`render_copy_shared`) and the one an edit takes
  (`render_copy_fresh`).
- `column_and_large_group_workloads` covers shapes the gallery lacks, at 256,
  1,024 and 4,096 objects: columns of ammonia labels, single bonds, condensed
  `NMe` labels and captioned arrows, and one collapsed group of that many
  atoms.
- `imported_polyalanine_scene` builds the scene of the drawing that importing
  polyalanine-200 from SMILES leaves on the canvas: 1,001 atoms folded into
  about seven by eight bond lengths, with their hydrogen labels. Run it alone
  (`scene::performance::imported_polyalanine_scene -- --ignored --exact`) so
  that `first_ms` is the first scene of a fresh process.

A step whose time grows much faster than the drawing shows up as a 16× row that
is far more than 16 times its 1× row.

For a rendered correctness check (requires a working headless renderer):

```sh
cargo test --release --locked --bin reshiki \
  canvas::performance::cached_canvas_matches_fresh_edits_and_committed_drag \
  -- --ignored --exact --nocapture
```

This compares cached rendering with a fresh canvas after text and position edits,
pan/zoom, undo/redo, and a whole-document drag followed by its committed result.
Exact equality is required for cached versus fresh frames. Preview versus commit
allows a small antialiasing tolerance for algebraically equivalent floating-point
translations: fewer than 0.1% of pixels may differ by more than eight channel
levels, with mean absolute channel error below 0.1/255.

## Profile findings and changes

The baseline is main `ae5ec464ace991ec498bb1acb155635cc7262c1a`, also published as
`nightly-0.9.1-nightly.20260929.36525615302.1`.

The baseline build received only the same `examples/canvas_performance.rs` and
`src/canvas/performance.rs` workload harnesses, the test-only module declaration,
and test-helper visibility needed to compile them. No runtime optimizations were
present in that baseline. Warming excludes first-use cache/font initialization;
the drag measurements cover preview updates, not the mouse-release commit.

- Long caption layout remeasured the growing text run for every appended
  character. Keeping its accumulated advance removes quadratic work without
  changing wrapping, justification, fallback fonts, or script positions.
- Unchanged scenes and selection bounds were reconstructed on each redraw and
  pointer query. Each canvas now retains one document snapshot, scene, and
  selection. Content equality invalidates derived data, including in-place edits,
  style changes, loading, and undo/redo. Selection changes invalidate only its
  bounds. Camera and viewport changes remap the cached world coordinates.
- Glyph outlines were regenerated while panning and dragging. Position-independent
  outlines are keyed by text, font, size, zoom, weight, slant, and color. The cache
  has an eight MiB accounting budget for keys and path events, plus a 2,048-entry
  cap. It clears when either limit is reached. Oversize entries render without
  being retained. Bounds conservatively include Bézier control points and an
  antialiasing margin for viewport culling. Underlines remain separate geometry.
- A whole-document translation preserves relative geometry, so its preview uses
  the existing scene with a translated camera. Partial drags rebuilt the scene
  because bond endpoints, labels, ring fusion, and crossing gaps can change;
  they now rebuild only the moved molecules (see
  [Previews and panning](#previews-and-panning)).
- A follow-up profile of partial drags found repeated ring endpoint lookups and
  bond-neighbor searches. Ring crossings now index atom endpoints and reject
  distant segments before visibility checks. Bond joins build eligibility,
  neighbors, and label visibility once per scene, preserving document order.
  These borrowed indexes have no lifetime beyond the current scene build.

Async scene generation remains deferred: the canvas profiles identified redundant
CPU work that can be removed directly. A subsequent [editing and chemistry audit](editing-latency.md)
introduces immediate background label updates with version checks and a bounded
per-molecule cache. Moving scene generation to a worker would require
versioned results, cancellation/coalescing, and a defined policy for stale previews.
The measurements below determine whether that additional complexity is needed.

## Measurements

Measured 2026-09-29. Mac: Apple M4, 32 GiB RAM, macOS 26.5.1, release build.
Canvas CPU milliseconds (median / p95):

| Workload                       |     Baseline 1× |  Optimized 1× |     Baseline 4× |  Optimized 4× |
| ------------------------------ | --------------: | ------------: | --------------: | ------------: |
| Unselected redraw              |   49.44 / 57.15 |   1.50 / 1.66 | 314.79 / 361.30 | 14.78 / 15.69 |
| Selected redraw                |  92.96 / 106.66 |   3.19 / 3.24 | 481.43 / 537.44 | 25.63 / 26.52 |
| Pointer/selection handle query |   30.13 / 30.65 |   0.01 / 0.01 | 106.10 / 113.54 |   0.07 / 0.07 |
| Drag all                       | 115.30 / 121.36 |   2.67 / 2.70 | 572.82 / 638.24 | 16.04 / 16.38 |
| Drag two atoms                 |   48.54 / 54.21 |   2.87 / 2.89 | 287.61 / 335.20 | 31.13 / 31.68 |
| Pan                            |  98.50 / 108.31 |   3.64 / 3.67 | 472.12 / 512.86 | 26.59 / 31.97 |
| Zoom                           |   93.29 / 97.86 | 25.12 / 26.49 | 467.03 / 530.79 | 55.71 / 69.06 |

The large workload still spends about 31 ms on a partial drag and 56 ms on
a changing zoom level. These are remaining optimization targets; the change
does not claim a universal 60 FPS frame budget.

Windows test VM: AMD Ryzen 9 7940HS, Windows 11, remote/software display
adapters, release build. Matched CPU workloads (median / p95 milliseconds):

| Workload                       |     Baseline 1× |    Optimized 1× |       Baseline 4× |    Optimized 4× |
| ------------------------------ | --------------: | --------------: | ----------------: | --------------: |
| Unselected redraw              | 166.81 / 173.94 |     2.92 / 3.09 |   774.89 / 813.63 |   17.01 / 19.42 |
| Selected redraw                | 230.28 / 238.06 |     6.65 / 6.91 | 1006.05 / 1057.88 |   32.85 / 33.70 |
| Pointer/selection handle query |   37.97 / 42.38 |     0.02 / 0.02 |   160.75 / 164.72 |     0.07 / 0.09 |
| Drag all                       | 253.99 / 273.11 |     5.42 / 5.68 | 1182.48 / 1216.23 |   23.11 / 23.86 |
| Drag two atoms                 | 155.49 / 168.76 |     5.19 / 5.43 |   764.00 / 788.21 |   39.68 / 41.42 |
| Pan                            | 226.44 / 232.32 |     6.68 / 6.95 | 1004.18 / 1035.06 |   34.32 / 36.88 |
| Zoom                           | 233.97 / 243.48 | 105.50 / 114.61 | 1027.72 / 1109.86 | 132.31 / 192.77 |

The Windows renderer pixel comparison also passed. These results measure CPU
preparation on the test VM, not an Intel Iris Xe GPU or Windows display FPS.
Zoom remains relatively expensive on this host (106 ms at 1×, 132 ms at 4×).

## Validation and limits

- 507 targeted tests passed: 230 library tests, 264 application tests, nine
  typography tests, three figure-export tests, and one CDX/CDXML exchange test.
  The exchange test covers nine ChemDraw fixture families. These are automated
  interchange checks, not a new external ChemDraw clipboard session.
- The release-mode rendered-pixel check passed on macOS and Windows. Cache tests cover edits,
  document replacement with reused IDs, selection changes, undo/redo, font,
  color, size, zoom, and eviction limits.
- An isolated macOS application using the release binary was checked with the
  actual shortcut gallery at 50% zoom: select all, six pairs of forward/reverse
  drags, undo/redo, double-click molecule selection, dragging that molecule, and
  zooming to 62%. No stale drawing, selection, or caption placement was observed.
  `sample` recordings from both the published nightly and candidate support the
  CPU findings. This desktop check does not measure display FPS.
- Windows baseline/candidate CPU workloads and the renderer pixel comparison
  completed successfully. The Mac desktop interaction check remains separate
  from these headless Windows tests.
- The scene cache retains one document snapshot and compares document contents.
  Equality checking remains linear in document size; large image-heavy drawings
  were not measured. The text budget accounts for retained keys and path events,
  rather than claiming an exact allocator or process-memory ceiling.

This is a performance change with intended identical visual output. The timing
comparison, pixel regression, and desktop interaction checks are the review
evidence; a static before/after drawing would not demonstrate the lag reduction.

Release-note caption: “Dragging and selecting the shortcut gallery requires much
less CPU work, with bounded text caching and checks that edits and undo/redo keep
the drawing current.”

## Previews and panning

Measured 2026-11-15 against main `fcd2cde`. Ring, template and bond previews and
partial moves rebuilt the scene of the whole drawing on every pointer move, even
with all of it off screen. Panning at Fit re-tessellated every visible bond and
label, because the retained geometry was keyed on the camera center.

- A preview that changes only atoms, bonds and abbreviations now draws the
  molecules it touches over the retained rest of the drawing. Bonds from either
  drawing join a changed atom to its molecule. The rest keeps its scene and
  tessellation for as long as the same objects are left out. The split applies
  only while the two parts stay apart. Each part counts its ink (each subpath,
  label and halo), its atoms and bonds, drawn or not, and the reach of each
  automatic indicator (`Indicator::reach`): the area whose atoms, bonds and text
  can move it. No box of the redrawn molecules may come within three pixels plus
  0.12 of the largest label font size of the rest's boxes. That is the gap
  automatic hydrogens, charges and group labels keep from other ink. Crossing
  gaps, halos, paint order and automatic label placement then cannot differ from
  drawing the whole preview. Otherwise, and for captions, arrows, graphics, 3D
  models and rear opacity, the preview draws the whole drawing as before.
- The first pan at a zoom level tessellates half a view beyond each edge. Later
  pans within that margin draw the same geometry shifted, through the canvas
  widget's layer translation. Edits and zoom changes still tessellate only the
  view, so they cost what they did.

### Reproduction

```sh
cargo test --release --locked --bin reshiki \
  canvas::performance::manual_preview_workloads -- --ignored --exact --nocapture
```

The benchmark uses the gallery at 1×, 4× and 16× (483, 1,932 and 7,728 atoms) in
the same 1,066 × 570 view. "Onscreen" shows the gallery's bottom row at 50% zoom
with the pointer on the empty paper below it; "offscreen" moves the camera far
from all of it. The bond is drawn from the lowest end carbon, and the partial move
drags that carbon and its neighbor. Pan runs at Fit and at 50%.

Correctness checks (require a headless renderer):

```sh
cargo test --locked --bin reshiki canvas::paper::preview_tests -- --ignored --test-threads=1
cargo test --locked --bin reshiki \
  retained_paper_scene_matches_live_layers_and_invalidates_draw_inputs \
  -- --ignored --test-threads=1
```

`preview_fragments_match_whole_previews` compares 384 preview frames (four tools,
24 pointer positions, two zoom levels, two views) with the whole preview. All are
pixel-identical; 208 of them split. It also moves an anonymous dummy atom, which
draws nothing, next to an automatic attached number: the number moves in both. `preview_frames_draw_only_the_changed_molecules`
counts the primitives a second preview frame draws at 4×: 1,321 before (the whole
drawing), at most 26 after. A pan within the margin draws the shifted geometry
within the drag preview's tolerance above; edges differ by floating-point ulps
because the geometry was tessellated at other, equivalent coordinates.

### Measurements

Apple M1, 16 GB, macOS 27.0.1, release builds, wgpu renderer. The baseline is
main `fcd2cde` with only this benchmark (`src/canvas/performance.rs`) added; the
candidate is this change. Both were built before any run. The runs alternate
baseline and candidate, three of each, and each starts only when no compiler,
linker or cargo process runs. Another application kept the load average at 2.6–3.9
throughout, so compare the columns rather than the absolute times.
Milliseconds, the median over runs of each run's median of 20 iterations:

| Workload                   | Before 1× | After 1× | Before 4× | After 4× | Before 16× | After 16× |
| -------------------------- | --------: | -------: | --------: | -------: | ---------: | --------: |
| Benzene hover, onscreen    |      6.54 |     2.03 |      29.5 |      9.7 |        214 |        77 |
| Benzene hover, still       |      6.49 |     2.02 |      29.6 |     9.75 |        216 |        78 |
| Benzene hover, offscreen   |      5.57 |     1.69 |      26.5 |      7.5 |        205 |        67 |
| Cyclohexane template hover |      6.46 |     2.03 |      28.8 |     9.33 |        185 |        46 |
| Bond drag                  |      6.60 |     2.54 |      34.6 |     17.1 |        295 |       171 |
| Partial move               |      4.89 |     0.63 |      21.3 |     2.63 |        148 |      14.4 |
| Pan at Fit                 |      7.66 |     0.10 |      31.2 |     0.57 |        138 |      5.25 |
| Pan at 50%                 |      2.06 |     0.10 |      2.58 |     0.57 |       8.12 |      5.32 |
| Unchanged redraw, onscreen |      0.86 |     0.83 |      5.58 |     5.47 |       27.8 |      26.9 |

Each workload's run medians stayed within 8% of each other, most within 3%. The preview rows still grow
with the drawing, because every frame builds the preview document and checks
it against the whole drawing (see [Remaining cost](#remaining-cost)). "Still"
redraws the same pointer position, and costs as much as a moving pointer. In
`loaded_canvas_workloads` (two runs each), partial drags fell from 4.81 to
0.64 ms at 1× and from 21.1 to 2.59 ms at 4×, dragging everything from 0.96 to
0.36 ms at 1× and 2.55 to 1.82 ms at 4×, and its pan from 1.06 to 0.44 ms at 1×.
Draws, hits and zoom stayed within 4%.

### Remaining cost

A preview frame still builds the preview document and checks it against the
drawing, even when the pointer has not moved since the last frame. An earlier
profile at 16× on this Mac, taken while other builds ran, found placing the
benzene ring took about 40 ms (12 ms of it validating the whole drawing),
copying the drawing about 3 ms, and the editor markers about 6 ms. A bond drag
still costs about 170 ms at 16×. The first frame
of a preview that changes an existing molecule builds the scene of the rest once.

Inserting polyalanine-200 (1,001 atoms) from SMILES, measured with
`app::engine_jobs::tests::profile_polyalanine_insert_stages` and a release
helper while other builds ran: layout 313 ms, analysis 41 ms (130 ms with stereocenters), Fit 146 ms, and
two scene builds of about 960 ms each. The analysis steps are SMILES 13–17 ms,
InChI 15–100 ms and descriptors 3 ms; the rest take under a millisecond. In two
runs, the first analysis after a new helper executable was copied took about one
second (991 and 1,201 ms). That suggests a cost of first launching a new
executable, such as macOS checking it, but the profiler does not time the
helper's launch apart from its work, so the cause is not confirmed. The scene
builds are the subject of the model scaling work.
