# Contributors

Thank you to all the contributors who have helped improve ReShiki. Your contributions are greatly appreciated.

This list credits authors of contributions to ReShiki.

- @Ameyanagi — project creator and maintainer; contributed ordinary-Copy ChemDoodle Open/Load reaction handoff in [PR #144](https://github.com/Ameyanagi/ReShiki/pull/144), Rust 3D optimization, depth appearance, and default keyboard drawing in [PR #146](https://github.com/Ameyanagi/ReShiki/pull/146), bounded 3D initialization in [PR #149](https://github.com/Ameyanagi/ReShiki/pull/149), editable embedded ChemDraw paste from Windows Office in [PR #150](https://github.com/Ameyanagi/ReShiki/pull/150), and signing and privacy policies in [PR #151](https://github.com/Ameyanagi/ReShiki/pull/151). Signing-policy wording clarified in [PR #152](https://github.com/Ameyanagi/ReShiki/pull/152); release CI shard coverage and Windows reference-fixture UTF-8 handling contributed in [PR #158](https://github.com/Ameyanagi/ReShiki/pull/158).
- @HiroYokoyama — proposed improved benzene fusion and Kekulé-pattern selection, with regression tests and follow-up review fixes, in [PR #40](https://github.com/Ameyanagi/ReShiki/pull/40), and added Ctrl/Cmd drag to copy and Shift drag to lock an axis in [PR #87](https://github.com/Ameyanagi/ReShiki/pull/87).

Release preparation and documentation for 0.11.0: @Ameyanagi in [PR #185](https://github.com/Ameyanagi/ReShiki/pull/185).

Public-download validation for 0.11.0: @Ameyanagi in [PR #187](https://github.com/Ameyanagi/ReShiki/pull/187).

Windows SignPath signing and internal test-signed nightly integration: @Ameyanagi in [PR #270](https://github.com/Ameyanagi/ReShiki/pull/270).

Merged, unreleased: @Ameyanagi — checked numbered ChemDraw attachment import and original silicon fixtures for [issue #247](https://github.com/Ameyanagi/ReShiki/issues/247) in [PR #284](https://github.com/Ameyanagi/ReShiki/pull/284), with [Mac File Open/native save/reopen and controlled Windows ChemDraw/PowerPoint clipboard evidence](changes/chemdraw-numbered-paste.md).

MOL source3D import and ordinary-H presentation (merged, unreleased): @Ameyanagi; [review and desktop evidence](changes/mol-import-3d-review.md), [PR #282](https://github.com/Ameyanagi/ReShiki/pull/282) · [Integration PR #296](https://github.com/Ameyanagi/ReShiki/pull/296).

Merged, unreleased: @Ameyanagi — bounded Windows EMF picture import, retained source and original controlled spectrum fixtures for [issue #64](https://github.com/Ameyanagi/ReShiki/issues/64) in [PR #285](https://github.com/Ameyanagi/ReShiki/pull/285), with [Mac portable native save/resize/reopen and controlled Windows import/Word/PowerPoint native acceptance evidence](changes/emf-picture-import.md) · [Integration PR #296](https://github.com/Ameyanagi/ReShiki/pull/296).

Projected fullerene double-bond rendering and matched desktop/export review (merged, unreleased): @Ameyanagi in [PR #271](https://github.com/Ameyanagi/ReShiki/pull/271).

Rear-side opacity and view-occluded cage visibility with preserved molecular data (merged, unreleased): @Ameyanagi in [PR #289](https://github.com/Ameyanagi/ReShiki/pull/289); [current review](changes/rear-visibility-20261010.md), following [PR #271](https://github.com/Ameyanagi/ReShiki/pull/271).

Independent mechanism-arrow curvature (merged, unreleased): @Ameyanagi in [PR #276](https://github.com/Ameyanagi/ReShiki/pull/276) for [issue #91](https://github.com/Ameyanagi/ReShiki/issues/91), with [matched desktop evidence](changes/mechanism-curvature.md).

Connected pen paths and node/tangent editing (merged, unreleased): @Ameyanagi in [PR #288](https://github.com/Ameyanagi/ReShiki/pull/288) for [issue #67](https://github.com/Ameyanagi/ReShiki/issues/67), with [matched desktop evidence](changes/tunable-pen-lines.md).

Optional mechanism-arrow target attachments (merged, unreleased): @Ameyanagi for [issue #92](https://github.com/Ameyanagi/ReShiki/issues/92), with [matched desktop evidence and native readbacks](changes/mechanism-attachments.md). [PR #286](https://github.com/Ameyanagi/ReShiki/pull/286); [combined macOS native review](changes/drawing-tools-integration.md) passes, while current combined Windows native acceptance remains pending.

Exact reference alignment, shared-pivot copies and branch-only bond stretching:
@Ameyanagi in [PR #278](https://github.com/Ameyanagi/ReShiki/pull/278), merged, unreleased;
[review evidence](changes/reference-geometry.md).

Chemically defined symmetric free-base porphine template and supplied-seed
construction: @Ameyanagi, merged, unreleased;
[review evidence](changes/porphine-core.md). [PR #287](https://github.com/Ameyanagi/ReShiki/pull/287).

Drawing-tool defaults, orbital snapping and label readability: @Ameyanagi,
merged, unreleased; original review in [PR #277](https://github.com/Ameyanagi/ReShiki/pull/277) for
[#250](https://github.com/Ameyanagi/ReShiki/issues/250),
[#90](https://github.com/Ameyanagi/ReShiki/issues/90) and
[#249](https://github.com/Ameyanagi/ReShiki/issues/249).
[Visual review](drawing-tool-defaults-review.md).

Offline atom-linked HOSE NMR prediction, compact palette that preserves drawing space, measured-data provenance and native desktop validation (merged, unreleased): @Ameyanagi in [PR #272](https://github.com/Ameyanagi/ReShiki/pull/272).

The drawing and NMR contributions above were merged, unreleased, in [Integration PR #297](https://github.com/Ameyanagi/ReShiki/pull/297). Its [combined macOS native results](changes/drawing-tools-integration.md) cover the floating NMR report, exposed cage rim, attached cubic-arrow Stretch and save/fresh-process reopen. Original branch evidence remains historical; current combined Windows native acceptance remains pending.

Local Rust chemical naming and editor integration (under review): @Ameyanagi. The separate parser is introduced in [PR #291](https://github.com/Ameyanagi/ReShiki/pull/291); the [guide and native review](chemical-naming-rust.md) describe the bounded app workflow.

Local Rust editor naming and molecule-name captions (under review, unreleased): @Ameyanagi in [PR #292](https://github.com/Ameyanagi/ReShiki/pull/292) and [PR #293](https://github.com/Ameyanagi/ReShiki/pull/293); [Import dock and caption review](changes/molecule-name-label.md). The upstream-derived OPSIN parser retains its separate MIT license and attribution in [PR #291](https://github.com/Ameyanagi/ReShiki/pull/291).

Visible reaction atom maps, reviewed mapping and rigid alignment (under review, unreleased): @Ameyanagi in [PR #294](https://github.com/Ameyanagi/ReShiki/pull/294); [review and native evidence](reaction-atom-mapping-validation.md).

Retained `(+)-lactic acid` in the local naming dock (under review, unreleased): @Ameyanagi; [bounded retained-name correction and native evidence](changes/retained-plus-lactic-acid.md). This complete retained name resolves to the explicit S structure; it does not introduce a general optical-rotation-to-configuration rule.

The [GitHub contributor history](https://github.com/Ameyanagi/ReShiki/graphs/contributors) records merged commits. Individual [release notes](changes-0.11.md) credit contributors alongside their changes.
