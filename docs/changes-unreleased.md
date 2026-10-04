# Unreleased changes

Review checks for the completed allocation and correctness audit are listed in
[the audit review checklist](changes/audit-review-2026-10-04.md).
[PR #142](https://github.com/Ameyanagi/ReShiki/pull/142) · @Ameyanagi.
Validation wording corrections: [PR #143](https://github.com/Ameyanagi/ReShiki/pull/143) · @Ameyanagi.

See [ReShiki 0.10.0](changes-0.10.md) for the latest release notes.

- **Under review — Chemical text alongside ordinary Copy:** prepare molecular
  SMILES or copy-only reaction JSON without changing the drawing. The accepted
  reaction workflow is ReShiki Cmd+C → ChemDoodle Open text field Cmd+V → Load.
  October 5 Edge/macOS checks preserved both complete participants and their
  roles in the aromatic fixture, and imported the no-arrow two-ring SMILES
  through CAS Draw's text field, Add and Center Structure. Both complete rings
  were visible; no search was submitted. Direct canvas paste did not import the
  ReShiki selection in either tested editor; CAS reaction integration is outside
  this change.
  [Workflow and limits](scifinder-handoff.md)
  · [Fixture image](images/scifinder-handoff/benzene-hydrogenation.png)
  · [PR #144](https://github.com/Ameyanagi/ReShiki/pull/144) · @Ameyanagi.

- **Shared release checks and descriptor preparation:** share
  native target definitions, checksum writing and descriptor query operations
  while preserving supported platforms, checksums and generated chemistry data.
  [Validation](changes/release-descriptor-tooling-2026-10-04.md)
  · [PR #132](https://github.com/Ameyanagi/ReShiki/pull/132) · @Ameyanagi.

- **Lower temporary memory:** reuse numeric-transform candidates,
  consume owned picture buffers and release completed development-worker packets
  while preserving drawing output and editing behavior.
  [Measurements and exact-output checks](changes/memory-footprint-2026-10-04.md)
  · [PR #131](https://github.com/Ameyanagi/ReShiki/pull/131) · @Ameyanagi.

- **Document and geometry cleanup:** reuse internal drawing
  iterators, owned ring and template data, and bounded editing history while
  preserving drawing output, selection order, geometry and Undo behavior.
  [Validation](changes/document-geometry-cleanup-2026-10-04.md)
  · [PR #135](https://github.com/Ameyanagi/ReShiki/pull/135) · @Ameyanagi.

- **Shared import and rendering logic:** share palette parsing,
  clipboard encoding, reference checkout setup and caption controls; borrow
  canvas primitives and batch Arrange bounds while preserving existing output.
  [Validation and local measurements](changes/refactor-shared-core-2026-10-04.md)
  · [PR #130](https://github.com/Ameyanagi/ReShiki/pull/130) · @Ameyanagi.

- **Shared graphic, export, Office and release logic:** consolidate graphic-style
  controls, SVG parsing, Office revision guards and release checksum generation
  while preserving the existing controls, output settings and error messages.
  Automated regression checks and rendered graphic-control checks passed.
  [PR #129](https://github.com/Ameyanagi/ReShiki/pull/129) · @Ameyanagi.

- **Fewer temporary copies in editing and properties:** consume
  owned aromatic edit drawings, build one properties-cache key per subscription,
  and retain a smaller native accessibility action registry. Validation errors,
  cached failures and stale-action rejection preserve their existing behavior.
  [Validation and limits](changes/app-property-accessibility-2026-10-04.md)
  · [PR #133](https://github.com/Ameyanagi/ReShiki/pull/133) · @Ameyanagi.

- **Under review — Atom shortcut targeting and growth:** shortcuts recognize
  complete atom labels, dimethyl and saturated rings use a nitrogen target when
  its valence allows, and terminal carbonyls continue the carbon chain before
  placing oxygen. Selection shortcuts refresh their property fields.
  [Details](contextual-shortcuts.md) · @Ameyanagi.

- **Canvas preview memory:** Borrow unchanged previews and share cache reads while preserving exact renderer pixels; measured allocation savings and CPU limits are documented.
  [Validation and limits](changes/canvas-preview-cache-2026-10-04.md)
  · [PR #140](https://github.com/Ameyanagi/ReShiki/pull/140) · @Ameyanagi.

- **Under review — Handle shortcuts for precise transforms:** double-click a
  rotation, corner or edge handle to open the matching numeric field, ready to
  type. Opening a field leaves the drawing and Undo history unchanged.
  [Details](selection-transforms.md) · @Ameyanagi.

- **Reaction membership on joins:** Joining existing fragments preserves reaction roles, coefficients and references, and rejects conflicting participants atomically.
  [Validation and limits](changes/joining-reaction-membership-2026-10-04.md)
  · [PR #137](https://github.com/Ameyanagi/ReShiki/pull/137) · @Ameyanagi.

- **Under review — Stable selection rotation:** repeated rotations no longer
  shift asymmetric molecules such as Pyrrole. Keyboard, numeric and handle
  rotations share a stable center, including mixed drawing selections.
  [Details](selection-transforms.md) · [Issue #96](https://github.com/Ameyanagi/ReShiki/issues/96)
  · @Ameyanagi.

- **Under review — Reliable template-library saves:** release the save lock
  explicitly across success and error paths, including concurrent process
  launches, and distinguish contention from OS errors.
  [Evidence](changes/issue-work-2026-10-02.md#interchange-and-nonvisual-evidence)
  · [Issue #56](https://github.com/Ameyanagi/ReShiki/issues/56) · @Ameyanagi.

- **Picture and export memory:** Compact stored PNG buffers and reuse owned reflection and export rasters while preserving exact drawing output; measured import latency tradeoffs are documented.
  [Validation and limits](changes/image-export-memory-2026-10-04.md)
  · [PR #138](https://github.com/Ameyanagi/ReShiki/pull/138) · @Ameyanagi.

- **Under review — Variable-font export weights:** normal and bold labels keep
  their requested weight in SVG rendering and selectable-text PDFs.
  [Matched before/after](changes/issue-work-2026-10-02.md#variable-font-weight-103)
  · [Image](images/issue-work-2026-10-02/font-normal-after.png)
  · [Issue #103](https://github.com/Ameyanagi/ReShiki/issues/103) · @Ameyanagi.

- **Native clipboard transport:** Share ChemDraw alias buffers, release completed Wayland requests and wait for X11 events while preserving formats, pipe bounds and print behavior.
  [Validation and limits](changes/native-clipboard-transport-2026-10-04.md)
  · [PR #139](https://github.com/Ameyanagi/ReShiki/pull/139) · @Ameyanagi.

- **Under review — Safer native print and OLE code:** use safe macOS print-info
  construction and encode Windows object descriptors without reading raw struct
  memory. Native contract tests preserve the existing output layout.
  [Evidence](changes/issue-work-2026-10-02.md#interchange-and-nonvisual-evidence)
  · [Issue #107](https://github.com/Ameyanagi/ReShiki/issues/107) · @Ameyanagi.

- **Built-in template journal size:** Built-in templates inserted in empty space follow the current journal size; personal and attached templates keep their existing geometry.
  [Validation and limits](changes/template-journal-size-2026-10-04.md)
  · [PR #136](https://github.com/Ameyanagi/ReShiki/pull/136) · @Ameyanagi.

- **Under review — Physical drawing-style units:** enter dimensions in pt, mm
  or cm while stored styles keep their physical size. Incomplete input retains
  focus so it can be finished before applying.
  [Details](drawing-styles.md#settings)
  · [Dimension units image](images/issue-work-2026-10-02/style-units-1280.png)
  · [Native check](changes/issue-work-2026-10-02.md#physical-units-and-retained-input-focus-89)
  · [Issue #89](https://github.com/Ameyanagi/ReShiki/issues/89) · @Ameyanagi.

- **Native chemistry allocations:** Move owned requests, borrow serialization and ranking data, and share immutable codec defaults while preserving scientific outputs and bounded worker behavior.
  [Validation and limits](changes/native-chemistry-allocations-2026-10-04.md)
  · [PR #141](https://github.com/Ameyanagi/ReShiki/pull/141) · @Ameyanagi.

- **Under review — Manual SciFinder handoff:** follow verified molecule-import
  and bounded ChemDoodle reaction-search workflows using ReShiki's prepared
  structures. Search and query review remain in the browser.
  [Guide and limits](scifinder-handoff.md)
  · [Issue #95](https://github.com/Ameyanagi/ReShiki/issues/95) · @Ameyanagi.

- **Office and LibreOffice payload handling:** reuse drawing
  bytes and stored XML lengths while preserving document data, validation and
  recovery behavior. Office regressions and Linux headless Writer/Calc/Impress
  reopen, reload and Save As checks passed; desktop and cross-platform
  acceptance remains separate.
  [Validation and limits](changes/office-payload-copies-2026-10-04.md)
  · [PR #134](https://github.com/Ameyanagi/ReShiki/pull/134) · @Ameyanagi.

- **Under review — Editable LibreOffice drawings:** an optional extension stores
  native drawings and previews in ODT, ODS and ODP; Linux gains persistent
  multi-format clipboard transport. Three-platform headless persistence and
  exchange checks passed; desktop edit/paste acceptance remains separate.
  [Setup and validation](../integrations/libreoffice/README.md)
  · [Issue #109](https://github.com/Ameyanagi/ReShiki/issues/109) · @Ameyanagi.

- **Under review — Scoped Copy as:** choose picture, molecular or reaction
  formats for selected objects or the whole drawing. Stale preparation results
  cannot overwrite a newer ReShiki copy.
  [Formats and limits](clipboard.md)
  · [Menu image](images/issue-work-2026-10-02/copy-as-1040.png)
  · [Evidence and image provenance](changes/issue-work-2026-10-02.md#copy-as-and-compact-controls-110-78)
  · [Issue #110](https://github.com/Ameyanagi/ReShiki/issues/110) · @Ameyanagi.

- **Under review — Windows security evidence:** collect exact-file Defender
  scan results with artifact hashes, matched scan events and unchanged
  protection settings. Final development binaries passed; release-package and
  additional antivirus qualification remain open.
  [Evidence protocol](windows-security-evidence.md)
  · [Issue #62](https://github.com/Ameyanagi/ReShiki/issues/62) · @Ameyanagi.

- **Under review — Combined interface acceptance:** expand the shared regression
  fixtures and record targeted macOS checks for compact controls, transforms,
  arc editing and history. Final Windows, accessibility and performance gates
  remain open.
  [Acceptance matrix](changes/ui-declutter-validation.md)
  · [Compact Transform image](images/issue-work-2026-10-02/transform-1040.png)
  · [Issue #78](https://github.com/Ameyanagi/ReShiki/issues/78) · @Ameyanagi.
