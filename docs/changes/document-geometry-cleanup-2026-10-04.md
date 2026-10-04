# Document and geometry cleanup

This internal refactor removes repeated allocation, copying, and scanning from
document editing while preserving drawing output and controls. The branch is
`refactor/document-geometry-cleanup`; its comparison baseline is
`81ca82101061ecc201545a3cae8d8257f03254d3`. This note describes the core branch's
changes and validation recorded on 2026-10-04.

## Scope and preserved behavior

| Change                                             | Preserved behavior                                                                                                                                                                                                                                                                 |
| -------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Construct new/replacement bonds once               | Existing slot and endpoint orientation, endpoint guards and invalidation timing, five retained appearance fields, and reset chemical/display fields. Indicator defaults remain lazy.                                                                                               |
| Iterate drawable IDs internally                    | Atoms → annotations → arrows → graphics order. Public `all_ids()` still collects a vector; next-ID calculation additionally includes groups and retains saturation. Validation keeps duplicate-ID error precedence.                                                                |
| Iterate path-command points internally             | Move/Line/Cubic point order and the public `points()` vector API. Bounds retain the first-point min/max operation sequence, signed-zero and NaN behavior, empty-path fallback, control extents, and picture padding.                                                               |
| Reuse selection and distance helpers               | Expanded membership sets change membership lookups only; document traversal, output order, collapsed-group handling, and boundary stereo invalidation remain. Nearest bonds use the existing segment-distance implementation with unchanged strict radius and tie behavior.        |
| Reuse ring and template ownership                  | Connected rings move the owned preset and reuse its ID vector. Template comparison substitutes only the incoming identity and retains full derived equality, first matching template, favorite remapping, collision behavior, empty-library imports, and transactional validation. |
| Cache snap-ring target eligibility                 | Initialize lazily after the first usable source edge. Preserve source-edge order, target-bond order, both orientations, strict comparisons, midpoint/length/scoring operations, chemistry guards, and mutation order.                                                              |
| Use deques for History and recent-molecule context | Retain the 100-snapshot cap, chronological undo/redo, unchanged commits, continuous gestures, redo invalidation on a new branch, and recent-context epoch reset.                                                                                                                   |

The iterator consumers are document validation/ID allocation, editing selection,
joining membership checks, template object counts, graphic validation/bounds,
crossing strokes, rotation, arc construction, highlights, and scene geometry.
The two changes in `src/joining.rs` only substitute the drawable-ID iterator when
constructing membership sets. Public collecting APIs and the document schema
remain unchanged.

## Parity regressions

Ten regression tests were added across document/history tests, editing tests,
recent-molecule context, graphics, rings, and template libraries. Parameterized
cases include:

- **840 paired snap comparisons** against a frozen copy of the baseline scorer:
  serialized document snapshots, returned ID vectors, and atom-position bits.
  Cases include ring sizes 3–8, reversed encounter/orientation order, exact ties,
  changed chemistry, missing endpoints, overvalence, degenerate lengths, groups,
  NaN payloads, and strict radius boundaries.
- **432 graphic-bounds comparisons** across 36 kinds, three origins, and four
  paths, including empty/Close-only paths, control extents, skewed frames, large
  origins, signed zero, NaNs, and picture padding. Public point values/order/bits
  are also checked against the original collecting behavior.
- **28 connected-ring cases** covering seven presets, alternate settings, and
  explicit/default directions, with source-ID order and host immutability checks.
- Template comparisons that vary every other content field, first-match and
  favorite behavior, same-ID collisions, idempotent imports, and empty targets.
- Bond retained/reset fields and invalid-endpoint no-ops; ID saturation and error
  precedence; collapsed selection groups and boundary stereo/indicator behavior;
  the 101st history edit, undo/redo chronology, unchanged commits, continuous
  gestures, edit branches, and recent-context epoch changes.

## Recorded validation

Root ran the tests in a combined candidate containing this core work alongside
other independently scoped changes. The library/binary run passed **807 tests**
(314 library, 493 binary), with 60 ignored and no failures. The focused integration
run passed **106 tests across 18 targets**, with five ignored and no failures.
Both commands returned zero. These counts describe the recorded runs; they are
not a full repository suite, a standalone core-branch run, or a cross-platform
claim. Ignored tests were not executed.

```sh
cargo test --locked --lib --bin reshiki -- --test-threads=1
cargo test --locked \
  --test template_library --test graphics --test rings --test joining \
  --test bonds --test highlights --test selection_resize --test rotation_pivot \
  --test abbreviations --test attachments --test grouping --test reactions \
  --test template_style --test pictures --test figure_exports --test pages \
  --test font_export_103 --test picture_memory -- --test-threads=1
```

Formatting and `git diff --check` also passed. The logs and command receipts are
retained locally under `/tmp/reshiki-improvements-20261004/evidence` as
`combined-lib-bin.{log,json}` and `combined-core-images-integration.{log,json}`.
Those local artifacts are verification records, not permanent published assets.

## Mechanism and measurement limits

Production source has a net change of **−22 physical lines**. Tests and test
wiring add **714 lines**, for **+692 overall**, excluding this documentation.
The count includes formatting changes; it is not a semantic complexity metric.

Internal ID and point consumers avoid temporary vectors; graphic command
generation still creates its existing vector. Ring connection avoids an
owned-document clone, and each template comparison avoids a full template
document clone without introducing one for an empty destination. The library's
transactional clone remains. Selection sets allocate storage proportional to
expanded selection size in exchange for repeated membership scans. Snap caching
adds a target-pair vector and removes repeated endpoint/valence classification
across source edges. Deques avoid shifting snapshot headers at the cap; snapshot
clones and retained history remain.

No allocation counter or timing benchmark was run for this core change. These
mechanisms support reduced temporary work in the affected paths, but do not
establish lower process memory, faster interactions, or an application-wide
performance improvement. The parity regressions establish the tested cases;
they do not exhaust every possible document or non-finite input.
