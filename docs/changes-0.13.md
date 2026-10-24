# ReShiki 0.13.0

ReShiki 0.13.0 shows molecules as 3D models (ball & stick, stick, wireframe
and space-filling) beside or in place of their drawings, and opens crystal
structures from CIF, PDB, POSCAR and XYZ files with their unit cell. The full
editor now also runs in a web browser, as a preview on
[reshiki.com/demo](https://reshiki.com/demo/) and as the `@ameyanagi/reshiki`
npm package. Typing groups, the command palette and many drawing details work
better. Installed applications still work on your computer without Python,
RDKit or uv.

## File compatibility

Drawings saved in 0.13.0 use document version **29**, even when they use none
of the new features. ReShiki 0.12.x and earlier cannot open them; ReShiki 0.12
says that the drawing was made with a newer version and asks you to update.
Older drawings still open in 0.13.0. If you share files with people on 0.12,
keep a copy saved by 0.12.

## Highlights

- Show a molecule as a **3D** model, or **Both** 2D and 3D, from the right-click
  **Show as** menu. Choose ball & stick, stick, wireframe or space-filling,
  then rotate and resize the model on the canvas. Exports draw it as vectors.
- Open CIF, PDB, POSCAR/CONTCAR and XYZ crystal files. They open as 2D drawings
  with their unit cell, and can repeat cells and look along an axis.
- Try the editor in your browser at [reshiki.com/demo](https://reshiki.com/demo/),
  or embed it in your own page with `bun add @ameyanagi/reshiki`.
- Type 359 group abbreviations, define your own, and run any command from the
  command palette (**:**).

Previous release: [ReShiki 0.12.0](changes-0.12.md). The changes below keep
their contributor credits and links to their issues and reviews.

## 3D models

- **3D models of structures.** Select a molecule and choose **2D**, **3D** or
  **Both** from the right-click **Show as** menu, the command palette or
  **Properties → Structure** to show it as a vector model in place of its
  drawing or beside it. The conformer comes from the 3D optimize worker (ETKDG,
  then MMFF94s) and is kept for switching back; each switch is one Undo step,
  and chemistry, atom IDs and the 2D layout never change. When a model appears,
  its atoms move from the drawing into 3D and the hydrogens grow out. Clicking
  the model selects its molecule; a chemical edit removes the model. Flipping,
  resizing (except the model's own corner handles), cleaning up, optimizing or
  joining a molecule shown only in 3D asks you to switch to 2D, and rings never
  fuse onto its hidden bonds. `.rsk` files keep models, and PNG, SVG and PDF
  draw them.
  [3D models](3d-keyboard-drawing.md#show-a-molecule-as-a-3d-model) ·
  [Issue #386](https://github.com/Ameyanagi/ReShiki/issues/386) · @Ameyanagi.
- **Rotate, resize and flatten a model.** Once a shown model is selected, drag
  inside its frame to rotate it, **Shift**-drag to roll it and double-click to
  reset the view; corner handles resize the frame. An unselected model, or its
  frame's edge, still drags to move. Going back to 2D can keep the **Original
  drawing** or flatten the model **From the 3D view** as it is turned now, with
  far bonds lighter, like 3D optimize. The **3D tilt** tool, its X/Y step
  commands and the **Tilt X / Tilt Y** fields turn the model of a molecule
  shown as **3D** or **Both** and leave its 2D drawing as drawn. The rotation
  handle, **Rotate 180°**, the rotate step keys and the **Rotate** field roll a
  3D-only model about the view axis; a flip leaves it alone, since a mirrored
  model would be the other enantiomer. Each rotation, tilt, resize and flatten
  is one Undo step, and every drag commits exactly what it previews.
  [3D models](3d-keyboard-drawing.md#show-a-molecule-as-a-3d-model) ·
  [Issue #386](https://github.com/Ameyanagi/ReShiki/issues/386),
  [#401](https://github.com/Ameyanagi/ReShiki/issues/401) · @Ameyanagi.
- **Styles and the 3D model panel.** **Properties → 3D model** offers **Ball &
  stick**, **Stick** (tubes with mitered joints and one outline round their
  outer edge), **Wireframe** and **Space-filling** (van der Waals radii), each
  with a pictogram; a **Flat · journal** or **Glossy · slides** look; **All**,
  **Polar** or **None** hydrogens; **Depth fade** and **Perspective** sliders;
  a **None** or **Soft card** background; and **Reset view**. Model colours
  follow the drawing's colour theme and canvas (Publication uses
  Presentation's colours, with a light grey hydrogen); on a dark canvas the
  flat look's outline is a soft light rim and the soft card is dark. Each
  change is one Undo step, a slider drag included. Balls shade smoothly without
  rings at any zoom, and each bond's shading runs on across its colour change.
  [3D models](3d-keyboard-drawing.md#show-a-molecule-as-a-3d-model) ·
  [Issue #386](https://github.com/Ameyanagi/ReShiki/issues/386) · @Ameyanagi.
- **Space-filling balls meet in true seams.** Overlapping atoms show only the
  part of each ball in front, so two atoms meet along the curve where their
  spheres cross. The **Flat · journal** look outlines only the visible edges
  and seams. There is no contact shadow, so a model draws as quickly at rest as
  while it turns, and exported figures stay small. An atom hidden inside
  another is not drawn. The canvas and PNG, SVG, PDF and Office SVG exports
  draw the same vector shapes.
  [3D models](3d-keyboard-drawing.md#show-a-molecule-as-a-3d-model) ·
  [Issue #455](https://github.com/Ameyanagi/ReShiki/issues/455) · @Ameyanagi.
- **Show as moves out of the way.** Selecting a molecule no longer pops up a
  floating **Show as** bar. Use the right-click **Show as** menu, the command
  palette (**Show as 3D model**, **Show as 2D and 3D**, **Show as 2D drawing**,
  **Show as 2D from the 3D view**) or **Properties → Structure**. In
  **Properties**, the **Structure** and **3D model** sections follow
  **Molecular properties** and **Labels & chemistry**, before **Drawing
  style**.
  [3D models](3d-keyboard-drawing.md#show-a-molecule-as-a-3d-model) ·
  [Issue #422](https://github.com/Ameyanagi/ReShiki/issues/422) · @Ameyanagi.
- **Show as 3D with a name caption.** A molecule imported with **Import → Name
  → Add name below** now shows as **3D** or **Both** and keeps its caption
  attached. Arrow condition text, another molecule's depth fade or another
  molecule's link node no longer stop a model either. A molecule with a link
  node or repeating unit of its own still can't be shown in 3D, and says so.
  [Issue #409](https://github.com/Ameyanagi/ReShiki/issues/409) · @Ameyanagi.

## Crystal structures

- **Open crystal structures.** **Open**, **Import** and drag-and-drop read CIF,
  PDB, VASP POSCAR/CONTCAR (also `*.vasp`), plain XYZ and extended XYZ
  (`.extxyz`) files. CIF symmetry operators are expanded, keeping atoms on the
  cell boundary once and the first alternative of disordered sites; bonds come
  from covalent radii across the cell boundary. Molecular crystals keep whole
  molecules, and extended networks such as NaCl keep each atom once. Saved
  `.rsk` files keep the lattice, atoms, labels, charges, bonds and CIF
  displacement parameters, not the rest of the file.
  [Crystal structures](crystal-structures.md) ·
  [Issue #417](https://github.com/Ameyanagi/ReShiki/issues/417) · @Ameyanagi.
- **Crystals open as 2D drawings.** A file with a unit cell opens as a 2D
  drawing with depth: a parallel projection with the cell box, dashed hidden
  edges, an origin dot and _a_, _b_, _c_ labels placed clear of the box, and
  short, light dashed stubs where bonds leave the cell. A linear carbon shows
  its C (O=C=O), molecules get their double bonds (urea has C=O), atom labels
  sit on a paper halo over the box, and a bond behind another shows a gap. Ion
  charges stay hidden until **Properties → Structure → Ion charges → Show**.
  The opening view keeps labels, bonds and box edges apart. **Show as 3D**
  shows the structure as a 3D model in its cell, with the usual styles, and
  the box turns with the model and appears in PNG, SVG and PDF export.
  [Crystal structures](crystal-structures.md) ·
  [Issue #439](https://github.com/Ameyanagi/ReShiki/issues/439) · @Ameyanagi.
- **Supercells and Look along.** **Properties → Structure** has **Repeat** (1–4
  cells along each of _a_, _b_ and _c_) and **Look along** _a_, _b_, _c_ or
  [111] for a crystal drawn in 2D, each one Undo step and kept in the file. The
  other cells are lighter copies that follow the editable cell. As
  crystallographers draw a cell, atoms on the origin's faces, edges and corners
  are also shown on the far ones, so rock salt shows all 27 ions; the drawing
  still holds each atom once.
  [Crystal structures](crystal-structures.md) ·
  [Issue #417](https://github.com/Ameyanagi/ReShiki/issues/417) · @Ameyanagi.
- **VESTA colour theme.** **Theme → VESTA** colours elements with VESTA's
  element colours (K. Momma and F. Izumi, https://jp-minerals.org/vesta/), via
  pymatgen's ElementColorSchemes.yaml (MIT), for H through Mt. Labels darken or
  brighten only as needed for 5:1 contrast on light and dark paper.
  **Manage themes… → Reference theme** offers VESTA next to Jmol, which stays
  the default, and theme files accept `"base": "vesta"`.
  [Colour palettes](https://reshiki.com/guide/color-palettes/) ·
  [Issue #420](https://github.com/Ameyanagi/ReShiki/issues/420) · @Ameyanagi.

## The editor in a web browser

- **Browser preview.** The full editor runs in the browser (WebGPU, with WebGL2
  as a fallback) at [reshiki.com/demo](https://reshiki.com/demo/), linked from
  the home page and the install guide. It is a preview, not the full app:
  naming, NMR prediction, the assistant, printing and Office integration need
  the desktop app, and 3D models pause the page while they are computed. Your
  files stay on your computer. The first load downloads about 8 MB, and
  Japanese, Korean or Chinese text may download a font from jsDelivr.
  [Install guide](https://reshiki.com/guide/install/#try-it-in-your-browser) ·
  [Issue #405](https://github.com/Ameyanagi/ReShiki/issues/405) · @Ameyanagi.
- **`@ameyanagi/reshiki` on npm.** `bun add @ameyanagi/reshiki` installs the
  editor for your own page: `mount(element)` with `load`, `save`,
  `exportImage`, `onChange` and `destroy`, a `<reshiki-editor>` element, and
  `embed()` for an editor in an iframe. `mount()` also offers `isDirty()`,
  `onDirtyChange`, `onTitleChange`, `fonts` and `cjkFont`. A headless library
  converts between .rsk, MOL, SMILES, CDXML, CDX and InChI and renders SVG, PNG
  and PDF, with SMILES, MOL, InChI and CDXML output identical to the desktop
  CLI on the tested structures. Each version tag publishes the package with the
  app's version and npm provenance.
  [Package README](https://github.com/Ameyanagi/ReShiki/tree/main/packages/editor#readme) ·
  [Issues #396](https://github.com/Ameyanagi/ReShiki/issues/396),
  [#400](https://github.com/Ameyanagi/ReShiki/issues/400),
  [#411](https://github.com/Ameyanagi/ReShiki/issues/411) · @Ameyanagi.
- **Files.** **Open**, **Import → Choose file…**, **Drawing style → Load**,
  **Theme → Import**, **Templates → Import…** and the picture inspector's
  **Replace** use the browser's own file chooser. Dropping structure, crystal
  or picture files inserts them, and dropping drawings opens them in tabs.
  [Issue #428](https://github.com/Ameyanagi/ReShiki/issues/428) · @Ameyanagi.
- **Copy and paste.** **Cmd/Ctrl+C** puts a PNG picture of the selection on the
  clipboard, the same picture as HTML for Word, Google Docs and mail, and
  SMILES as text; pasting it back into a browser editor keeps the drawing
  editable. **Cmd/Ctrl+V** pastes ReShiki drawings, chemical text and PNG
  pictures, and text fields reach the system clipboard. Browsers cannot hold
  PDF or CDX on the clipboard, so **Copy as → PDF** and **CDX** point to Export.
  [Clipboard](clipboard.md#in-the-browser) ·
  [Issues #425](https://github.com/Ameyanagi/ReShiki/issues/425),
  [#429](https://github.com/Ameyanagi/ReShiki/issues/429) · @Ameyanagi.
- **Shortcuts.** On a Mac, ⌘ is the Command key as in the desktop app, also in
  text fields, and menus show ⌘, ⇧, ⌥ and ⌃. Browsers keep Ctrl/⌘+N, W and Tab,
  so **New** is Alt+N, **Close tab** Alt+W and **Next / previous tab**
  Alt+] / Alt+[ (⌃N, ⌃W and ⌃] / ⌃[ on a Mac).
  [Keys](contextual-shortcuts.md#files-clipboard-and-help) ·
  [Issue #424](https://github.com/Ameyanagi/ReShiki/issues/424) · @Ameyanagi.
- **Work is kept.** Reloading or closing the tab with unsaved changes asks
  first, and closing an unsaved editor tab asks **Save**, **Don’t Save** or
  **Cancel**. Recovery drafts of unsaved drawings are kept in browser storage
  and offered after a reload or crash. Settings, templates, themes, your own
  abbreviations and added fonts are kept in the browser's storage too.
  [Issues #426](https://github.com/Ameyanagi/ReShiki/issues/426),
  [#427](https://github.com/Ameyanagi/ReShiki/issues/427) · @Ameyanagi.
- **Text and colours.** Japanese, Chinese and Korean input methods type into
  text fields and captions. Japanese, Korean and Chinese labels and file names
  load a fallback font instead of showing empty boxes, interface symbols draw,
  and panels and 3D models are no longer drawn darker than in the desktop app.
  [Issues #400](https://github.com/Ameyanagi/ReShiki/issues/400),
  [#410](https://github.com/Ameyanagi/ReShiki/issues/410),
  [#429](https://github.com/Ameyanagi/ReShiki/issues/429) · @Ameyanagi.
- **Updates load cleanly.** The demo's JavaScript and WebAssembly are named by
  content hash, so a browser never mixes two releases after an update.
  [Release builds](releasing.md) ·
  [Issue #433](https://github.com/Ameyanagi/ReShiki/issues/433) · @Ameyanagi.
- **Privacy policy.** The [privacy policy](privacy-policy.md#website-and-external-links)
  now covers the Cloudflare Web Analytics that reshiki.com adds to every page,
  the browser preview included, and the font the preview downloads for Japanese,
  Korean or Chinese text. No drawing data reaches either service.
  [Issue #433](https://github.com/Ameyanagi/ReShiki/issues/433) · @Ameyanagi.

## Typing groups and labels

- **359 abbreviations.** The catalog adds protecting groups (Alloc, Teoc, SEM,
  THP, DMTr, Mtt, Pbf, Dde, ivDde, Acm and the carbohydrate set such as OBn,
  OTBS, OPMB, NHAc, NHBoc, OTr, OTf), carbonyl and acid derivatives, sulfonyl,
  phosphorus, boron, tin and zinc groups, amines and aryl and heteroaryl rings.
  Every group's formula was checked against its SMILES, and every label is
  tested by typing it on a bond end. **Contract common groups** can now prefer
  a larger group such as NHCbz, OTMS or SO2Ph. Aliases such as TBDMS, MPM and
  Trt create their groups.
  [Group list](abbreviations.md#chemical-abbreviations) ·
  [PR #348](https://github.com/Ameyanagi/ReShiki/pull/348),
  [#362](https://github.com/Ameyanagi/ReShiki/pull/362),
  [#364](https://github.com/Ameyanagi/ReShiki/pull/364) · @Ameyanagi.
- **Your own abbreviations.** Select a group with one bond to the rest of the
  drawing, enter a label and choose **Define abbreviation from selection**.
  Typing that label on any endpoint then creates the real group, and
  **Contract common groups** finds it.
  [Your own groups](abbreviations.md#your-own-groups) ·
  [PR #363](https://github.com/Ameyanagi/ReShiki/pull/363) · @Ameyanagi.
- **Reverse spellings.** **MeO** typed on a bond end gives OMe, **TBSO** gives
  OTBS, **O2N** gives NO2 and **(HO)2B** gives B(OH)2, each shown in whichever
  spelling suits the free side. A full name such as **morpholino** stays text,
  and the label editor suggests the label to type ("Did you mean Morph?"). A
  group typed over a label that reads to the left keeps that side, so **OBn**
  typed on glucose's C4 **HO** reads **BnO**.
  [Typing groups](abbreviations.md#chemical-abbreviations) ·
  [Issues #376](https://github.com/Ameyanagi/ReShiki/issues/376),
  [#377](https://github.com/Ameyanagi/ReShiki/issues/377) · @Ameyanagi.
- **Bonds meet the attachment atom.** In labels such as MeO, AcS or SPh, the
  bond meets the O or S instead of the middle of the label. A **Bond attachment
  (text labels)** setting chooses flush left, centered, flush right or a
  specific character, and **Flush left** and **Centered** labels come back
  unchanged after a round trip through ChemDraw.
  [Atom labels](atom-labels.md) ·
  [PR #360](https://github.com/Ameyanagi/ReShiki/pull/360) ·
  [Issue #378](https://github.com/Ameyanagi/ReShiki/issues/378) · @Ameyanagi.
- **Labels sit where they fit.** An alternative list such as `[Cl,Br,I]` on a
  horizontal bond meets the bond with its near edge, a longer custom atom
  number grows away from the drawing, and atom-map numbers stack above or below
  element labels.
  [PR #345](https://github.com/Ameyanagi/ReShiki/pull/345),
  [#354](https://github.com/Ameyanagi/ReShiki/pull/354),
  [#365](https://github.com/Ameyanagi/ReShiki/pull/365) · @Ameyanagi.
- **Smaller circled charges.** A circled charge (⊕/⊖) on an atom draws with a
  circle 0.80 × its mark size across, so it no longer stands taller than an O.
  The stored size is unchanged, so ChemDraw round trips keep the same size.
  [Issue #388](https://github.com/Ameyanagi/ReShiki/issues/388) · @Ameyanagi.
- **Attached marks orbit their atom.** Dragging an attached lone pair, charge
  or radical moves it around the atom in 15° steps just outside the label,
  skipping steps that would touch a bond. Hold **Option/Alt** or **Shift** to
  place it freely.
  [Chemical symbols](scientific-symbols.md#add-a-mark-to-an-atom) ·
  [Issue #387](https://github.com/Ameyanagi/ReShiki/issues/387) · @Ameyanagi.

## Commands and drawing

- **Command palette.** Press **:** to search every command, such as benzene,
  OMe, double bond, Predict NMR or an export format, and run it with Enter on
  the selection or the pointed atom or bond, as one Undo step. Results are one
  line each, with category chips and the selected command's description.
  Search words match prefixes, initials (**bt**), letters in order and one
  typo; value commands such as **rot 30**, **tilt x -15** and **scale 150**
  take a number. Vim-style **:w**, **:q** and **:wq** run file commands.
  [Run a command](contextual-shortcuts.md#run-a-command) ·
  [PR #361](https://github.com/Ameyanagi/ReShiki/pull/361) ·
  [Issues #372](https://github.com/Ameyanagi/ReShiki/issues/372),
  [#403](https://github.com/Ameyanagi/ReShiki/issues/403) · @Ameyanagi.
- **Shortcut finder.** Help (?, F1 or Cmd/Ctrl+Shift+/) is a searchable list of
  every shortcut, with live before/after drawings for atom and bond keys.
  [Shortcuts guide](contextual-shortcuts.md) ·
  [PR #357](https://github.com/Ameyanagi/ReShiki/pull/357) · @Ameyanagi.
- **Arrow length follows its condition text.** Text typed or dropped above or
  below a straight reaction arrow attaches to it, stays centred and keeps the
  arrow at least as long as the text.
  [Arrow conditions](arrows.md#write-conditions-on-an-arrow) ·
  [PR #359](https://github.com/Ameyanagi/ReShiki/pull/359) · @Ameyanagi.
- **Rotation.** Drag with a ring tool to turn the ring in 15° steps toward the
  pointer (Option/Alt rotates freely). The rotation handle shows the angle and
  the step keys, a **°** field rotates the selection by a typed angle, and a 3D
  tilt drag keeps turning past 75°, all the way round.
  [Selection handles](selection-transforms.md) ·
  [PR #355](https://github.com/Ameyanagi/ReShiki/pull/355),
  [#358](https://github.com/Ameyanagi/ReShiki/pull/358) ·
  [Issue #389](https://github.com/Ameyanagi/ReShiki/issues/389) · @Ameyanagi.
- **H in the ATOMS palette.** The ATOMS buttons at the bottom of the tool
  palette start with **H**, which picks H for the Atom tool like the other
  element buttons. They read H C / N O / S P / F Cl / Br Fe / \*, so H and C
  show without scrolling the palette at the default window size.
  [Issue #456](https://github.com/Ameyanagi/ReShiki/issues/456) · @Ameyanagi.
- **Word-aware caption wrapping and `reshiki --version`.** Wrapped text breaks
  only between words, or next to CJK characters. `reshiki --version` prints the
  version and exits without opening a window.
  [PR #343](https://github.com/Ameyanagi/ReShiki/pull/343) · @Ameyanagi.

## Fixes

- Curved arrows keep their heads off bond lines, also after you aim their end,
  and clicking any visible stroke of a double or triple bond picks it as the
  arrow's target
  ([#344](https://github.com/Ameyanagi/ReShiki/pull/344),
  [#352](https://github.com/Ameyanagi/ReShiki/pull/352),
  [issue #379](https://github.com/Ameyanagi/ReShiki/issues/379)).
- On 3D cages such as C60, bonds behind the cage are hidden cleanly wherever
  front bonds, rails or ring circles cover them
  ([#356](https://github.com/Ameyanagi/ReShiki/pull/356)).
- Orbital lobes draw whole, with atom labels on top in a halo
  ([#353](https://github.com/Ameyanagi/ReShiki/pull/353)).
- Automatic hydrogens stay clear of nearby labels and charges
  ([#351](https://github.com/Ameyanagi/ReShiki/pull/351)).
- Pasting part of a ChemDraw drawing recalculates hydrogens at the cut bonds
  ([#347](https://github.com/Ameyanagi/ReShiki/pull/347)).
- Aligning mapped reactions moves the product plus with its participants
  ([#346](https://github.com/Ameyanagi/ReShiki/pull/346)).
- Generic structures: small list products expand within the work allowance,
  list labels stay off the ring, the source tab's status settles after an
  expansion, and `[CH2]1-3` is described as a repeat
  ([#340](https://github.com/Ameyanagi/ReShiki/pull/340),
  [#349](https://github.com/Ameyanagi/ReShiki/pull/349)).
- Ferrocene drawn with the Cp shortcut is neutral
  ([#341](https://github.com/Ameyanagi/ReShiki/pull/341)).
- CO2Me stays a real group when its count is subscripted with **Cmd/Ctrl+-**
  ([#338](https://github.com/Ameyanagi/ReShiki/pull/338)).
- The NMR palette's scrollbar stays off the results, and every row fits
  ([#339](https://github.com/Ameyanagi/ReShiki/pull/339)).
- Escape closes status-bar drop-downs such as the theme menu
  ([#342](https://github.com/Ameyanagi/ReShiki/pull/342)).
- **Import → Name** with a salt such as `sodium chloride` says that salts and
  mixtures aren't supported yet and suggests drawing the ions or pasting SMILES
  ([issue #380](https://github.com/Ameyanagi/ReShiki/issues/380)).
- Intel Macs no longer crash when a background task recovers from a panic
  ([#328](https://github.com/Ameyanagi/ReShiki/pull/328)).
- Windows uses Direct3D 12 consistently when it picks a graphics adapter,
  fixing a startup failure seen in a Windows sandbox. Installer antivirus
  detections remain under vendor review
  ([#327](https://github.com/Ameyanagi/ReShiki/pull/327),
  [investigation and limits](release-0.12.0-windows-security.md)).

All fixes above are by @Ameyanagi.

## Known limits and pending checks

- **Browser preview:** naming, NMR prediction, the assistant, printing and
  Office integration need the desktop app. Long 3D generation, cleanups and
  imports pause the page. Fonts installed on the computer are not listed
  without a permission prompt, so other fonts show a substitute.
- **3D models:** a molecule with a link node or repeating unit can't be shown
  in 3D. A flip leaves a model alone, since a mirrored model would be the
  other enantiomer.
- **Crystals:** CIF files keep only the first alternative of disordered sites.
- **Windows signing:** public Windows downloads are unsigned while the SignPath
  Foundation release certificate is pending.

## Build and verification

- **COSMolKit 0.5.0.** InChI, 3D generation and MMFF94, MMFF94s and UFF use
  COSMolKit 0.5.0 instead of a patched 0.3.0 branch. Generated 3D starting
  geometries for a given seed can differ from earlier versions.
  [Issue #369](https://github.com/Ameyanagi/ReShiki/issues/369) · @Ameyanagi.
- **Nix flake.** `nix run github:Ameyanagi/ReShiki` builds and runs ReShiki on
  Linux (x64 and ARM) and Apple Silicon Macs, with an InChI and PNG export
  check after building.
  [Install guide](https://reshiki.com/guide/install/#nix) ·
  [PR #366](https://github.com/Ameyanagi/ReShiki/pull/366) · @Stefanomarton.
- **Browser build checked on every pull request.** A **Browser build
  (wasm32)** job runs Clippy on the browser editor and headless library and the
  `packages/editor` tests.
  [Development](development.md#ci) ·
  [Issue #432](https://github.com/Ameyanagi/ReShiki/issues/432) · @Ameyanagi.
- **0.12.0 release validation.** The record of the tagged source, tag runs,
  publishing run and public downloads.
  [Validation record](release-0.12.0-validation.md) ·
  [PR #324](https://github.com/Ameyanagi/ReShiki/pull/324) · @Ameyanagi.
- **Release preparation.** Update the version, the release notes and the
  documentation for 0.13.0.
  [Validation record](release-0.13.0-validation.md) · @Ameyanagi.
