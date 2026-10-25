# Contributors

Thank you to all the contributors who have helped improve ReShiki. Your contributions are greatly appreciated.

This list credits authors of contributions to ReShiki.

Under review: @Ameyanagi — readable long-chain imports with retained molecular
identity and stereochemistry in [PR #539](https://github.com/Ameyanagi/ReShiki/pull/539).
Under review: @Ameyanagi — moving 3D models from their visible frame edges in
[PR #540](https://github.com/Ameyanagi/ReShiki/pull/540).
Under review: @Ameyanagi — Safari pastes clipboard text into captions and Import fields without turning Command-V into a typed v.
[PR #541](https://github.com/Ameyanagi/ReShiki/pull/541).
Under review: @Ameyanagi — Crystal import notes stay with the drawing and survive native save/reopen; the guide now explains the opening view correctly.
[PR #542](https://github.com/Ameyanagi/ReShiki/pull/542).
Under review: @Ameyanagi — atom-label placement, readable controls and chemical
suggestions in [PR #543](https://github.com/Ameyanagi/ReShiki/pull/543).
Under review: @Ameyanagi — drawing-specific inspector state, fresh fragment-name drafts, and concise abbreviation confirmation in [PR #544](https://github.com/Ameyanagi/ReShiki/pull/544).
Under review: @Ameyanagi — chemical mark placement, reaction label spacing, visible terminal bonds, repeated chemical symbols and attached-arrow clearance in [PR #545](https://github.com/Ameyanagi/ReShiki/pull/545).
Under review: @Ameyanagi — Export dialogs name their format, browser progress and download receipts describe what happened, and the npm package has a complete HTML recipe with the correct release version.
[PR #557](https://github.com/Ameyanagi/ReShiki/pull/557) · [QA report](changes/qa-browser-polish.md).
Under review: @Ameyanagi — Crystal substitutions keep their cell and copies, update the drawn formula, and preserve unvalidated bond context after unsupported edits.
[PR #558](https://github.com/Ameyanagi/ReShiki/pull/558) · [QA report](changes/qa-crystal-substitution.md).
Under review: @Ameyanagi — Large peptide imports avoid repeated ring searches during label placement. Long-running Structure-input operations show elapsed time and can be cancelled without changing the drawing.
[PR #559](https://github.com/Ameyanagi/ReShiki/pull/559) · [QA report](changes/qa-508-import-performance.md).
Under review: @Ameyanagi — See the label the Text tool will repeat, keep its chemical meaning, and read clear feedback for generic and refreshed drawings.
[PR #560](https://github.com/Ameyanagi/ReShiki/pull/560) · [QA report](changes/qa-label-feedback.md).
Under review: @Ameyanagi — Reach 3D controls in one click, bring clipped models into view, recover directly from blocked scale commands, and cancel long native model generation with Escape.
[PR #561](https://github.com/Ameyanagi/ReShiki/pull/561) · [QA report](changes/qa-model-controls.md).

Released in 0.13.0: @Ameyanagi — searchable shortcut finder with live before/after
examples replacing Help in [PR #357](https://github.com/Ameyanagi/ReShiki/pull/357).

Released in 0.13.0: @Ameyanagi — Intel Mac panic recovery and native global-allocator
regression coverage in [PR #328](https://github.com/Ameyanagi/ReShiki/pull/328).

Released in 0.13.0: @Ameyanagi — consistent Direct3D 12 Windows
startup and graphics diagnostics in [PR #327](https://github.com/Ameyanagi/ReShiki/pull/327).
The [security investigation](release-0.12.0-windows-security.md) records the
startup comparison and unresolved installer antivirus detections.

Released in 0.12.0: @Ameyanagi — bounded Windows OLE clipboard acquisition and native
contention regression coverage in [PR #305](https://github.com/Ameyanagi/ReShiki/pull/305).
Included in [published Nightly 0.11.0-nightly.20261011.38123945065.1](https://github.com/Ameyanagi/ReShiki/releases/tag/nightly-0.11.0-nightly.20261011.38123945065.1),
with [passing CI and Windows portable checks](changes/windows-ole-clipboard-retry.md);
[Scoped GUI checks at 96 DPI / 100% scale](changes/nightly-windows-20261011.md) also pass; fresh Office and high or mixed DPI acceptance remain pending.

Coordination/Join, generic structures, adaptive chemistry, Assistant and
Windows/Office integration (released in 0.12.0):
@Ameyanagi in [PR #300](https://github.com/Ameyanagi/ReShiki/pull/300), including
the adaptive/Assistant source from [PR #299](https://github.com/Ameyanagi/ReShiki/pull/299).
The source-pinned Windows review-build route in [PR #301](https://github.com/Ameyanagi/ReShiki/pull/301)
is merged. The [published Windows Nightly review](changes/nightly-windows-20261011.md)
passes scoped arc, tab, ethanol naming and floating NMR interactions at 100% scale;
fresh Office and high or mixed DPI acceptance remain pending.
[Combined native review](changes/coordination-join-integration.md) records the
supported behavior and deferred Join/Co(en)3 accuracy work; issue #246 remains open.

- @Ameyanagi — project creator and maintainer; contributed ordinary-Copy ChemDoodle Open/Load reaction handoff in [PR #144](https://github.com/Ameyanagi/ReShiki/pull/144), Rust 3D optimization, depth appearance, and default keyboard drawing in [PR #146](https://github.com/Ameyanagi/ReShiki/pull/146), bounded 3D initialization in [PR #149](https://github.com/Ameyanagi/ReShiki/pull/149), editable embedded ChemDraw paste from Windows Office in [PR #150](https://github.com/Ameyanagi/ReShiki/pull/150), and signing and privacy policies in [PR #151](https://github.com/Ameyanagi/ReShiki/pull/151). Signing-policy wording clarified in [PR #152](https://github.com/Ameyanagi/ReShiki/pull/152); release CI shard coverage and Windows reference-fixture UTF-8 handling contributed in [PR #158](https://github.com/Ameyanagi/ReShiki/pull/158).
- @HiroYokoyama — proposed improved benzene fusion and Kekulé-pattern selection, with regression tests and follow-up review fixes, in [PR #40](https://github.com/Ameyanagi/ReShiki/pull/40), and added Ctrl/Cmd drag to copy and Shift drag to lock an axis in [PR #87](https://github.com/Ameyanagi/ReShiki/pull/87).

Release preparation and documentation for 0.11.0: @Ameyanagi in [PR #185](https://github.com/Ameyanagi/ReShiki/pull/185).

Public-download validation for 0.11.0: @Ameyanagi in [PR #187](https://github.com/Ameyanagi/ReShiki/pull/187).

Windows SignPath signing and internal test-signed nightly integration: @Ameyanagi in [PR #270](https://github.com/Ameyanagi/ReShiki/pull/270).

Nightly naming-helper configuration and regression coverage for all six release targets (released in 0.12.0): @Ameyanagi in [PR #304](https://github.com/Ameyanagi/ReShiki/pull/304).

Published Windows Nightly verification, actual screenshots and route/DPI receipts: @Ameyanagi in [PR #306](https://github.com/Ameyanagi/ReShiki/pull/306); [review and repeat steps](changes/nightly-windows-20261011.md).

Windows NMR surface shadow correction and matched native/partial-redraw regression coverage (released in 0.12.0): @Ameyanagi in [PR #310](https://github.com/Ameyanagi/ReShiki/pull/310); [review and reusable images](changes/windows-nmr-shadows.md).

Released in 0.12.0: @Ameyanagi — checked numbered ChemDraw attachment import and original silicon fixtures for [issue #247](https://github.com/Ameyanagi/ReShiki/issues/247) in [PR #284](https://github.com/Ameyanagi/ReShiki/pull/284), with [Mac File Open/native save/reopen and controlled Windows ChemDraw/PowerPoint clipboard evidence](changes/chemdraw-numbered-paste.md).

MOL source3D import and ordinary-H presentation (released in 0.12.0): @Ameyanagi; [review and desktop evidence](changes/mol-import-3d-review.md), [PR #282](https://github.com/Ameyanagi/ReShiki/pull/282) · [Integration PR #296](https://github.com/Ameyanagi/ReShiki/pull/296).

Released in 0.12.0: @Ameyanagi — bounded Windows EMF picture import, retained source and original controlled spectrum fixtures for [issue #64](https://github.com/Ameyanagi/ReShiki/issues/64) in [PR #285](https://github.com/Ameyanagi/ReShiki/pull/285), with [Mac portable native save/resize/reopen and controlled Windows import/Word/PowerPoint native acceptance evidence](changes/emf-picture-import.md) · [Integration PR #296](https://github.com/Ameyanagi/ReShiki/pull/296).

Projected fullerene double-bond rendering and matched desktop/export review (released in 0.12.0): @Ameyanagi in [PR #271](https://github.com/Ameyanagi/ReShiki/pull/271).

Rear opacity for view-occluded cage ink with preserved molecular data (released in 0.12.0): @Ameyanagi in [PR #289](https://github.com/Ameyanagi/ReShiki/pull/289); [current review](changes/rear-visibility-20261010.md), following [PR #271](https://github.com/Ameyanagi/ReShiki/pull/271).

Independent mechanism-arrow curvature (released in 0.12.0): @Ameyanagi in [PR #276](https://github.com/Ameyanagi/ReShiki/pull/276) for [issue #91](https://github.com/Ameyanagi/ReShiki/issues/91), with [matched desktop evidence](changes/mechanism-curvature.md).

Connected pen paths and node/tangent editing (released in 0.12.0): @Ameyanagi in [PR #288](https://github.com/Ameyanagi/ReShiki/pull/288) for [issue #67](https://github.com/Ameyanagi/ReShiki/issues/67), with [matched desktop evidence](changes/tunable-pen-lines.md).

Optional mechanism-arrow target attachments (released in 0.12.0): @Ameyanagi for [issue #92](https://github.com/Ameyanagi/ReShiki/issues/92), with [matched desktop evidence and native readbacks](changes/mechanism-attachments.md). [PR #286](https://github.com/Ameyanagi/ReShiki/pull/286); [combined macOS native review](changes/drawing-tools-integration.md) passes, while Windows acceptance for this attached-arrow case remains pending.

Exact reference alignment, shared-pivot copies and branch-only bond stretching:
@Ameyanagi in [PR #278](https://github.com/Ameyanagi/ReShiki/pull/278), released in 0.12.0;
[review evidence](changes/reference-geometry.md).

Chemically defined symmetric free-base porphine template and supplied-seed
construction: @Ameyanagi, released in 0.12.0;
[review evidence](changes/porphine-core.md). [PR #287](https://github.com/Ameyanagi/ReShiki/pull/287).

Fixed hydrogen-label reflections and rotations (released in 0.12.0): @Ameyanagi in
[PR #308](https://github.com/Ameyanagi/ReShiki/pull/308);
[matched native review and regression checks](changes/reflected-hydrogen-labels.md).

Drawing-tool defaults, orbital snapping and label readability: @Ameyanagi,
released in 0.12.0; original review in [PR #277](https://github.com/Ameyanagi/ReShiki/pull/277) for
[#250](https://github.com/Ameyanagi/ReShiki/issues/250),
[#90](https://github.com/Ameyanagi/ReShiki/issues/90) and
[#249](https://github.com/Ameyanagi/ReShiki/issues/249).
[Visual review](drawing-tool-defaults-review.md).

Offline atom-linked HOSE NMR prediction, compact palette that preserves drawing space, measured-data provenance and native desktop validation (released in 0.12.0): @Ameyanagi in [PR #272](https://github.com/Ameyanagi/ReShiki/pull/272).

The drawing and NMR contributions above were merged for 0.12.0 in [Integration PR #297](https://github.com/Ameyanagi/ReShiki/pull/297). Its [combined macOS native results](changes/drawing-tools-integration.md) cover the floating NMR report, exposed cage rim, attached cubic-arrow Stretch and save/fresh-process reopen. Original branch evidence remains historical. [Windows Nightly ethanol floating NMR controls](changes/nightly-windows-20261011.md) pass at 100% scale; the combined cage/attached-arrow/EMF scenario was not repeated on Windows.

Local Rust chemical naming and editor integration (released in 0.12.0): @Ameyanagi. The separate parser is introduced in [PR #291](https://github.com/Ameyanagi/ReShiki/pull/291); the [guide and native review](chemical-naming-rust.md) describe the bounded app workflow.

Local Rust editor naming and molecule-name captions (released in 0.12.0): @Ameyanagi in [PR #292](https://github.com/Ameyanagi/ReShiki/pull/292) and [PR #293](https://github.com/Ameyanagi/ReShiki/pull/293); [Import dock and caption review](changes/molecule-name-label.md). The upstream-derived OPSIN parser retains its separate MIT license and attribution in [PR #291](https://github.com/Ameyanagi/ReShiki/pull/291).

Visible reaction atom maps, reviewed mapping and rigid alignment (released in 0.12.0): @Ameyanagi in [PR #294](https://github.com/Ameyanagi/ReShiki/pull/294); [review and native evidence](reaction-atom-mapping-validation.md).

Retained `(+)-lactic acid` in the local naming dock (released in 0.12.0): @Ameyanagi; [bounded retained-name correction and native evidence](changes/retained-plus-lactic-acid.md). This complete retained name resolves to the explicit S structure; it does not introduce a general optical-rotation-to-configuration rule.

The local naming, caption and reaction-map contributions above were merged for 0.12.0
in [Integration PR #298](https://github.com/Ameyanagi/ReShiki/pull/298).
Their original review images retain their identified producers.

Curated offline chemical-name aliases (released in 0.12.0): @Ameyanagi in
[PR #309](https://github.com/Ameyanagi/ReShiki/pull/309). The finite catalog adds
40 provenance-labelled names for 30 exact identities, with separate systematic
name results, Copy and linked-caption actions, without a naming API or Java
runtime. [Qualified native review and reusable images](changes/offline-name-synonyms.md).

Multi-atom Join with retained survivor identity/style and fixed surrounding
atoms (released in 0.12.0): @Ameyanagi in [PR #295](https://github.com/Ameyanagi/ReShiki/pull/295),
included in [Integration PR #300](https://github.com/Ameyanagi/ReShiki/pull/300).
[Controls and restrictions](fragment-joining.md); selection/placement accuracy remains TODO.

Typed generic structures and finite expansion (released in 0.12.0): @Ameyanagi
in [PR #295](https://github.com/Ameyanagi/ReShiki/pull/295), included in
[Integration PR #300](https://github.com/Ameyanagi/ReShiki/pull/300).
[Supported scope](generic-structures.md) and [actual combined review](changes/coordination-join-integration.md)
retain the original variable-position layout defect and its saved example;
the correction was merged separately in [PR #302](https://github.com/Ameyanagi/ReShiki/pull/302).

Ring attachment placement, bounded chemical-product deduplication and native
review (released in 0.12.0): @Ameyanagi in
[PR #302](https://github.com/Ameyanagi/ReShiki/pull/302).
The [source-pinned review](https://github.com/Ameyanagi/ReShiki/blob/1259f4a357558079a2c8556a591d552118864606/docs/changes/generic-ring-expansion.md)
records the actual macOS producer, passing local model tests, native
Undo/Redo/save/fresh-process reopen and retained placement/work limits.
[Cross-platform CI](https://github.com/Ameyanagi/ReShiki/actions/runs/38110013391) passes.
Join/Co(en)3 accuracy remains TODO;
issue #246 stays open.

Office SVG picture export (released in 0.12.0, [PR #290](https://github.com/Ameyanagi/ReShiki/pull/290)): @Ameyanagi. [Review and evidence](changes/office-svg-placement.md).

Released in 0.12.0: Windows window fitting by @Ameyanagi in [PR #273](https://github.com/Ameyanagi/ReShiki/pull/273). The [published Nightly window](changes/nightly-windows-20261011.md) fits the measured work area at 96 DPI / 100% scale; high and mixed DPI remain pending.

Released in 0.12.0: Windows/Linux document-tab launches with per-document Office sessions by @Ameyanagi, originating in closed [PR #274](https://github.com/Ameyanagi/ReShiki/pull/274) and integrated through [PR #300](https://github.com/Ameyanagi/ReShiki/pull/300). [Windows new-file, duplicate-file and no-argument launches](changes/nightly-windows-20261011.md) pass at 100% scale. [Published Linux X11 runtime checks](changes/linux-tab-routing-20261011.md) ([PR #307](https://github.com/Ameyanagi/ReShiki/pull/307)) also pass request admission, window-title routing, cold-launch convergence and bounded fallback. Linux desktop, file-manager association and Office save-back acceptance remain pending.

Coordination attachment, in-place chelate closure and endpoint-independent dative
projection (released in 0.12.0; [PR #283](https://github.com/Ameyanagi/ReShiki/pull/283)):
@Ameyanagi, addressing
[#246](https://github.com/Ameyanagi/ReShiki/issues/246). See the
[desktop validation record](coordination-desktop-validation.md).

Adaptive calculation budgets and ordered fused-ring aromaticity enumeration
(released in 0.12.0; [PR #279](https://github.com/Ameyanagi/ReShiki/pull/279)):
@Ameyanagi, addressing [#245](https://github.com/Ameyanagi/ReShiki/issues/245)
and [#248](https://github.com/Ameyanagi/ReShiki/issues/248). See the
[desktop validation record](adaptive-chemistry-limits-validation.md).

Assistant setup and the original guided ethanol image exercise: @Ameyanagi,
project creator and maintainer; merged for 0.12.0 in
[PR #280](https://github.com/Ameyanagi/ReShiki/pull/280).
[Issue #93](https://github.com/Ameyanagi/ReShiki/issues/93) ·
[Native review and evidence](changes/assistant-setup-review-2026-10-09.md).

Assistant image benchmark, original synthetic fixture cohort and independent
reference-graph comparison: @Ameyanagi, project creator and maintainer; merged for 0.12.0
through [PR #300](https://github.com/Ameyanagi/ReShiki/pull/300),
originating in closed [PR #281](https://github.com/Ameyanagi/ReShiki/pull/281).
[Issue #94](https://github.com/Ameyanagi/ReShiki/issues/94) ·
[Method and measured results](assistant-benchmark.md).

Clearer controls and wording for the new features, ChemDraw-like atom labels, the
agent skill and the MOL and polymer fixes (released in 0.12.0): @Ameyanagi in
[PR #319](https://github.com/Ameyanagi/ReShiki/pull/319).

Release preparation and documentation for 0.12.0: @Ameyanagi in [PR #320](https://github.com/Ameyanagi/ReShiki/pull/320).

Public-download validation for 0.12.0: @Ameyanagi in [PR #324](https://github.com/Ameyanagi/ReShiki/pull/324).

Nix flake for Linux and Apple Silicon Macs (released in 0.13.0): @Stefanomarton in [PR #366](https://github.com/Ameyanagi/ReShiki/pull/366).

Release preparation and documentation for 0.13.0: @Ameyanagi.

The [GitHub contributor history](https://github.com/Ameyanagi/ReShiki/graphs/contributors) records merged commits. Individual [release notes](changes-0.13.md) credit contributors alongside their changes.

Space-filling hydrogen visibility: @Ameyanagi in [PR #535](https://github.com/Ameyanagi/ReShiki/pull/535) (merged); [validation and reusable evidence](changes/qa-513.md).

PDF gradient alignment for molecular and crystal models: @Ameyanagi in [PR #536](https://github.com/Ameyanagi/ReShiki/pull/536) (under review); [validation and reusable evidence](changes/qa-522.md).

Clear CLI errors for unsupported crystal input: @Ameyanagi in [PR #537](https://github.com/Ameyanagi/ReShiki/pull/537) (under review); [validation](changes/qa-490.md).
