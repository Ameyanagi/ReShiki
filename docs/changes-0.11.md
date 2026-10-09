# ReShiki 0.11.0

ReShiki 0.11.0 adds interactive Rust 3D optimization, mouse and keyboard drawing
by default, persistent structure highlights, and editable paste of supported
embedded ChemDraw objects from Windows Office. It improves precise transforms,
font weights, clipboard handoff, template sizing, reaction joins and temporary
memory use. Installed applications continue to work locally without Python,
RDKit or uv.

**File compatibility:** drawings saved in 0.11.0 use document version **19** and
cannot be opened in ReShiki 0.10.0 or earlier. Older drawings still open. Keep a
separate copy before saving if you need to use an earlier release.

## Highlights

- Generate and relax 3D conformers with MMFF94, MMFF94s or UFF, pin atoms, and
  review an editable projection before Apply.
- Draw with a mouse/keyboard hotspot in Select or Lasso, navigate with arrows,
  and mark/connect atoms for ring closure; F8 restores classic arrow nudging.
- Recover usable starting geometry for C60 and difficult imported structures,
  or reuse validated existing XYZ.
- Add persistent atom/bond highlights and style contracted labels independently
  of their internal atoms.
- Paste supported embedded ChemDraw objects from Windows Office as editable
  atoms and bonds, and use scoped Copy as or ordinary-Copy text handoff.
- Use stable rotation centers, double-click handles for numeric transforms,
  physical drawing-style units, and preserved variable-font weights.

Previous release: [ReShiki 0.10.0](changes-0.10.md). The detailed changes below
retain contributor credits and links to their validation evidence.

## Current limits

