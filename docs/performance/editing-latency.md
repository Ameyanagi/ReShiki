# Editing and chemistry-label latency

This extends the canvas work in [PR #84](https://github.com/Ameyanagi/ReShiki/pull/84).
The comparison baseline is `c9f2c25`, which already includes the first canvas
optimizations. This investigation targets the editor update thread and derived
hydrogen/stereochemistry labels, including the reported O → OH pause.

## Findings and implementation

An atom shortcut set a refresh deadline to now, but the event arrived through a
250 ms polling subscription. Ordinary chemistry edits used a 350 ms deadline,
which normally required two timer ticks. The refresh then ran full Analyze:
sanitization, drawing reconstruction, CIP, SMILES, InChI and descriptors. The
chemistry itself already used blocking workers; simply wrapping it in another
async function would not remove the scheduled delay or unnecessary work.

Label updates now start directly after an edit. They use the existing native
sanitization, drawing reconstruction and full CIP algorithms, without generating
identifiers or molecular descriptors. A separate background task calculates
properties when the Properties inspector is visible. Explicit Check, import,
export and clipboard operations retain their complete analysis paths.

A calculation receives an immutable document snapshot. There is at most one
label task in flight per window; intervening edits set a dirty flag, and the next
calculation uses the latest document. Applying a result requires both its file
epoch and edit revision to match. Old successes and errors cannot change newer
labels or notices. Only `label_h` and `cip_label` are applied: chemical input,
coordinates, selection and undo history are untouched.

Labels are calculated per connected component. Bond connectivity and attachment
membership keep dependent atoms together; persistent visual groups do not merge
independent molecules. An invalid/unsupported component cannot prevent labels
from updating on an independent ordinary molecule elsewhere on the page.

The cache uses exact normalized drawing equality, including atom IDs, elements,
charge, isotope, explicit hydrogen policy, radicals, coordinates, topology and
stereochemistry. Presentation fields and previous computed labels are excluded.
The next snapshot retains only its own components, up to 1,024 components,
8,192 atoms and 16,384 bonds. Excess components are calculated without caching.
Failed components are cached too, so an unchanged invalid structure is not
repeatedly sanitized. No process-global cache, mutex or unbounded work queue is
introduced. A running blocking calculation may finish after becoming stale;
its result is checked before publication.

A macOS sampling profile of the edit workload identified abbreviation validation
and selection grouping as substantial repeated work. The following changes keep
those operations synchronous while reducing their cost:

- Build one adjacency index for a batch of abbreviation checks; visit each
  abbreviation's own neighbors rather than repeatedly scanning the entire page.
- Index bond and group membership for selection grouping, preserving traversal
  order, partial-selection rules, nested groups and attachment membership.
- Index reaction-molecule traversal; hydrogen interactions still do not combine
  separate reaction participants.
- Apply computed labels through atom/bond maps rather than repeated linear
  searches, and build the surviving-ID set once when retaining selection.
- Measure selected bounds once for both width and height readouts.

## Reproduction

```sh
cargo test --release --locked --bin reshiki \
  app::performance::editing_workloads -- --ignored --exact --nocapture
```

Use `RESHIKI_INCHI_HELPER` pointing at the packaged helper for the full-analysis
baseline. The new label path itself does not invoke the helper. The benchmark
uses a two-atom C–O/C–C fragment alone, and alongside one or four appended copies
of `assets/examples/shortcut-examples.rsk`. There are three warmups and thirty
measured iterations; report median and p95 milliseconds. The hotkey workload is
an **O then C edit pair**, including application update and inspector readout
synchronization. It excludes task execution and rendering. Worker round-trip
timings separately include snapshot copying, worker dispatch, calculation and
applying labels, but exclude event-loop/display presentation. Cold-cache label
timings deliberately pass an empty cache on every iteration.

The baseline received only the test harness. Sampling used macOS `sample` on the
release test executable during this workload. Raw logs and samples are retained
in `artifacts/performance/` in the performance checkout; the results below are
not display FPS or instrumented key-to-photon latency.

## Mac measurements

Apple M4, 32 GiB RAM, macOS 26.5.1; release build, 2026-09-29.
Milliseconds, median / p95:

| Workload                    |   Baseline 1× |    Updated 1× |     Baseline 4× |      Updated 4× |
| --------------------------- | ------------: | ------------: | --------------: | --------------: |
| Validate drawing            | 0.389 / 0.404 | 0.192 / 0.203 |   4.827 / 5.213 |   0.891 / 0.922 |
| Group selection             | 0.335 / 0.339 | 0.105 / 0.114 |   7.476 / 7.502 |   0.460 / 0.515 |
| O/C edit pair               | 3.151 / 3.181 | 2.058 / 2.088 | 39.388 / 41.281 | 13.291 / 13.475 |
| Inspector view construction | 0.302 / 0.310 | 0.407 / 0.420 |   3.403 / 3.448 |   3.738 / 3.857 |

The new cache/worker costs, measured separately:

| Label operation                     | Two-atom fragment | 1× gallery + fragment | 4× gallery + fragment |
| ----------------------------------- | ----------------: | --------------------: | --------------------: |
| Cold cache                          |     0.018 / 0.023 |         3.503 / 3.550 |       15.112 / 15.394 |
| Only fragment changed               |     0.017 / 0.018 |         0.497 / 0.521 |         2.291 / 2.334 |
| Worker round trip, fragment changed |     0.025 / 0.028 |         0.602 / 0.635 |         2.493 / 2.627 |

Full analysis of the two-atom fragment took 1.84 ms median, compared with
0.025 ms for the new worker round trip, in addition to eliminating the 250 ms
shortcut polling delay. The baseline gallery label refresh was skipped because
of semantic attachments; a numerical whole-gallery before/after label ratio
would therefore be misleading. The new workload verifies that the independent
oxygen's labels can be computed on that page.

Inspector view construction did not improve and increased slightly in this run;
it remains an additional optimization target. The O/C benchmark holds a pending
label task after its warmup and measures edit dispatch/coalescing, not visible
label completion. Rendering is also separate; do not add medians and present
the sum as measured input-to-display latency.

## Windows measurements

AMD Ryzen 9 7940HS test VM, Windows 11, MSVC release build,
2026-09-29. These are CPU timings on the VM, not a measurement of the user's
physical Iris Xe GPU. Milliseconds, median / p95:

| Workload                    |   Baseline 1× |    Updated 1× |     Baseline 4× |      Updated 4× |
| --------------------------- | ------------: | ------------: | --------------: | --------------: |
| Validate drawing            | 0.593 / 0.678 | 0.257 / 0.286 |  8.545 / 10.595 |   1.157 / 1.493 |
| Group selection             | 0.423 / 0.454 | 0.135 / 0.149 |   6.656 / 6.901 |   0.495 / 0.506 |
| O/C edit pair               | 5.825 / 6.518 | 3.100 / 3.557 | 59.837 / 61.794 | 16.041 / 17.009 |
| Inspector view construction | 0.595 / 0.623 | 0.744 / 1.115 |   5.391 / 5.985 |   5.602 / 6.323 |

| Label operation                     | Two-atom fragment | 1× gallery + fragment | 4× gallery + fragment |
| ----------------------------------- | ----------------: | --------------------: | --------------------: |
| Cold cache                          |     0.025 / 0.026 |         6.624 / 7.476 |       24.111 / 25.167 |
| Only fragment changed               |     0.025 / 0.032 |         0.840 / 0.953 |         3.837 / 4.093 |
| Worker round trip, fragment changed |     0.059 / 0.062 |         1.283 / 1.394 |         5.540 / 6.653 |

The baseline's full two-atom analysis took 20.09 ms median; the candidate still
runs complete analysis in about 20.96 ms when requested. Immediate label updates
use the separate worker path instead. Source is commit `f31ceea`; archive
extraction retained older timestamps, so both crate roots were touched before
building to ensure Cargo actually rebuilt the candidate. The logs confirm the
new label workloads and new test cases ran.

## Validation

Regression coverage includes full-analysis parity for neutral/charged/radical,
isotopic, aromatic, tetrahedral and E/Z examples; exact cache invalidation;
unsupported gallery components; cache limits and eviction; stale success/error
results; document replacement with reused IDs; rapid edits; undo/redo; selection
and menu preservation; lazy property calculations; and existing grouping,
reaction, abbreviation and clipboard exchange suites. The final macOS release
suite passed 531 tests (14 explicitly ignored). All-target/all-feature Clippy
passed with warnings denied, as did all-target/all-feature compile checks and
formatting through the normal pre-commit hooks. The Windows application suite
passed 269 tests (12 explicitly ignored), and the final macOS renderer pixel
comparison passed. Actual desktop checks on the isolated release
application verified O → OH on the loaded gallery, rapid N/S/O replacements,
Undo to SH and Redo to OH, without changing surrounding drawings.

## Broader audit and follow-up

| Area                                                                                | Finding / disposition                                                                                                                                                                                                                            |
| ----------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Atom shortcuts and labels                                                           | Fixed scheduled delay, unnecessary full analysis and repeated calculations on unchanged molecules.                                                                                                                                               |
| Document validation / selection / reactions                                         | Fixed profiled adjacency scans and repeated lookup work; retain transaction validation.                                                                                                                                                          |
| Canvas, text and numeric readouts                                                   | Previous scene/text caches remain; removed duplicate dimension measurements. Large partial drags and zoom remain documented targets.                                                                                                             |
| Properties inspector                                                                | Calculate lazily while visible; coalesce pending work and reject stale selection/document results.                                                                                                                                               |
| Cleanup, native import/export, picture decoding, clipboard preparation and printing | Heavy paths already use task/blocking-worker boundaries. Retain their snapshot checks and chemistry/export contracts.                                                                                                                            |
| Autosave / recovery                                                                 | Recovery validation, serialization and atomic write currently run in the Tick handler. Candidate for a separate measured improvement; async writes need ordered save/clear operations so an old write cannot recreate a discarded recovery file. |
| File/template/theme I/O                                                             | Some reads, writes and library maintenance remain synchronous in handlers or inside async futures. Profile large files/libraries before adding workers; background futures alone do not make CPU work nonblocking.                               |
| High-order chemistry algorithms                                                     | Keep the validated chemistry rules. Avoid speculative algorithm changes or caches keyed only by atom count/IDs; those would miss charge, stereo and topology edits.                                                                              |

These follow-ups are findings, not claims that every operation now fits a frame
budget. The measured changes target common editing latency without changing the
chemical interpretation or adding a broad asynchronous persistence system.

The next pass implemented ordered background recovery, native file and template
library workers, lazy inspector sections, selection decoration reuse and shared
hinted glyphs. See [background I/O, inspector and zoom measurements](background-io-and-zoom.md)
for results, concurrency checks and the remaining limits. The table above records
the findings at the end of this editing-latency pass.
