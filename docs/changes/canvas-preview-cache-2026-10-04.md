# Canvas preview ownership and cache retrieval

The canvas now borrows its preview document until an operation changes it.
Unchanged redraws, pan, and zoom avoid the previous eager document clone;
editing gestures still own the preview when their first mutation occurs.
Hidden captions still require a separate preview. Whole-document drags retain
the translated preview used by editing aids.

Scene-cache retrieval returns markers and primitives together, avoiding a
second document comparison and cache borrow for the same draw. Marker drawing
still precedes primitive drawing. Full document snapshots, selection
invalidation, and cache retention remain unchanged. The drawing-preview cache
also drops its redundant dark flag: document equality already includes the
canvas theme, and the viewport size remains a cache key.

These retained changes are C01, R25, and R24 from the audit. Production source
changes by **−5 lines** relative to
`81ca82101061ecc201545a3cae8d8257f03254d3`; added test instrumentation and
renderer cases are counted separately. Existing borrowed primitive slices and
shared-cache eligibility from PR #130 are retained.

## Final integrated validation

The post-C06-revert integrated candidate passed **64 exact captured RGBA
comparisons: 32 on WGPU and 32 on tiny-skia**. Each backend covered the existing
20-image primitive/gesture matrix and 12 additional text-placement cases.
The matrix checks selection, hidden captions, whole/partial/copy drags and
resize in light/dark themes. The added text cases cover rulers on/off,
fractional pan and zoom, multiple fonts/styles/scripts, emoji, and interleaved
picture/vector layers, with cold/warm and cached/fresh outline comparisons.
No pixel tolerance was relaxed.

These debug-profile captures use the same host fonts and fixtures and establish
exact fixture parity on that host, rather than cross-platform font identity.
They use the actual application renderer. Large raw captures remain outside Git.

All three final records, `canvas-final-wgpu.{log,json}`,
`canvas-final-tiny.{log,json}` and `canvas-final-performance.{log,json}`,
returned zero and identify the same source:

- Base HEAD: `81ca82101061ecc201545a3cae8d8257f03254d3`.
- Integrated diff SHA-256:
  `6d1ebed1392f5be339e174cca1a78f5756428777bd3422a44363b2f7d6e0d7d5`.
- The integrated source includes other approved core, image, template and B01
  changes; it is not a standalone canvas-branch measurement.

The final records include hashes of every changed source file. The cache and
both canvas test-source hashes match the standalone canvas worktree. All local
run records and baseline artifacts are under
`/tmp/reshiki-improvements-20261004/evidence`.

Rust formatting and `git diff --check` also passed on the final canvas worktree.
The final integrated all-target/all-feature Clippy run with `-D warnings`
passed at the same diff fingerprint. The combined library/binary suite passed
820 tests (327 library, 493 binary), including the three scene-cache tests, at
diff `3dbf9fa2ca8b14bd9df70ef4c262d77ce424263c4874d149f4aa2fa87e1b5f16`.
All four canvas source-file hashes in that unit record match the final renderer
records, although its complete integrated source fingerprint differs.
An earlier warmed/fresh drawing-preview theme/size test passed on tiny-skia;
its `candidate-canvas-cache` source included C06, so that separate run is
historical evidence rather than a final-source result.

## Final requested-allocation and CPU measurements

The same-host baseline and final integrated candidate used release-mode WGPU,
three warmups and 10 CPU iterations per workload, followed by a separate warmed
requested-allocation pass. The commands and allocation instrumentation are
identical. C06 is absent from these final results.

| Redraw        | Requested Rust bytes/draw, base → final | Peak additional requested bytes, base → final | Allocation calls/draw, base → final |
| ------------- | --------------------------------------- | --------------------------------------------- | ----------------------------------- |
| 1× unselected | 2,054,763 → 1,737,117                   | 1,005,350 → 687,704                           | 9,386 → 7,237                       |
| 1× selected   | 2,344,250 → 2,026,604                   | 1,136,422 → 818,776                           | 9,899 → 7,750                       |
| 4× unselected | 6,523,112 → 5,252,573 (−19.5%)          | 3,225,611 → 1,955,072 (−39.4%)                | 35,654 → 27,079                     |
| 4× selected   | 8,123,319 → 6,852,780 (−15.6%)          | 4,012,043 → 2,741,504 (−31.7%)                | 36,167 → 27,592                     |

Unchanged redraw, pan and zoom requested 317,646 fewer bytes/2,149 fewer calls
per draw at 1×, and 1,270,539 fewer bytes/8,575 fewer calls at 4×. This fixed
delta is consistent with removing the eager document clone. Other integrated
changes prevent attributing every measured result solely to the canvas work.
Whole-document drag allocation bytes, calls and peaks now equal baseline at
both sizes: the rejected C06 overhead is gone. Pointer-hit allocation remains
zero; retained-byte deltas are unchanged.

