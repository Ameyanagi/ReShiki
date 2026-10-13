# Unreleased changes

Nightly builds add chemical names, NMR prediction, generic structures, atom
maps, rear opacity, pen paths and arrows that attach to atoms, together with
Windows and Office improvements and an experimental agent API. None of these
changes are in ReShiki 0.11.0; see [ReShiki 0.11.0](changes-0.11.md) for the
latest release notes.

## File compatibility

Nightly builds save drawings in a newer format. ReShiki 0.11.0 cannot open them
and asks you to update. If you share files with people on 0.11.0, keep a copy
saved by 0.11.0.

## Highlights

- Type a chemical name to insert its structure, or label a molecule with its
  systematic name. Names are converted on your computer.
- Predict ¹H and ¹³C NMR shifts that link to the atoms of a molecule.
- Draw element lists, variable positions and repeating units, then expand them
  into every concrete structure.
- Map atoms between reactants and products, with **Auto-map** to suggest a
  mapping.
- Fade the back of 3D cages with **Rear opacity**, and keep 3D coordinates from
  MOL files.
- Draw pen paths, and mechanism arrows that follow the atoms, bonds and lone
  pairs they point at.

## Drawing and editing

- **Clearer controls for the new features.** The new panels use the same
  controls as the rest of the inspector, NMR opens under **Predict**, and
  status messages are shorter. **Chemical names…** in the right-click menu opens
  the names inspector, and clicking a dative bond again with the **Dative** tool
  swaps donor and acceptor, like **Reverse bonds**. Background jobs, menus and
  export notices also behave more reliably.
  [Writing and terms](writing-style.md) · @Ameyanagi.

