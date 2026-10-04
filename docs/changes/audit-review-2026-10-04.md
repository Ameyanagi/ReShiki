# Reviewing the allocation and correctness audit

Start desktop review with Nature-sized templates, Office edit/cancel/reopen,
and mixed-picture exports plus Undo. Choose checks for the workflows you use;
these optional acceptance checks are separate from required CI and code review.

Status on 2026-10-04 15:38 UTC: PRs #132–#141 merged after passing CI;
valid Greptile comments are fixed and resolved.

## Scope and source budget

The behavior-preserving refactors total **−65 physical production lines**.
The two intentional correctness fixes add **55** and **12** lines separately.
Counts exclude tests, test-only wiring, QA/measurement harnesses and docs/assets.
Runtime helpers/declarations remain production; tests grow separately. LOC is not a performance metric.

| PR                                                    | Change                                         | Production LOC | Public validation                                      |
| ----------------------------------------------------- | ---------------------------------------------- | -------------: | ------------------------------------------------------ |
| [#132](https://github.com/Ameyanagi/ReShiki/pull/132) | Release/descriptor tooling                     |            −10 | [Evidence](release-descriptor-tooling-2026-10-04.md)   |
| [#133](https://github.com/Ameyanagi/ReShiki/pull/133) | Aromatic/properties/accessibility              |             −6 | [Evidence](app-property-accessibility-2026-10-04.md)   |
| [#134](https://github.com/Ameyanagi/ReShiki/pull/134) | Office/LibreOffice payloads                    |            −15 | [Evidence](office-payload-copies-2026-10-04.md)        |
| [#135](https://github.com/Ameyanagi/ReShiki/pull/135) | Document/geometry/history                      |            −22 | [Evidence](document-geometry-cleanup-2026-10-04.md)    |
| [#136](https://github.com/Ameyanagi/ReShiki/pull/136) | Journal-sized free templates (intentional fix) |            +55 | [Evidence](template-journal-size-2026-10-04.md)        |
| [#137](https://github.com/Ameyanagi/ReShiki/pull/137) | Reaction membership on joins (intentional fix) |            +12 | [Evidence](joining-reaction-membership-2026-10-04.md)  |
| [#138](https://github.com/Ameyanagi/ReShiki/pull/138) | Picture/export buffers                         |             −5 | [Evidence](image-export-memory-2026-10-04.md)          |
| [#139](https://github.com/Ameyanagi/ReShiki/pull/139) | Native clipboard/print transport               |             −2 | [Evidence](native-clipboard-transport-2026-10-04.md)   |
| [#140](https://github.com/Ameyanagi/ReShiki/pull/140) | Canvas preview/cache                           |             −5 | [Evidence](canvas-preview-cache-2026-10-04.md)         |
| [#141](https://github.com/Ameyanagi/ReShiki/pull/141) | Native chemistry allocations                   |              0 | [Evidence](native-chemistry-allocations-2026-10-04.md) |

PR #136's final count is +55, not its historical +59: integration with the
borrowed canvas preview removes four shared initializer lines. Its raw source
change is +811, including +756 tests/wiring. No scientific algorithm or oracle
was replaced; output, ordering, provenance and validation guards remain scoped
by each linked note's exact comparisons.

## Optional desktop checks

1. **Nature templates:** select built-in Cyclohexane before switching ACS→Nature,
   hover and insert into empty space; repeat with the journal change first.
   Preview and committed bonds should match the Nature reference (~31.49658
   world units). Check Undo/Redo and canceled placement. Personal templates keep
   saved geometry; connected/fused placement, toolbar rings and Move & attach
   keep their existing routes. The template note links the ACS fixture/figures.
2. **Office persistence:** edit an embedded drawing, cancel another edit, then
   save and reopen Word/PowerPoint or the supported Excel workflow. Confirm the
   expected drawing and host placement survive; stale edits must not overwrite
   current content. In Writer/Calc/Impress, also reload and try live Save As.
3. **Pictures and Undo:** use mixed text/vector/picture content, including alpha
   edges and rotated/reflected images; export PNG/SVG at unchanged settings.
   Check orientation, fonts, transparency and drawing history after Undo/Redo.
   Toggle theme/rulers and pan/zoom while selected; preview/markers stay current.
4. **Reaction joins:** when relevant, join existing participants with Connect,
   ShareAtom or FuseBond; roles/coefficients and arrow/caption references survive.
   Conflicting participants within one reaction must fail without partial edits.

## What the evidence establishes

Exact canvas comparisons cover WGPU and tiny-skia; the final Nature hover tests
exercise the actual Draw path and independently prepared reference geometry.
Real installed Linux LibreOffice/UNO checks cover Writer/Calc/Impress persistence,
reopen, reload and live Save As. They do not establish Windows/macOS LibreOffice
GUI or Microsoft desktop Office acceptance. Recorded macOS template QA covers
insertion/save and limited visible Undo/Redo, not every desktop workflow.
Public CI covers its documented OS/contract/native-test matrices; it does not
establish all-platform GUI, real printer, or end-to-end performance acceptance.

## Tradeoffs and withheld proposals

- **C06 was reverted:** pixel parity passed, but the measured path added an
  allocation without an independently demonstrated benefit. Final canvas
  evidence is post-revert; CPU timings remain mixed.
- **C08 schema:** retain 21,520 requested Rust bytes after first use to avoid
  repeated map construction. **C13 attachments:** the 4,096-atom modeled case
  retains a temporary 139,272-byte index and raises modeled peak from 327,680
  to 466,952 bytes; attachment-free exports retain their original transient set.
- Picture compaction reduces retained capacity with unchanged import peak and
  observed import-latency costs on several fixtures. Requested Rust bytes do
  not measure allocator internals, native memory or RSS; no broad speedup follows.
- Callback-cache/invalidation shortcuts, active-tab dispatch shortcuts, Windows
  decoded-print-image caching and the nightly-page parse visitor remain withheld.
  No weakened error/heap/work/checksum/source/license guard justifies a shortcut.