CPU construction times are variable. The final run has an unselected 4× redraw
regression and a smaller 4× pointer-hit median increase; both are included here.

| Workload            | 1× median ms, base → final | 4× median ms, base → final | 4× p95 ms, base → final |
| ------------------- | -------------------------- | -------------------------- | ----------------------- |
| Unselected draw     | 1.9407 → 1.6322            | 14.6149 → 15.6159 (+6.8%)  | 14.8796 → 15.8513       |
| Selected draw       | 1.9395 → 1.6249            | 14.6357 → 14.2750          | 14.7824 → 14.3132       |
| Pointer hit         | 0.0172 → 0.0149            | 0.0727 → 0.0741 (+1.9%)    | 0.0752 → 0.0751         |
| Whole-document drag | 1.1466 → 0.9819            | 3.9036 → 3.8180            | 3.9596 → 3.9508         |
| Partial drag        | 3.0938 → 2.7208            | 31.8603 → 31.1430          | 32.6230 → 32.4577       |
| Pan                 | 2.0876 → 1.8439            | 14.8925 → 14.4359          | 14.9783 → 14.4888       |
| Zoom                | 9.5083 → 8.4789            | 22.3981 → 21.9757          | 24.9162 → 24.3462       |

This one paired 10-iteration run does not establish a general CPU speedup.
Peak values are maximum additional live requested Rust bytes over the warmed
allocation pass, not averages per draw. Native/GPU allocations, allocator
headers, RSS and FPS are outside the measurement. The fixture has no pictures,
so it cannot quantify the integrated image-buffer changes.

The baseline record is `baseline-canvas-performance.{log,json}` with source
diff SHA-256
`1ce0a1d715ce2cbdbf9600201f0f0929420ff82e496a49a9535fc963cce12930`.

## Rejected C06 text placement

C06 moved glyph-path translation into the frame transform. The earlier
`candidate-canvas-{wgpu,tiny}` captures passed 64 exact comparisons, but their
source included C06 and differs from the final records above. The historical
`candidate-canvas-performance` run added 96 requested bytes and one allocation
per whole-document drag draw.

In locked Iced WGPU, nonidentity-frame `fill` clones/transforms the path.
C06 replaced the existing explicit Lyon-path clone with the backend clone in
the measured rulers-off path and added a transform-stack allocation.
Tiny-skia and ruler-offset WGPU could avoid a copy, but no isolated measurement
established a useful benefit. C06 was reverted to the original text-placement
code; the broader exact-pixel tests remain. Final allocation measurements
confirm removal of the added overhead.

## Reproduction

Use matching baseline artifacts captured from unchanged production source with
the same harness, fonts, backend and fixture metadata. Set
`RESHIKI_CANVAS_CAPTURE_BASELINE=1` only for baseline capture; final comparisons
omit it. Final records used the following commands in the integrated worktree:

```sh
RESHIKI_CANVAS_PIXELS=/tmp/reshiki-improvements-20261004/evidence/canvas-wgpu-before cargo test --locked --bin reshiki matches_captured_baseline -- --ignored --nocapture --test-threads=1
RESHIKI_PERF_RENDERER=tiny-skia RESHIKI_CANVAS_PIXELS=/tmp/reshiki-improvements-20261004/evidence/canvas-tiny-before cargo test --locked --features iced/tiny-skia --bin reshiki matches_captured_baseline -- --ignored --nocapture --test-threads=1
RESHIKI_PERF_ALLOCATIONS=1 RESHIKI_PERF_ITERATIONS=10 cargo test --release --locked --bin reshiki canvas::performance::loaded_canvas_workloads -- --ignored --exact --nocapture --test-threads=1
cargo clippy --locked --all-targets --all-features -- -D warnings
```

The combined unit run used `cargo test --locked --lib --bin reshiki --
--test-threads=1` and the local `candidate-inchi-helper/reshiki-inchi-helper`
through `RESHIKI_INCHI_HELPER`. Its records are
`combined-full-lib-bin-fixed.{log,json}`; strict Clippy records are
`final-combined-clippy-fixed.{log,json}`.

The source contains three scene-cache unit tests for same-allocation document
edits, load/empty documents, undo/redo, selection changes and guide-layout
lifetime, plus the separate warmed/fresh preview theme/size renderer test.

## Release-note caption

Canvas redraws borrow unchanged drawing previews and share one cache lookup,
while preserving the drawing's appearance and editing behavior.
