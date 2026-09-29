# Performance from v0.9.1 to the current implementation

The original slow baseline remains part of the record. In the four-gallery drag
workload, the pre-optimization nightly needed **572.82 ms on Mac and 1,182.48 ms on
Windows**; the third optimization stage measured **4.11 ms and 6.86 ms**. These are
CPU drawing-preparation timings, not display FPS. That nightly was `ae5ec46`,
**not the actual v0.9.1 release**.

We also rebuilt and measured the exact stable tag, **v0.9.1 (`167d893c`)**, against
**`ee2ce1a`**, the current measured implementation. That matched comparison is
separate below: four-gallery drag measured **557.14 → 3.75 ms on Mac** and
**1,114.11 → 3.13 ms on Windows**. Later review fixes and documentation commits are
not presented as additional measured checkpoints.

The [complete CSV](data/measurements.csv), [run manifest](data/manifest.json) and
[benchmark output logs](data/logs/) preserve all reported workload rows, sample
counts, median/p95 values and available means. The earlier reports remain linked
for implementation and regression-test details; their slower starting points
have not been replaced by a later optimized baseline.

## Which revisions were measured

| Checkpoint                        | Source                             | Scope and evidence                                                                                                                                                      |
| --------------------------------- | ---------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Stable v0.9.1                     | `167d893c` / `v0.9.1`              | Fresh matched canvas, editing, inspector and synchronous autosave runs.                                                                                                 |
| Original investigation baseline   | `ae5ec46`                          | Published pre-optimization nightly `nightly-0.9.1-nightly.20260929.36525615302.1`; the starting point in [loaded-canvas profiling](loaded-canvas.md).                   |
| Stage 1: loaded canvas            | `f88d2c4`; documentation `c9f2c25` | Scene/text caching, caption layout, rigid drag reuse and scene-index calculations.                                                                                      |
| Stage 2: editing and labels       | `f31ceea`; documentation `805b9fb` | Immediate label workers, per-molecule cache, indexed validation/grouping, shared dimension calculation; [editing report](editing-latency.md).                           |
| Stage 3: background work and zoom | `e9818f5`                          | Ordered recovery/file/library workers, inspector construction, selection decorations and exact-size glyph caching; [background/zoom report](background-io-and-zoom.md). |
| Current measured implementation   | `ee2ce1a`                          | Includes subsequent attachment-label and tilted-double-bond fixes; fresh stable/current rerun.                                                                          |

These are recorded checkpoints, not measurements of every intervening commit.
Some original Mac candidate runs preceded the final stage commit; the manifest
identifies them as working-tree runs of that stage. Documentation-only hashes do
not imply another measured runtime change.

## Original nightly through all optimization stages

