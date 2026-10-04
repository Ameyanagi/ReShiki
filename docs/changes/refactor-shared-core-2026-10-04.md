# Shared core and rendering refactors

These seven internal refactors preserve existing controls, interchange formats
and drawing behavior. Five independent workers implemented and reviewed disjoint
areas. The comparison baseline is `ef6fe623`, whose production content was merged
into `main` by [PR #129](https://github.com/Ameyanagi/ReShiki/pull/129) at `80eed592`.

| Change                                 | Preserved behavior and evidence                                                                                                                                                                                                                         |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Reuse the CDXML palette decoder        | The existing expression and decoder have identical syntax trees. All 3,581 presentation-reference cases produced identical bytes, including 24 eager palette errors.                                                                                    |
| Share clipboard request encoding       | Move the same serializer and post-serialization size check ahead of each existing platform transport. Operations, field order, escaping, error text and the strict 128 MiB limit are unchanged.                                                         |
| Share pinned reference checkout setup  | Retain Windows LF initialization, exact fetch arguments and timeout, detached checkout, verification before publication, final verification and failure cleanup. Local Git tests verify commands, bytes and destination visibility.                     |
| Consolidate caption inspector controls | Expanded panel tokens and quoted strings match the original. Inline and ordinary panels retain their different row alignment and spacing. State tests cover paragraph drafts, invalid input, independent document edits and local versus document Undo. |
| Compute canvas cache routing once      | Preserve the existing eligibility expression after hidden-caption filtering. Marker and primitive cameras retain the negative translation delta; selection boxes use the original camera and then translate world coordinates.                          |
| Borrow rendering primitives            | Preserve primitive order, raster/vector layer boundaries, flipped pictures, paths, text styles, underlines and minimum strokes while removing cached primitive clones.                                                                                  |
| Batch Arrange bounds                   | Measure disjoint groups in one scene pass, retaining group order, absent-bound filtering, stable sorting and translation arithmetic. Empty groups retain no bounds work; a single group is still measured.                                              |

## Exact output comparisons

Before editing canvas or Arrange production code, the new characterization
harnesses captured the original implementation's output. Candidate runs read
those artifacts with capture flags unset. They do not contain a second copy of
the production algorithm.

- **336 complete Arrange documents match:** six fixtures, seven selection
  variants and all eight align/distribute actions. Fixtures include nested and
  overlapping group memberships, attachments, labels, highlights, captions,
  curved arrows, graphics, tied bounds and the bundled shortcut gallery.
- **20 RGBA images match exactly:** light/dark themes, mixed primitive layers,
  minimum stroke settings, idle/selected/hidden-caption states, whole and partial
  moves, hidden-caption moves, copy moves and resizing. Every case also compares
  its cold and warmed cache output. Images are 640 × 400 at zoom 1 on the same
  WGPU renderer and host fonts.
- Picture fixtures explicitly load upright and flipped handles before capture
  and retain their allocations. This removes asynchronous PNG initialization
  from the comparison; sample checks also prove that the pictures are visible.
- Existing rendered regressions separately cover cached versus fresh edits,
  committed dragging and selection markers with other cameras and viewports.

The CDXML reference output is 2,637,272 bytes with SHA-256
`3dd18ec4d36e89bbf5f380c6faec18130b9a6cafeafa37eb17467ecbbc29dcae`
on both implementations. Panel-source comparison preserves its quoted strings
and layout differences as well as its callback and keyboard-binding tokens.

## Local performance measurements

Measurements used Rust release builds on macOS 26.5.1, Apple M4 arm64. Each run
has three warmups and 30 measured iterations. The table gives the range of
medians across two runs per implementation. Align Left timings include cloning,
arranging and dropping the document.

| Workload                          | Original median, ms | Refactored median, ms |
| --------------------------------- | ------------------: | --------------------: |
| Shortcut gallery, Align Left      |         15.24–19.74 |             2.27–2.47 |
| Four gallery copies, Align Left   |       827.39–921.96 |           32.70–41.56 |
| 64 highlighted groups, Align Left |                1.28 |             0.20–0.24 |

The four-copy gallery contains 1,932 atoms, 1,716 bonds and 528 captions.
Canvas draw workloads were also run in original/candidate/candidate/original
order. Selected four-copy draw medians were 15.06–15.26 ms originally and
14.42–14.48 ms after refactoring; other workloads were mixed. These tests measure
CPU construction of canvas geometry, not GPU frame latency or FPS. Background
user workloads remained active, so small timing differences are not attributed
to these changes. The large Arrange improvement is supported separately by a
test verifying that highlighted-bond joins are constructed once across groups.

## Reproduction and review

The full local Rust suite passed 1,205 tests. Five targeted opt-in functional
tests also passed: the captured Arrange and pixel comparisons, both existing
canvas regressions, and the caption inspector's actual spacing, typing and Enter
events in inline and ordinary modes. The relevant Python suites passed 45
engine/CDX/style tests and 24 checkout/profile/reference-scope tests. Normal
commit hooks passed formatting, linting, type checks, Clippy and all-target,
all-feature compilation.

Run ordinary Rust regressions with `cargo test --locked --no-default-features`.
Run the local CPU workloads with
`cargo run --release --locked --no-default-features --example canvas_performance`.
The opt-in tests require a headless renderer:

```sh
RESHIKI_PERF_ITERATIONS=30 RESHIKI_PERF_RENDERER=wgpu \
cargo test --release --locked --no-default-features --bin reshiki \
  canvas::performance::loaded_canvas_workloads -- --ignored --exact --nocapture
cargo test --release --locked --no-default-features --bin reshiki \
  canvas::performance::cached_canvas_matches_fresh_edits_and_committed_drag -- --ignored
cargo test --release --locked --no-default-features --bin reshiki \
  canvas::performance::selection_markers_match_unculled_reference -- --ignored
cargo test --locked --no-default-features --bin reshiki \
  app::typography::caption_controls_tests::rendered::caption_controls_publish_spacing_width_and_enter_in_both_modes -- --ignored
```

For a fresh original/candidate characterization, install the harnesses on the
original production tree and capture there first. The original primitive call
uses `primitives.clone()`; the refactored call borrows `&primitives`. Use the same
renderer, fonts, fixtures and artifact directories in both trees. Set
`RESHIKI_ARRANGE_OUTPUTS` and `RESHIKI_CANVAS_PIXELS` to separate artifact
directories. Set `RESHIKI_ARRANGE_CAPTURE_BASELINE=1` and
`RESHIKI_CANVAS_CAPTURE_BASELINE=1` only for original captures, then unset them
for candidate comparison. The relevant filters are
`editing::arrange_tests::arrangement_matches_captured_baseline` and
`canvas::render_parity_tests::renderer_matches_captured_baseline`. The new
highlight-join construction test is intended for the refactored implementation.
Run each characterization filter explicitly, with its artifact-directory
environment variable set:

```sh
cargo test --release --locked --no-default-features --lib \
  editing::arrange_tests::arrangement_matches_captured_baseline -- --ignored --exact --nocapture
cargo test --release --locked --no-default-features --bin reshiki \
  canvas::render_parity_tests::renderer_matches_captured_baseline -- --ignored --exact --nocapture
```

Review the caption inspector's spacing, Wrap width (pt), Enter submission,
invalid draft retention and Undo in selected-caption and inline-edit modes.
For Arrange, compare alignment/distribution of mixed objects and nested groups.
For drawing, inspect pictures, selection outlines, dragging and resizing in both
canvas themes. These are focused reviewer checks; the automated comparisons
above already verify the captured cases.
