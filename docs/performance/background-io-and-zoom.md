# Background I/O, inspector construction and selected-canvas zoom

This follows the [editing latency audit](editing-latency.md) in
[PR #84](https://github.com/Ameyanagi/ReShiki/pull/84). The baseline is
`805b9fb`, including the earlier loaded-canvas and immediate O → OH work.
Measurements use optimized release builds and the shortcut gallery, with one
or four copies. They measure CPU work, not display FPS or end-to-end input latency.

## Changes and ownership

### Autosave and recovery

The five-second autosave tick now captures a drawing snapshot and dispatches a
blocking worker. Validation, JSON serialization, atomic write/fsync, recovery
candidate deletion and clearing run on that worker. Snapshot serialization borrows
the document instead of cloning it again. Inline captions receive local draft
checks before capture; full document validation happens on the worker.

Each window permits one recovery operation at a time. A boolean records that a
new snapshot is needed; repeated ticks do not queue document copies. Clear requests
run after any pending write and before a later save. Completion status requires
the matching document epoch, revision and draft generation. Text draft edits and
Undo invalidate completions even when the document revision has not changed.

Closing waits for explicit file/library operations and then recovery clearing.
A failed explicit write cancels that close and retains recovery protection. Update
installation drains recovery before handing off to the installer helper, so a
slow or failed clear cannot leave a helper waiting for an editor that stays open.
A restored candidate is deleted only after a durable replacement is written;
failed writes preserve both its file and its identity for a retry.

### Files and template libraries

Native file reading, JSON parsing and validation run on blocking workers. The
application publishes a prepared document only when the request serial, file epoch
and edit revision still match and no new text draft/chemistry edit is active. A
slow open cannot replace a newer drawing. Save serialization joins the existing
atomic-write worker, with one explicit save in flight per window.

Template changes capture owned transactions. Mutation, validation, reading the
current library and the checked atomic write run off the event loop. The visible
library changes only after success. Existing cross-window locking/conflict checks
are preserved. Import reading and library writes share a bounded pending state;
other mutations cannot overtake an import. Export serialization/writing also uses
a blocking worker. Canvas editing remains available, and late library completion
cannot reset a newer drawing's tool, selection or editing context.

### Inspector and rendering

Collapsed molecular, arrangement and grouping sections no longer construct hidden
content. Expanded sections share selected-ID indexes and alignment counts. Contrast
checks index bond degrees, and ring controls reject selections with too many
chemical atoms before expensive ring lookup. These are local calculations, with
no new inspector cache or retained document snapshots.

The existing scene cache now retains selection markers and caption rectangles in
world coordinates. Exact document and selected-ID comparisons invalidate them;
pan/zoom reuse them and offscreen decorations are culled.

A separate glyph cache shares Iced's hinted outlines across text strings. It keeps
the exact font-size key instead of scaling an outline from another zoom. Shaping,
glyph paths, color and bitmap fallback match Iced. Retention is bounded by 2 MiB of
accounted glyph data and 4,096 entries, in addition to the previous 8 MiB/2,048-entry
text-outline cache. These accounting limits are not a process-RSS guarantee;
allocator overhead and temporary shaping/rendering allocations are separate.
The adapted Iced implementation retains its MIT attribution in `licenses/iced/`.

## macOS measurements

Apple M4, 32 GiB RAM, macOS 26.5.1; 2026-09-29.
Canvas workloads use 20 measured iterations; inspector and I/O use three warmups
and 30 iterations. Values below are milliseconds, median / p95.

| Workload                                      |      Baseline |       Updated |
| --------------------------------------------- | ------------: | ------------: |
| Zoom, 1× gallery                              | 28.99 / 32.10 |  8.28 / 10.38 |
| Zoom, 4× gallery                              | 47.25 / 58.61 | 24.25 / 27.10 |
| Selected redraw, 4×                           | 29.39 / 29.61 | 12.76 / 13.81 |
| Drag all, 4×                                  | 18.13 / 18.61 |   4.11 / 4.18 |
| Full view, one selected object, collapsed, 4× |   3.62 / 3.65 |   0.52 / 0.55 |
| Full view, all selected, collapsed, 4×        | 11.15 / 12.16 |   2.98 / 3.05 |
| Full view, all selected, expanded, 4×         | 11.44 / 12.48 |   4.97 / 5.13 |

Inspector comparisons run the retained baseline executable and candidate adjacent
to each other with the same harness. The expanded workload intentionally measures
the contents that remain visible; collapsing a section is not required for a gain.

Recovery measurements separate UI dispatch from durable completion:

| Recovery operation                             |  1× gallery |    4× gallery |
| ---------------------------------------------- | ----------: | ------------: |
| Synchronous capture + validate/serialize/write | 6.16 / 8.35 | 11.03 / 12.25 |
| Updated Tick dispatch, including snapshot      | 0.20 / 0.31 |   0.37 / 0.55 |
| Updated worker round trip, including capture   | 8.40 / 9.33 | 10.04 / 12.06 |

The synchronous row invokes the same durable operation directly; it is a cost
comparison of work removed from the event loop, not a historical end-to-end app
measurement. Dispatch returns before the file is saved. The round trip includes
worker scheduling and disk completion and excludes display/event-loop latency.
Disk variability means asynchronous I/O is not a claim of faster storage.

Other moved synchronous work, measured independently:

| Operation                            | Median / p95 ms |
| ------------------------------------ | --------------: |
| Native parse + validate, 1× gallery  |   0.636 / 0.678 |
| Native serialization, 1× gallery     |   0.389 / 0.402 |
| Load 512 small templates             |   0.765 / 0.770 |
| Checked write of 512 small templates |   7.067 / 9.094 |

A separate trial indexed theme-library entries and sorted them once. For 300 files
it measured 6.36 ms versus 5.45 ms for the existing implementation. That change was
removed; there is no claimed theme-loading improvement.

## Windows measurements

AMD Ryzen 9 7940HS test VM, Windows 11, MSVC release build, 2026-09-29.
These are CPU measurements on the VM, not GPU usage or FPS on the physical Iris Xe.
The baseline is the previously built `805b9fb` code; the candidate is `e9818f5`.
The same canvas harness uses 20 iterations. Milliseconds, median / p95:

| Workload            |        Baseline |       Updated |
| ------------------- | --------------: | ------------: |
| Zoom, 1× gallery    | 107.28 / 118.10 | 31.50 / 46.26 |
| Zoom, 4× gallery    | 137.16 / 193.64 | 45.93 / 56.53 |
| Selected redraw, 4× |   35.14 / 36.13 | 17.16 / 17.57 |
| Drag all, 4×        |   23.97 / 25.52 |   6.86 / 7.62 |
| Partial drag, 4×    |   39.11 / 41.12 | 38.72 / 40.28 |

Updated full-view construction with all objects selected took 4.15 / 4.44 ms with
collapsed sections and 7.71 / 8.15 ms expanded at four galleries. No Windows
before/after inspector ratio is claimed because the old Windows executable did
not include the new inspector workload.

| Recovery operation                             |  1× gallery |   4× gallery |
| ---------------------------------------------- | ----------: | -----------: |
| Synchronous capture + validate/serialize/write | 3.52 / 4.16 | 8.20 / 10.48 |
| Updated Tick dispatch, including snapshot      | 0.22 / 0.25 |  0.75 / 0.87 |
| Updated worker round trip, including capture   | 3.78 / 4.55 | 8.10 / 10.04 |

Native parse/validation took 1.16 ms median and serialization 0.67 ms for one
gallery. A checked write of 512 small templates took 13.75 ms median. These bodies
now run on workers; they do not measure the time to present a completed save.

The final Windows application run passed 293 tests, with 16 explicitly ignored
cases, including the exact Iced glyph parity matrix and cache eviction tests.
Both the independent selection-marker and cached/fresh edit/drag pixel
comparisons passed. The build initially
hit a file lock from a test-listing process; the runner was corrected and the
final source rebuilt successfully. Source archive extraction touched the crate
roots so Cargo did not reuse a binary with older code.

## Validation and reproduction

The macOS release regression run passed 579 tests, with 18 explicitly ignored
benchmarks/platform/renderer cases. It includes native open/save behavior, stale
open results, inline drafts, ordered autosave, clear/close/restart, failed restore
retry, template import ordering and competing library writers. Existing chemistry,
clipboard CDX/CDXML, grouping, theme, typography and export suites also passed.

Canvas tests compare cached glyph paths directly with Iced across fonts, sizes,
bold/italic styles, Greek, Japanese, Arabic, emoji and multiline strings. The
renderer checks compare cached/fresh edits and drag results, and new selection
markers against the previous uncached implementation, including viewport edges.
Both pixel comparisons passed. No drawing format or chemistry algorithm changes
are introduced by this follow-up.

The isolated macOS release app passed actual desktop checks for selecting and
dragging the entire gallery, zoom, collapsed inspector controls, Undo/Redo,
autosave, native Save, New and Open. The saved/reopened fixture contains 483 atoms
(including five attachment points), 429 bonds and 132 captions. A recovery snapshot
was observed after editing and removed after saving; existing user windows and
preferences were preserved. All-target/all-feature compile checks, Clippy with
warnings denied, Rust formatting and Markdown formatting passed through the
normal pre-commit hooks.

A subsequent desktop report exposed a misleading red “Invalid drawing” notice
for the gallery's valid semantic attachments. Automatic label refresh now skips
analysis of attachment/centroid components after validating the complete drawing,
preserving their supplied labels. The inspector's existing limitation notice and
explicit molecular-export restrictions remain. Regression coverage checks the
gallery alongside an ordinary alcohol, both semantic attachment types, legacy
centroids, cache reuse, malformed targets and independent valence errors.

```sh
cargo test --release --locked --bin reshiki \
  app::autosave::tests::recovery_workloads -- --ignored --exact --nocapture
cargo test --release --locked --bin reshiki \
  app::files::performance::file_library_workloads -- --ignored --exact --nocapture
cargo test --release --locked --bin reshiki \
  app::inspector::tests::inspector_workloads -- --ignored --exact --nocapture
cargo test --release --locked --bin reshiki \
  canvas::performance::loaded_canvas_workloads -- --ignored --exact --nocapture
```

Raw benchmark, test and macOS `sample` logs are retained under
`artifacts/performance/` in the performance checkout. Timings for different
operations should not be added together and presented as measured input latency.

## Remaining limits

- Four-gallery zoom still exceeds a 16.7 ms CPU budget. Post-change sampling points
  to remaining outline preparation, repeated hover nearest-bond searches and ring
  geometry construction. These are findings, not implemented additional caches.
- Partial-selection dragging still builds a changed scene. This pass measured
  31.31 → 33.19 ms at four galleries; it did not improve that path.
- Document snapshot copying, saved-state comparison, publishing/fitting a loaded drawing and extracting a
  template selection still run on the event loop. Very large drawings can make
  those operations material.
- Initial recovery/library discovery and theme persistence/archive remain
  synchronous. Moving these safely needs its own measurements and state handling.
- Blocking work already running is allowed to finish. Stale results cannot publish
  over newer documents; there is no unbounded per-edit recovery/library queue.