The complete original-to-stage-3 canvas series is shown here so the starting cost
is visible. Values are **median milliseconds**, four copies of the shortcut
gallery; the CSV and [detailed original tables](loaded-canvas.md#measurements)
include p95 and the one-gallery measurements.

| Canvas checkpoint          | Mac drag all | Mac zoom | Windows drag all | Windows zoom |
| -------------------------- | -----------: | -------: | ---------------: | -----------: |
| Original nightly `ae5ec46` |       572.82 |   467.03 |         1,182.48 |     1,027.72 |
| Stage 1                    |        16.04 |    55.71 |            23.11 |       132.31 |
| Stage 2 baseline rerun     |        18.13 |    47.25 |            23.97 |       137.16 |
| Stage 3                    |         4.11 |    24.25 |             6.86 |        45.93 |

Stage 2 primarily changed editing/chemistry, so the canvas rerun is not a claim
of a new canvas improvement. Differences between unchanged paths can reflect
run variability. Historical Windows runs used automatic headless renderer
selection without recording the selected backend. Do not pool them with the
explicitly matched Windows renderer series below or divide across those series
to claim a hardware-independent speedup.

The editing series also retains its starting point. The four-gallery **O/C edit
pair** improved from **39.39 → 13.29 ms on Mac** and **59.84 → 16.04 ms on Windows**
from stages 1 to 2. This timing covers edit dispatch, not visible OH completion.
The separate shortcut label polling delay of up to 250 ms was removed; label workers
avoid full identifier/property analysis and reject stale results. Per-molecule
cold/cache-hit and worker-round-trip rows are all retained in the dataset.

## Matched actual stable release versus current

Measured 2026-09-29 in optimized release builds. Mac: Apple M4, 32 GiB RAM,
macOS 26.5.1, default wgpu build. Windows: AMD Ryzen 9 7940HS Windows 11 test VM,
MSVC build. Both Windows canvas executables explicitly use **tiny-skia**, because
v0.9.1 did not enable the Windows wgpu renderer. This comparison controls the
renderer rather than attributing a renderer change to the optimizations; it does
not measure the physical Intel Iris Xe GPU.

Canvas has three warmups and 20 measured iterations. Editing/inspector/recovery
have three warmups and 30 measured iterations. All table values below are
**median / p95 milliseconds**. The full dataset includes every other workload and
selection combination, including rows that did not improve.

<!-- MATCHED_TABLES -->

### Mac

| Workload                               |   Stable v0.9.1 | Current `ee2ce1a` |
| -------------------------------------- | --------------: | ----------------: |
| Drag all, 1×                           | 110.74 / 121.25 |       0.97 / 0.98 |
| Zoom, 1×                               |   89.23 / 95.72 |      9.12 / 11.98 |
| Unselected redraw, 4×                  | 325.83 / 378.37 |     13.74 / 14.17 |
| Selected redraw, 4×                    | 462.62 / 515.81 |     12.66 / 13.05 |
| Pointer/handle query, 4×               | 104.06 / 114.39 |       0.07 / 0.07 |
| Drag all, 4×                           | 557.14 / 618.70 |       3.75 / 3.80 |
| Drag two atoms, 4×                     | 288.02 / 364.78 |     27.52 / 27.82 |
| Pan, 4×                                | 458.94 / 499.85 |     12.73 / 14.14 |
| Zoom, 4×                               | 452.97 / 499.09 |     19.88 / 22.24 |
| O/C edit pair, 4×                      |   34.78 / 35.49 |     12.42 / 12.66 |
| Drawing validation, 4×                 |     5.25 / 5.79 |       0.90 / 0.92 |
| Selection grouping, 4×                 |     7.47 / 7.49 |       0.46 / 0.48 |
| Full view, one selected, collapsed, 4× |     2.96 / 2.98 |       0.49 / 0.50 |
| Full view, all selected, collapsed, 4× |   28.43 / 29.90 |       2.84 / 2.88 |
| Full view, all selected, expanded, 4×  |   28.48 / 28.83 |       4.61 / 4.70 |

### Windows

| Workload                               |     Stable v0.9.1 | Current `ee2ce1a` |
| -------------------------------------- | ----------------: | ----------------: |
| Drag all, 1×                           |   252.34 / 260.07 |       0.86 / 0.88 |
| Zoom, 1×                               |   225.46 / 236.11 |     30.26 / 47.25 |
| Unselected redraw, 4×                  |   722.97 / 736.99 |     14.10 / 15.65 |
| Selected redraw, 4×                    |  981.70 / 1055.24 |     13.99 / 14.95 |
| Pointer/handle query, 4×               |   158.42 / 164.74 |       0.07 / 0.09 |
| Drag all, 4×                           | 1114.11 / 1149.89 |       3.13 / 3.34 |
| Drag two atoms, 4×                     |   713.79 / 736.44 |     36.74 / 38.68 |
| Pan, 4×                                |  989.86 / 1016.59 |     13.90 / 14.46 |
| Zoom, 4×                               |  998.17 / 1087.28 |     46.19 / 54.13 |
| O/C edit pair, 4×                      |     52.35 / 54.32 |     17.39 / 20.12 |
| Drawing validation, 4×                 |       8.01 / 8.37 |       1.63 / 1.91 |
| Selection grouping, 4×                 |       6.88 / 7.15 |       0.58 / 0.83 |
| Full view, one selected, collapsed, 4× |       5.75 / 5.89 |       0.82 / 1.02 |
| Full view, all selected, collapsed, 4× |     34.81 / 36.30 |       4.11 / 6.47 |
| Full view, all selected, expanded, 4×  |     35.04 / 36.01 |       7.57 / 8.09 |

Small workloads do not all improve. For example, the Mac one-gallery, single-object expanded full view changed from 0.28 / 0.29 to 0.31 / 0.32 ms. These rows remain in the CSV alongside the larger-workload gains.

<!-- END_MATCHED_TABLES -->

## Autosave, files and what the numbers mean

The stable autosave Tick blocks until validation, serialization and its durable
write finish. The updated Tick captures/dispatches a snapshot and returns before
writing; a separate worker-round-trip row includes disk completion. Compare the
Tick rows for time occupying the UI thread, and read the round-trip rows for save
completion. They are different boundaries, not evidence that asynchronous disk
writes are inherently faster.

<!-- RECOVERY_TABLES -->

| Platform / size | Stable Tick, includes durable save | Current Tick dispatch | Current worker round trip |
| --------------- | ---------------------------------: | --------------------: | ------------------------: |
| Mac, 1×         |                        5.26 / 5.85 |           0.05 / 0.05 |               7.84 / 8.93 |
| Mac, 4×         |                      11.72 / 13.19 |           0.40 / 0.47 |              9.86 / 14.79 |
| Windows, 1×     |                        4.07 / 5.23 |           0.19 / 0.22 |               3.41 / 4.20 |
| Windows, 4×     |                      15.32 / 16.77 |           0.70 / 0.75 |               7.59 / 9.19 |

<!-- END_RECOVERY_TABLES -->

The current file/library benchmark measures parsing/validation, serialization,
loading 512 templates and checked library writes independently. These operation
bodies moved off the event loop. Their full current results and the earlier
stage-3 results are in the CSV; no before/after file/library ratio is claimed
because that harness was not run on the stable tag. The 300-file theme-loading
trial measured **5.45 → 6.36 ms**, regressed, and was reverted. Its two rows are
explicitly marked `rejected_experiment`.

## Reproduction and limits

The [dataset guide](data/README.md#reproduction) includes the benchmark commands,
renderer selection and [benchmark-only stable patch](data/v091-harness.patch).
The patch exposes/adds test harnesses and does not apply the runtime performance
fixes to the baseline. Source revisions, normalization and original workload
labels are recorded in the manifest. Logs contain aggregate summaries; original
per-iteration samples were not emitted and cannot be reconstructed.

The canvas fixture contains 483 atoms, 429 bonds, 132 captions, 16 abbreviations
and one arrow; the larger fixture appends four copies. Editing adds a two-atom
fragment and inspector testing adds one oxygen. Canvas uses a 1,066 × 570 viewport
and starts at 50% zoom. CPU/elapsed preparation times exclude GPU presentation
and display latency. Do not add independent medians and call the sum measured
input latency.

Large partial-selection drags still rebuild a changed scene, and four-gallery
zoom still exceeds a 16.7 ms budget on measured configurations. Remaining profiles
point to outline preparation, repeated hover nearest-bond queries and ring
geometry construction. Snapshot copying, saved-state comparison, publishing/fitting
loaded drawings, startup library discovery and theme persistence retain synchronous
work. These limits remain visible alongside the gains; no universal 60 FPS claim
is made. Validation evidence, exact glyph/pixel comparisons and async ordering
regressions are recorded in the three linked stage reports.