- **Rear opacity.** Make the bonds behind the front face of a 3D molecule
  lighter in **Properties → 3D appearance → Rear opacity**, while the visible
  outline stays solid.
  [How to use](https://reshiki.com/guide/editing/) ·
  [Details](rear-opacity.md) ·
  [Review](changes/rear-visibility-20261010.md) ·
  [Image](images/rear-visibility-20261010/c60-25-after.jpg) ·
  [#289](https://github.com/Ameyanagi/ReShiki/pull/289) · @Ameyanagi.

- **Double bonds on tilted cages.** The second stroke of a double bond in a
  tilted C60 or other cage follows its ring face instead of floating. Molecular
  coordinates and properties do not change.
  [Review](projected-double-bonds-review.md) ·
  [#271](https://github.com/Ameyanagi/ReShiki/pull/271) · @Ameyanagi.

- **Reference align/stretch.** Turn a bond or shape edge to an exact angle,
  rotate copies around a shared center, or stretch one bond while the rest stays
  rigid, in **Properties → Transform → Reference: align / stretch**.
  [How to use](https://reshiki.com/guide/editing/) ·
  [Details](reference-geometry.md) ·
  [Review](changes/reference-geometry.md) ·
  [#278](https://github.com/Ameyanagi/ReShiki/pull/278) · @Ameyanagi.

- **Porphine template.** Place free-base 21H,23H-porphine from **Templates →
  Macrocycles**, with equal bonds and both N–H labels inward, or build the
  scaffold with reference align/stretch.
  [How to use](https://reshiki.com/guide/templates/) ·
  [Details](porphine-core.md) · [Review](changes/porphine-core.md) ·
  [#287](https://github.com/Ameyanagi/ReShiki/pull/287) · @Ameyanagi.

- **Hydrogen positions follow flips and rotations.** **Flip horizontal**,
  **Flip vertical** and rotation keep a fixed **Hydrogen position** on the
  matching side. Other angles take the nearest side, and an exact 45° diagonal
  picks Left or Right. **Automatic** stays automatic, label text stays upright,
  and porphine's N–H labels stay inward after a vertical flip.
  [Review](changes/reflected-hydrogen-labels.md) ·
  [Image](images/reflected-hydrogen-labels/after.png) ·
  [#308](https://github.com/Ameyanagi/ReShiki/pull/308) · @Ameyanagi.

- **Join three or more atoms.** **Join** (Ctrl+J, ⌘J on macOS) merges three or
  more selected atoms into the first one you selected, and **Merge atoms** also
  merges four atoms that form two bonds. Surrounding atoms stay in place.
  [How to use](https://reshiki.com/guide/editing/) ·
  [Details](fragment-joining.md) · [Review](changes/join-atom-merge.md) ·
  [#295](https://github.com/Ameyanagi/ReShiki/pull/295),
  [#300](https://github.com/Ameyanagi/ReShiki/pull/300) · @Ameyanagi.

- **Coordinate in place.** Connect donor atoms to a metal without moving
  anything with **Coordinate in place…**, or select a donor and a metal and
  press Ctrl+J (⌘J on macOS). Close a chelate one donor at a time, and show the
  contact as a wedge or hash.
  [How to use](https://reshiki.com/guide/editing/) ·
  [Details](fragment-joining.md#coordination-contacts-in-place) ·
  [Review](coordination-desktop-validation.md) ·
  [Shortcut review](changes/coordination-join-shortcut.md) ·
  [#283](https://github.com/Ameyanagi/ReShiki/pull/283) · @Ameyanagi.

- **Generic structures.** Keep element lists, variable attachment positions and
  repeating units in the drawing, then choose **Expand generic structures** to
  open every concrete structure in one new tab. Substituents attach outward at
  the normal bond length, and duplicate products are removed.
  [How to use](https://reshiki.com/guide/generic-structures/) ·
  [Details](generic-structures.md) ·
  [Review](changes/coordination-join-integration.md) ·
  [Ring placement review](https://github.com/Ameyanagi/ReShiki/blob/1259f4a357558079a2c8556a591d552118864606/docs/changes/generic-ring-expansion.md) ·
  [#295](https://github.com/Ameyanagi/ReShiki/pull/295),
  [#302](https://github.com/Ameyanagi/ReShiki/pull/302) · @Ameyanagi.

- **Pen paths.** Draw straight and curved segments as one object with **Pen
  path**, then move nodes and directions with **Edit curve points**.
  [How to use](https://reshiki.com/guide/figures/) · [Details](graphics.md) ·
  [Review](changes/tunable-pen-lines.md) ·
  [Image](images/tunable-pen-lines/pen-final-continued-clean.jpg) ·
  [#288](https://github.com/Ameyanagi/ReShiki/pull/288) ·
  [issue #67](https://github.com/Ameyanagi/ReShiki/issues/67) · @Ameyanagi.

- **Curved arrow ends.** Drag the square handles of a curved arrow to change how
  each end leaves or arrives, without moving the ends.
  [How to use](https://reshiki.com/guide/reactions/) ·
  [Details](arrows.md) · [Review](changes/mechanism-curvature.md) ·
  [Image](images/mechanism-curvature/after-output.jpg) ·
  [#276](https://github.com/Ameyanagi/ReShiki/pull/276) ·
  [issue #91](https://github.com/Ameyanagi/ReShiki/issues/91) · @Ameyanagi.

- **Arrows that attach to atoms.** With **Curved / electron pair** or **Single
  electron**, click a source atom, bond or lone pair and then a destination; the
  arrow follows its targets when they move. **Detach start** and **Detach end**
  release an end.
  [How to use](https://reshiki.com/guide/reactions/) ·
  [Details](arrows.md#optional-target-attachments) ·
  [Review](changes/mechanism-attachments.md) ·
  [Image](images/mechanism-attachments/mechanism-attachment-crowded-carbonyl-f8-off.jpg) ·
  [#286](https://github.com/Ameyanagi/ReShiki/pull/286), built on
  [#276](https://github.com/Ameyanagi/ReShiki/pull/276) and merged through
  [#297](https://github.com/Ameyanagi/ReShiki/pull/297) ·
  [issue #92](https://github.com/Ameyanagi/ReShiki/issues/92) · @Ameyanagi.

- **Drawing-tool defaults.** Chain tools have clearer icons and hints, orbitals
  have **Snap to atoms** (hold Option/Alt to place freely) and leave a gap
  around atom labels, and new lone pairs sit closer to their atom. Marks you
  placed yourself do not move.
  [Details](scientific-symbols.md) ·
  [Review](drawing-tool-defaults-review.md) ·
  [Image](images/drawing-tool-defaults/lone-pair-click-after.jpg) ·
  [#277](https://github.com/Ameyanagi/ReShiki/pull/277) · issues
  [#250](https://github.com/Ameyanagi/ReShiki/issues/250),
  [#90](https://github.com/Ameyanagi/ReShiki/issues/90),
  [#249](https://github.com/Ameyanagi/ReShiki/issues/249) · @Ameyanagi.

## Reactions

- **Atom maps.** Number corresponding atoms in the **Atom maps** section of the
  reaction inspector, or choose **Auto-map** to get a suggestion that you review
  before **Apply maps**. **Align mapped structures** lines the products up with
  the reactants.
  [How to use](https://reshiki.com/guide/reactions/) ·
  [Details](reactions.md#atom-mapping-and-label-placement) ·
  [Review](reaction-atom-mapping-validation.md) ·
  [#294](https://github.com/Ameyanagi/ReShiki/pull/294) · @Ameyanagi.

## Chemistry tools

- **Chemical names.** Type a name in **Import → Name** to insert its structure,
  or right-click a molecule and choose **Show chemical name**. Names are
  converted on your computer. **Import → Name → Details** also lists other names
  from a bundled offline catalog with their source and license: **Copy** copies
  one, and **Use below** captions the next insertion. **Generate selected name**
  in the same Details area names the selected molecule, and
  `(+)-lactic acid` reads as (S)-lactic acid.
  [How to use](https://reshiki.com/guide/names/) ·
  [Details](chemical-naming-rust.md) ·
  [Review](changes/rust-chemical-naming.md) ·
  [Dock and caption review](changes/molecule-name-label.md) ·
  [Alias review](changes/offline-name-synonyms.md) ·
  [(+)-Lactic acid review](changes/retained-plus-lactic-acid.md) ·
  [Image](images/chemical-naming-rust/ethanol-local-parse-preview.jpg) ·
  [Caption image](images/offline-name-synonyms/compact-caption.png) ·
  [#291](https://github.com/Ameyanagi/ReShiki/pull/291),
  [#292](https://github.com/Ameyanagi/ReShiki/pull/292),
  [#293](https://github.com/Ameyanagi/ReShiki/pull/293),
  [#309](https://github.com/Ameyanagi/ReShiki/pull/309) · @Ameyanagi.

- **NMR prediction.** Choose **Properties → Predict NMR…** to predict ¹H and ¹³C
  shifts for one molecule. Each value links to its atom, and the report floats
  beside the drawing or docks under **Predict → NMR**.
  [How to use](https://reshiki.com/guide/nmr/) ·
  [Details](nmr-prediction.md) · [Review](nmr-visual-review.md) ·
  [#272](https://github.com/Ameyanagi/ReShiki/pull/272) · @Ameyanagi.

- **Limits that fit your computer.** 3D work on larger drawings and imports of
  fused rings get calculation limits that scale with the free memory of your
  computer. A job that starts can still stop at its time limit.
  [Details](chemistry-resource-policy.md) ·
  [Review](adaptive-chemistry-limits-validation.md) ·
  [#279](https://github.com/Ameyanagi/ReShiki/pull/279) · issues
  [#245](https://github.com/Ameyanagi/ReShiki/issues/245),
  [#248](https://github.com/Ameyanagi/ReShiki/issues/248) · @Ameyanagi.

## Import and export

- **3D coordinates from MOL files.** Opening a MOL file with 3D coordinates keeps
  them, so you can tilt the molecule, and ordinary hydrogens become H counts.
  Save a `.rsk` file to keep the 3D shape; MOL export still writes a flat
  drawing.
  [Details](mol-import-3d.md) · [Review](changes/mol-import-3d-review.md) ·
  [#282](https://github.com/Ameyanagi/ReShiki/pull/282),
  [#296](https://github.com/Ameyanagi/ReShiki/pull/296) · @Ameyanagi.

- **ChemDraw numbered attachment points.** ChemDraw 26 files and pastes with
  numbered attachment points, such as SiMe₃ groups, open as editable structures
  instead of failing.
  [Review](changes/chemdraw-numbered-paste.md) ·
  [#284](https://github.com/Ameyanagi/ReShiki/pull/284) ·
  [issue #247](https://github.com/Ameyanagi/ReShiki/issues/247) · @Ameyanagi.

## Windows and Office

- **SVG for Office.** Export **SVG for Office** for Word and PowerPoint. Text
  becomes outlines, so labels look the same in Office; ordinary SVG keeps
  editable text.
  [How to use](https://reshiki.com/guide/sharing/) ·
  [Review](changes/office-svg-placement.md) ·
  [#290](https://github.com/Ameyanagi/ReShiki/pull/290) · @Ameyanagi.

- **EMF pictures on Windows.** Insert `.emf` pictures, such as spectra, at their
  recorded size. `.rsk` files keep the original vectors and a preview that opens
  on every platform.
  [How to use](https://reshiki.com/guide/windows/) ·
  [Details](pictures.md) · [Review](changes/emf-picture-import.md) ·
  [#285](https://github.com/Ameyanagi/ReShiki/pull/285),
  [#296](https://github.com/Ameyanagi/ReShiki/pull/296) ·
  [issue #64](https://github.com/Ameyanagi/ReShiki/issues/64) · @Ameyanagi.

- **More reliable Office paste on Windows.** ReShiki retries when another app
  briefly holds the clipboard.
  [Review](changes/windows-ole-clipboard-retry.md) ·
  [#305](https://github.com/Ameyanagi/ReShiki/pull/305) · @Ameyanagi.

- **Readable panels on Windows.** On Windows, floating panels and menus,
  including the NMR report and its label menu, draw a border instead of a drop
  shadow, so they stay readable after repeated redraws.
  [Review](changes/windows-nmr-shadows.md) ·
  [#310](https://github.com/Ameyanagi/ReShiki/pull/310) · @Ameyanagi.

- **The editor window fits the screen on Windows.** It fits inside the screen's
  work area at startup and when the display scale changes.
  [Review](changes/windows-window-fit.md) ·
  [#273](https://github.com/Ameyanagi/ReShiki/pull/273) · @Ameyanagi.

- **Open files in tabs on Windows and Linux.** Opening a file while ReShiki runs
  adds a tab to the running window, and a file that is already open switches to
  its tab. Each Office drawing gets its own tab, so saving updates only that
  Office document.
  [How to use](https://reshiki.com/guide/windows/) ·
  [Review](changes/windows-linux-open-tabs.md) ·
  [Windows checks](changes/nightly-windows-20261011.md) ·
  [Linux X11 checks](changes/linux-tab-routing-20261011.md)
  ([#307](https://github.com/Ameyanagi/ReShiki/pull/307)) ·
  [#274](https://github.com/Ameyanagi/ReShiki/pull/274), merged through
  [#300](https://github.com/Ameyanagi/ReShiki/pull/300) · @Ameyanagi.

## Assistant

- **Assistant setup and a first example.** The Assistant panel walks you through
  installing Codex, signing in and testing the connection, then offers a small
  ethanol image to try. The draft waits for **Apply**, so you can check it
  first.
  [How to use](https://reshiki.com/guide/assistant-setup/) ·
  [Details](assistant-setup.md) ·
  [Review](changes/assistant-setup-review-2026-10-09.md) ·
  [Setup image](images/assistant-setup/assistant-setup-ready.jpg) ·
  [Example image](images/assistant-setup/assistant-local-example.jpg) ·
  [#280](https://github.com/Ameyanagi/ReShiki/pull/280) · @Ameyanagi.

## Agent API

- **Experimental agent API (Nightly builds since 2026-10-08):** `reshiki --mcp`
  lets Claude Code, Claude Desktop, Codex and other MCP clients import, analyze,
  render, compose, edit and export drawings, and save them only into folders you
  grant; `reshiki --cli` converts, renders, composes and analyzes from a
  terminal. Not in ReShiki 0.11.0; tools and results may change.
  [Setup guide](https://reshiki.com/guide/agents/) ·
  [Reference](agent-api.md) ·
  [Privacy](privacy-policy.md#optional-agent-api-and-command-line-experimental) ·
  [Acceptance record](agent-api-p1-validation.md) · @Ameyanagi.

## Known limits and pending checks

- **Join:** selection and placement accuracy, the Co(en)₃ example and arrowhead
  and tip appearance still need work
  ([issue #246](https://github.com/Ameyanagi/ReShiki/issues/246)).
- **Office, high-DPI and Linux desktop:** the Windows checks so far ran at 100%
  display scale. Fresh Office checks, high and mixed display scales on Windows,
  rear opacity and attached arrows on Windows, and Linux desktop, file-manager
  and Office save-back checks are still pending.
- **Copy to ChemDraw:** copying from ReShiki and pasting into a new ChemDraw
  document gives an empty page
  ([issue #88](https://github.com/Ameyanagi/ReShiki/issues/88)).
- **Assistant:** a draft that looks right can still be the wrong molecule. Check
  it before **Apply**.
- **Pen paths:** freehand tracing is not supported.
- **Windows signing:** public Windows downloads are unsigned while the SignPath
  Foundation release certificate is pending.

## Build and verification

- **Nightly naming checks.** Application and renderer tests use the naming
  helper built for each Windows, macOS and Linux release target. After the
  missing-helper failures in
  [run 38116417137](https://github.com/Ameyanagi/ReShiki/actions/runs/38116417137),
  [Nightly run 38123945065](https://github.com/Ameyanagi/ReShiki/actions/runs/38123945065)
  built all six platforms and published
  [0.11.0-nightly.20261011.38123945065.1](https://github.com/Ameyanagi/ReShiki/releases/tag/nightly-0.11.0-nightly.20261011.38123945065.1)
  from `8e38151190eb2c3814df23a3eb166fb21c972d64`.
  [#304](https://github.com/Ameyanagi/ReShiki/pull/304) · @Ameyanagi.
- **Windows Nightly check.** Arc editing with Undo/Redo, tab activation, ethanol
  insertion with its caption and a floating NMR report passed on the published
  Windows Nightly at 100% display scale.
  [Record](changes/nightly-windows-20261011.md) ·
  [#306](https://github.com/Ameyanagi/ReShiki/pull/306) · @Ameyanagi.
- **Windows signing.** SignPath-signed x64 and ARM64 applications and installers
  are verified, and internal test-signed Nightly artifacts are kept.
  [Setup guide](signpath-setup.md) ·
  [#270](https://github.com/Ameyanagi/ReShiki/pull/270) · @Ameyanagi.
- **Windows review builds.** A source-pinned workflow builds Windows apps for
  review. [#301](https://github.com/Ameyanagi/ReShiki/pull/301) · @Ameyanagi.
- **Assistant image benchmark.** A source-tree tool records graph identity,
  unfinished drafts and failures for eight original synthetic images. The
  cohort is small and does not measure accuracy on real scans.
  [Method and results](assistant-benchmark.md) ·
  [Provisional preview](assistant-benchmark-results/baseline-20261009/cyclic-gly4/run-2/provisional.png) ·
  [#281](https://github.com/Ameyanagi/ReShiki/pull/281), merged through
  [#300](https://github.com/Ameyanagi/ReShiki/pull/300) · @Ameyanagi.
- **Combined reviews.** The features were also checked together:
  [drawing tools and NMR](changes/drawing-tools-integration.md)
  ([#297](https://github.com/Ameyanagi/ReShiki/pull/297)); chemical names,
  captions and atom maps
  ([#298](https://github.com/Ameyanagi/ReShiki/pull/298)); and coordination,
  Join, generic structures, Office and Windows
  ([review](changes/coordination-join-integration.md),
  [#300](https://github.com/Ameyanagi/ReShiki/pull/300), with the adaptive
  limits and Assistant source from
  [#299](https://github.com/Ameyanagi/ReShiki/pull/299)).
