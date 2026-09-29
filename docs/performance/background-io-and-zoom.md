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
- Document snapshot copying, publishing/fitting a loaded drawing and extracting a
  template selection still run on the event loop. Very large drawings can make
  those operations material.
- Initial recovery/library discovery and theme persistence/archive remain
  synchronous. Moving these safely needs its own measurements and state handling.
- Blocking work already running is allowed to finish. Stale results cannot publish
  over newer documents; there is no unbounded per-edit recovery/library queue.