- **C60 rendering:** floating double-bond strokes can still appear in projected
  C60 drawings. This display issue remains unresolved. Geometry recovery does
  not guarantee a global energy minimum; timed-out sampling can fall back to
  one conformer. The documented initial paclitaxel preview is not converged,
  and larger fullerenes such as C70 have not been validated here. This
  limitation describes 0.11.0; a Nightly rendering correction is
  [under review in PR #271](https://github.com/Ameyanagi/ReShiki/pull/271),
  with [matched evidence](projected-double-bonds-review.md).
- **Receiving chemical text:** the verified ordinary-Copy reaction route is
  ReShiki Cmd+C → ChemDoodle Open text field Cmd+V → Load on macOS. The tested
  direct canvas paste did not import the selection. CAS Draw's tested molecule
  route uses its text field, Add and Center Structure; direct CAS reaction/API
  integration is outside this release.
- **Office integrations:** the optional Microsoft 365 task pane is an MVP for
  sideloaded testing with real-host acceptance pending. LibreOffice's documented
  headless persistence and interchange checks do not establish all-platform
  desktop editing acceptance. The Windows embedded ChemDraw paste confirmation
  covers editable atoms and bonds; save/reopen, Undo/Redo and explicit picture
  paste remain separate checklist items.
- **Desktop review:** combined interface, accessibility and end-to-end timing
  acceptance remain open. Automated contracts and earlier focused desktop
  evidence do not establish every platform workflow. Memory measurements have
  documented latency tradeoffs and do not imply a universal speedup.
- **Signing and security:** Windows and Linux packages remain unsigned.
  SignPath Foundation approval and Windows signing setup are pending. Earlier
  Defender results cover exact development files; release-package, browser
  reputation and additional antivirus qualification remain separate.

## Detailed changes

Review checks for the completed allocation and correctness audit are listed in
[the audit review checklist](changes/audit-review-2026-10-04.md).
[PR #142](https://github.com/Ameyanagi/ReShiki/pull/142) · @Ameyanagi.
Validation wording corrections: [PR #143](https://github.com/Ameyanagi/ReShiki/pull/143) · @Ameyanagi.

- **Signing and privacy policies:** document the current signing
  status, proposed SignPath process, local processing, and optional online data
  handling. Windows downloads remain unsigned while the application and signing
  setup are pending.
  [Code signing policy](code-signing-policy.md) · [Privacy policy](privacy-policy.md)
  · [PR #151](https://github.com/Ameyanagi/ReShiki/pull/151) · @Ameyanagi.
  Follow-up wording correction:
  [PR #152](https://github.com/Ameyanagi/ReShiki/pull/152) · @Ameyanagi.

- **Embedded ChemDraw paste on Windows:** normal Paste reads
  supported chemical data from embedded Office objects instead of choosing
  their presentation image. Paste picture retains its explicit image behavior.
  [Validation and desktop review checklist](changes/windows-chemdraw-paste-2026-10-05.md)
  · [PR #150](https://github.com/Ameyanagi/ReShiki/pull/150) · @Ameyanagi.
  The user confirmed editable atom and bond paste from Office on Windows.

- **Recover difficult 3D starting geometry:** generate usable
  previews for C60 and difficult imported structures, or continue from validated
  existing XYZ, with the selected Rust force field. Bounded retries can reduce
  timed-out sampling to one conformer; complex molecules may need further
  relaxation. C60 is verified within a conservative cage envelope.
  [Before/after and review checks](changes/geometry-initialization-2026-10-05.md)
  · [PR #149](https://github.com/Ameyanagi/ReShiki/pull/149) · @Ameyanagi.

- **Interactive 3D geometry and keyboard drawing:** generate
  conformers with Rust MMFF94, MMFF94s, or UFF, relax around dragged or pinned
  atoms, and rotate an editable projection before Apply. Automatic rear fading
  can be frozen or cleared independently of the geometry. Cmd/Ctrl+Shift+D
  opens optimization. Select/Lasso enables a mouse/keyboard drawing hotspot
  by default, with arrow navigation and marked ring closure; F8 turns it off.
  [Workflow and supported chemistry](3d-keyboard-drawing.md)
  · [Review evidence](changes/3d-keyboard-review-2026-10-05.md)
  · [PR #146](https://github.com/Ameyanagi/ReShiki/pull/146)
  · [Documentation PR #147](https://github.com/Ameyanagi/ReShiki/pull/147)
  · [Issue #48](https://github.com/Ameyanagi/ReShiki/issues/48) · @Ameyanagi.

- **Chemical text alongside ordinary Copy:** prepare molecular
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
  · [PR #144](https://github.com/Ameyanagi/ReShiki/pull/144)
  · [Documentation PR #145](https://github.com/Ameyanagi/ReShiki/pull/145) · @Ameyanagi.

- **Persistent structure highlights and independent label styles:** place
  editable tint halos behind selected atom labels and bonds. Highlights stay
  with the structure through editing, Undo, native save and figure export.
  Contracted labels retain their own font, size, emphasis and foreground ink
  independently of their internal atoms; supported ChemDraw exchange retains
  the separate highlight and label styles.
  [Style and exchange details](drawing-styles.md#highlight-parts-of-a-structure)
  · [PR #122](https://github.com/Ameyanagi/ReShiki/pull/122) · @Ameyanagi.

- **Cascading context menus:** keep parent menus visible while navigating
  submenus, with pointer and keyboard navigation, scrolling and dismissal
  handled together.
  [PR #123](https://github.com/Ameyanagi/ReShiki/pull/123) · @Ameyanagi.

- **Accessible editor controls and text focus:** expose live editor controls
  through the native accessibility bridge on macOS and Windows, and keep
  drawing shortcuts behind focused text fields, label drafts and dialogs.
  Full keyboard and assistive-technology desktop acceptance remains open.
  [Acceptance matrix](changes/ui-declutter-validation.md)
  · [PR #124](https://github.com/Ameyanagi/ReShiki/pull/124) · @Ameyanagi.

- **Optional Microsoft 365 task pane:** prepare editable native drawings and
  PNG previews for Word, Excel and PowerPoint on macOS and Windows, using a
  local HTTPS companion and explicit insert/edit/copy commands. This is an
  MVP for sideloaded testing; real-host round trips remain pending. It requires
  separate setup and does not add double-click activation to the shared pane.
  [Requirements, setup and acceptance table](../integrations/office/README.md)
  · [PR #127](https://github.com/Ameyanagi/ReShiki/pull/127) · @Ameyanagi.

- **Signed macOS geometry admission:** preserve the signed app bundle while
  checking the geometry backend's parameter sources.
  [Backend distribution](geometry-backend.md)
  · [PR #148](https://github.com/Ameyanagi/ReShiki/pull/148) · @Ameyanagi.

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

- **Atom shortcut targeting and growth:** shortcuts recognize
  complete atom labels, dimethyl and saturated rings use a nitrogen target when
  its valence allows, and terminal carbonyls continue the carbon chain before
  placing oxygen. Selection shortcuts refresh their property fields.
  [Details](contextual-shortcuts.md)
  · [PR #121](https://github.com/Ameyanagi/ReShiki/pull/121) · @Ameyanagi.

- **Canvas preview memory:** Borrow unchanged previews and share cache reads while preserving exact renderer pixels; measured allocation savings and CPU limits are documented.
  [Validation and limits](changes/canvas-preview-cache-2026-10-04.md)
  · [PR #140](https://github.com/Ameyanagi/ReShiki/pull/140) · @Ameyanagi.

- **Handle shortcuts for precise transforms:** double-click a
  rotation, corner or edge handle to open the matching numeric field, ready to
  type. Opening a field leaves the drawing and Undo history unchanged.
  [Details](selection-transforms.md)
  · [PR #112](https://github.com/Ameyanagi/ReShiki/pull/112) · @Ameyanagi.

- **Reaction membership on joins:** Joining existing fragments preserves reaction roles, coefficients and references, and rejects conflicting participants atomically.
  [Validation and limits](changes/joining-reaction-membership-2026-10-04.md)
  · [PR #137](https://github.com/Ameyanagi/ReShiki/pull/137) · @Ameyanagi.

- **Stable selection rotation:** repeated rotations no longer
  shift asymmetric molecules such as Pyrrole. Keyboard, numeric and handle
  rotations share a stable center, including mixed drawing selections.
  [Details](selection-transforms.md) · [Issue #96](https://github.com/Ameyanagi/ReShiki/issues/96)
  · [PR #112](https://github.com/Ameyanagi/ReShiki/pull/112) · @Ameyanagi.

- **Reliable template-library saves:** release the save lock
  explicitly across success and error paths, including concurrent process
  launches, and distinguish contention from OS errors.
  [Evidence](changes/issue-work-2026-10-02.md#interchange-and-nonvisual-evidence)
  · [Issue #56](https://github.com/Ameyanagi/ReShiki/issues/56)
  · [PR #111](https://github.com/Ameyanagi/ReShiki/pull/111) · @Ameyanagi.

- **Picture and export memory:** Compact stored PNG buffers and reuse owned reflection and export rasters while preserving exact drawing output; measured import latency tradeoffs are documented.
  [Validation and limits](changes/image-export-memory-2026-10-04.md)
  · [PR #138](https://github.com/Ameyanagi/ReShiki/pull/138) · @Ameyanagi.

- **Variable-font export weights:** normal and bold labels keep
  their requested weight in SVG rendering and selectable-text PDFs.
  [Matched before/after](changes/issue-work-2026-10-02.md#variable-font-weight-103)
  · [Image](images/issue-work-2026-10-02/font-normal-after.png)
  · [Issue #103](https://github.com/Ameyanagi/ReShiki/issues/103)
  · [PR #116](https://github.com/Ameyanagi/ReShiki/pull/116) · @Ameyanagi.

- **Native clipboard transport:** Share ChemDraw alias buffers, release completed Wayland requests and wait for X11 events while preserving formats, pipe bounds and print behavior.
  [Validation and limits](changes/native-clipboard-transport-2026-10-04.md)
  · [PR #139](https://github.com/Ameyanagi/ReShiki/pull/139) · @Ameyanagi.

- **Safer native print and OLE code:** use safe macOS print-info
  construction and encode Windows object descriptors without reading raw struct
  memory. Native contract tests preserve the existing output layout.
  [Evidence](changes/issue-work-2026-10-02.md#interchange-and-nonvisual-evidence)
  · [Issue #107](https://github.com/Ameyanagi/ReShiki/issues/107)
  · [PR #113](https://github.com/Ameyanagi/ReShiki/pull/113) · [PR #114](https://github.com/Ameyanagi/ReShiki/pull/114) · @Ameyanagi.

- **Built-in template journal size:** Built-in templates inserted in empty space follow the current journal size; personal and attached templates keep their existing geometry.
  [Validation and limits](changes/template-journal-size-2026-10-04.md)
  · [PR #136](https://github.com/Ameyanagi/ReShiki/pull/136) · @Ameyanagi.

- **Physical drawing-style units:** enter dimensions in pt, mm
  or cm while stored styles keep their physical size. Incomplete input retains
  focus so it can be finished before applying.
  [Details](drawing-styles.md#settings)
  · [Dimension units image](images/issue-work-2026-10-02/style-units-1280.png)
  · [Native check](changes/issue-work-2026-10-02.md#physical-units-and-retained-input-focus-89)
  · [Issue #89](https://github.com/Ameyanagi/ReShiki/issues/89)
  · [PR #115](https://github.com/Ameyanagi/ReShiki/pull/115) · @Ameyanagi.

- **Native chemistry allocations:** Move owned requests, borrow serialization and ranking data, and share immutable codec defaults while preserving scientific outputs and bounded worker behavior.
  [Validation and limits](changes/native-chemistry-allocations-2026-10-04.md)
  · [PR #141](https://github.com/Ameyanagi/ReShiki/pull/141) · @Ameyanagi.

- **Manual SciFinder handoff:** follow verified molecule-import
  and bounded ChemDoodle reaction-search workflows using ReShiki's prepared
  structures. Search and query review remain in the browser.
  [Guide and limits](scifinder-handoff.md)
  · [Issue #95](https://github.com/Ameyanagi/ReShiki/issues/95)
  · [PR #120](https://github.com/Ameyanagi/ReShiki/pull/120) · @Ameyanagi.

- **Office and LibreOffice payload handling:** reuse drawing
  bytes and stored XML lengths while preserving document data, validation and
  recovery behavior. Office regressions and Linux headless Writer/Calc/Impress
  reopen, reload and Save As checks passed; desktop and cross-platform
  acceptance remains separate.
  [Validation and limits](changes/office-payload-copies-2026-10-04.md)
  · [PR #134](https://github.com/Ameyanagi/ReShiki/pull/134) · @Ameyanagi.

- **Editable LibreOffice drawings:** an optional extension stores
  native drawings and previews in ODT, ODS and ODP; Linux gains persistent
  multi-format clipboard transport. Three-platform headless persistence and
  exchange checks passed; desktop edit/paste acceptance remains separate.
  [Setup and validation](../integrations/libreoffice/README.md)
  · [Issue #109](https://github.com/Ameyanagi/ReShiki/issues/109)
  · [PR #118](https://github.com/Ameyanagi/ReShiki/pull/118) · @Ameyanagi.

- **Scoped Copy as:** choose picture, molecular or reaction
  formats for selected objects or the whole drawing. Stale preparation results
  cannot overwrite a newer ReShiki copy.
  [Formats and limits](clipboard.md)
  · [Menu image](images/issue-work-2026-10-02/copy-as-1040.png)
  · [Evidence and image provenance](changes/issue-work-2026-10-02.md#copy-as-and-compact-controls-110-78)
  · [Issue #110](https://github.com/Ameyanagi/ReShiki/issues/110)
  · [PR #119](https://github.com/Ameyanagi/ReShiki/pull/119) · @Ameyanagi.

- **Windows security evidence:** collect exact-file Defender
  scan results with artifact hashes, matched scan events and unchanged
  protection settings. Final development binaries passed; release-package and
  additional antivirus qualification remain open.
  [Evidence protocol](windows-security-evidence.md)
  · [Issue #62](https://github.com/Ameyanagi/ReShiki/issues/62)
  · [PR #117](https://github.com/Ameyanagi/ReShiki/pull/117) · @Ameyanagi.

- **Combined interface acceptance:** expand the shared regression
  fixtures and record targeted macOS checks for compact controls, transforms,
  arc editing and history. Final Windows, accessibility and performance gates
  remain open.
  [Acceptance matrix](changes/ui-declutter-validation.md)
  · [Compact Transform image](images/issue-work-2026-10-02/transform-1040.png)
  · [Issue #78](https://github.com/Ameyanagi/ReShiki/issues/78)
  · [PR #124](https://github.com/Ameyanagi/ReShiki/pull/124) · [PR #126](https://github.com/Ameyanagi/ReShiki/pull/126) · @Ameyanagi.

- **Stable-release reference checks:** assign the C60 and embedding integration
  targets to a live-reference shard and run the reference-protocol fixture in
  Python UTF-8 mode on Windows. These CI and test changes preserve the
  application runtime and the required release checks.
  [PR #158](https://github.com/Ameyanagi/ReShiki/pull/158) · @Ameyanagi.

- **Release preparation:** update the version, release documentation and
  publication highlights while retaining the merged application code and
  excluding the ongoing refactoring branches.
  [PR #185](https://github.com/Ameyanagi/ReShiki/pull/185) · @Ameyanagi.
