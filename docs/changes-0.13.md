# ReShiki 0.13.0

ReShiki 0.13.0 shows molecules as 3D models (ball & stick, stick, wireframe
and space-filling) beside or in place of their drawings, and opens crystal
structures from CIF, PDB, POSCAR and XYZ files with their unit cell. The full
editor now also runs in a web browser, as a preview on
[reshiki.com/demo](https://reshiki.com/demo/) and as the `@ameyanagi/reshiki`
npm package, with naming and NMR prediction. Typing groups, the command
palette and many drawing details work better, and large drawings pan, zoom,
edit and export much faster. Installed applications still work on your
computer without Python, RDKit or uv.

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
  or embed it in your own page with `bun add @ameyanagi/reshiki`. Naming and
  NMR prediction work there too.
- Type 359 group abbreviations, define your own, and run any command from the
  command palette (**:**).
- Pan, zoom, hover, edit, copy and export large drawings much faster, with
  less memory for Undo and long stereo chains.

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
  naming and NMR prediction work, but printing, the assistant and Office
  integration need the desktop app, and 3D models pause the page while they
  are computed. Your files stay on your computer. The first load downloads
  about 8 MB, and Japanese, Korean or Chinese text may download a font from
  jsDelivr.
  [Install guide](https://reshiki.com/guide/install/#try-it-in-your-browser) ·
  [Issue #405](https://github.com/Ameyanagi/ReShiki/issues/405) · @Ameyanagi.
- **The demo page shows its progress.** [reshiki.com/demo](https://reshiki.com/demo/)
  shows a progress bar while the editor downloads, and a message with a
  **Reload** button and a desktop-app link when the browser has neither WebGPU
  nor WebGL 2 or the download fails. The editor's JavaScript and WebAssembly
  download together instead of one after the other. On a phone the page says
  ReShiki is made for a computer and downloads nothing until you choose
  **Continue anyway**. The "Browser preview" note is a banner above the editor
  instead of floating over the canvas, menus and status bar, and the page has a
  favicon, a description and a link preview.
  [PR #461](https://github.com/Ameyanagi/ReShiki/pull/461) ·
  [Issues #433](https://github.com/Ameyanagi/ReShiki/issues/433),
  [#434](https://github.com/Ameyanagi/ReShiki/issues/434) · @Ameyanagi.
- **Name ↔ structure conversion.** **Import → Name** and **Generate selected
  name** work in the browser editor instead of saying "Naming is not available
  in the browser yet". The OPSIN Rust port and naming rules run in the editor's
  own Web Worker and WebAssembly module (`dist/worker/`, 2.3 MB gzipped),
  loaded on the first conversion, so drawing and typing carry on while a name
  is read. As on the desktop, a request stops after 15 seconds and its heap is
  capped at 96 MiB, and the next request starts a new worker. The parser's
  resources also take less memory, on the desktop too: 7.5 MiB instead of
  33.8 MiB after loading.
  [Package README](https://github.com/Ameyanagi/ReShiki/tree/main/packages/editor#naming) ·
  [PR #471](https://github.com/Ameyanagi/ReShiki/pull/471) ·
  [Issue #430](https://github.com/Ameyanagi/ReShiki/issues/430) · @Ameyanagi.
- **NMR prediction.** **Predict NMR** (the Predict tab, **Properties → Predict
  NMR…** and the command palette) predicts ¹H and ¹³C shifts in the browser
  editor, with the same bundled data and report as the desktop app, instead of
  saying it needs the desktop app. The prediction runs on the page's own
  thread, so the page pauses while it runs, at most about 3 seconds; the busy
  state shows first.
  [PR #465](https://github.com/Ameyanagi/ReShiki/pull/465) ·
  [Issue #430](https://github.com/Ameyanagi/ReShiki/issues/430) · @Ameyanagi.
- **Only what the browser can run.** The **Assistant** button, the command
  palette's **AI assistant** and **Assistant panel**, and **Print…** with
  Cmd/Ctrl+P no longer appear in the browser editor, since each only said it
  needs the desktop app. Clicking the ReShiki logo opens **About ReShiki** with
  the version and a **Get the desktop app ↗** link instead of an update check
  that always failed.
  [PR #469](https://github.com/Ameyanagi/ReShiki/pull/469) ·
  [Issue #434](https://github.com/Ameyanagi/ReShiki/issues/434) · @Ameyanagi.
- **The npm editor recovers and reports errors.** After the editor's
  WebAssembly fails to download once, the next `mount()` tries again instead of
  failing until the page is reloaded. A `<reshiki-editor src>` that answers
  with an HTTP error, such as a 404 page, fires `error` with
  `Could not load <src>: HTTP 404` instead of reading the page as a drawing. On a page whose
  Content-Security-Policy keeps `style-src` strict, the editor fills its
  element instead of shrinking to a 300×150 canvas, and the package README
  lists what the editor needs.
  [Package README](https://github.com/Ameyanagi/ReShiki/tree/main/packages/editor#readme) ·
  [PR #468](https://github.com/Ameyanagi/ReShiki/pull/468) ·
  [Issue #431](https://github.com/Ameyanagi/ReShiki/issues/431) · @Ameyanagi.
- **Docs match the browser editor.** The npm README, the install guide and the
  home page no longer say that 3D needs the desktop app (3D models work but
  pause the page while they compute), that drawing styles, themes, templates
  and pictures cannot be loaded from files, that Insert cannot insert files,
  that copy and paste carry text only or that there are no recovery drafts.
  All three say that naming and NMR prediction work in the browser editor.
  [Install guide](https://reshiki.com/guide/install/#try-it-in-your-browser) ·
  [PR #460](https://github.com/Ameyanagi/ReShiki/pull/460) ·
  [Issue #434](https://github.com/Ameyanagi/ReShiki/issues/434) · @Ameyanagi.
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
- **Automatic label alignment.** The Style bar's alignment menu offers
  **Automatic**, **Left**, **Center**, **Right** and **Stacked above** when
  atom or group labels are selected, with Automatic checked by default, as
  ChemDraw's Text menu does. Automatic reads the label from its bonds (HO on a
  left-pointing bond, OH on a right-pointing one, MeO and OMe), and choosing it
  again returns a label forced to Left, Center or Right to automatic. It covers
  element labels (Left reads OH, Right HO) and free text labels as well as
  groups, and replaces the group-only **Auto / above** list. The **Edit atom
  label** dialog gains an **Alignment** list, Automatic by default. Captions
  keep Left / Center / Right / Justify.
  [Chemical abbreviations](abbreviations.md) ·
  [PR #466](https://github.com/Ameyanagi/ReShiki/pull/466) ·
  [Issue #458](https://github.com/Ameyanagi/ReShiki/issues/458) · @Ameyanagi.
- **The Text tool keeps and repeats labels.** Clicking or double-clicking
  another atom while typing a label commits it, as Enter does, instead of
  throwing it away, and so does a click anywhere else outside the label editor.
  The Text tool stays active after a label is committed. Double-clicking
  another atom, or Option/Alt-clicking it as in ChemDraw, puts the last label
  on it at once without opening the editor, so OMe typed once can go on atom
  after atom. Click an atom to change a repeated label. Each labelled atom is
  its own Undo step, and Escape or switching tools forgets the label.
  [Drawing tools](drawing-tools-validation.md) ·
  [PR #467](https://github.com/Ameyanagi/ReShiki/pull/467) ·
  [Issue #457](https://github.com/Ameyanagi/ReShiki/issues/457) · @Ameyanagi.

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
- **Exports named after the drawing.** PNG, SVG, PDF and structure exports
  suggest the saved drawing's name, such as `aspirin.png` for `aspirin.rsk`,
  instead of always `Molecule.png`; an untitled drawing still suggests
  `Molecule`. This applies to the desktop save dialog and to browser downloads.
  In the browser editor, downloads carry their file type, and **Save** says
  "Downloaded Untitled.rsk" instead of "Document saved".
  [PR #463](https://github.com/Ameyanagi/ReShiki/pull/463) ·
  [Issue #428](https://github.com/Ameyanagi/ReShiki/issues/428) · @Ameyanagi.

## Performance

Measured on the shortcut examples (483 atoms), or on those examples tiled 16
times (7,700 atoms). Drawings, labels, files and exports look the same as
before.

- **Panning and zooming.** Scrolling, panning and zooming only move the view
  instead of also running the whole edit pipeline (copying and comparing the
  drawing, checking it and the Undo history) on every scroll tick, so they stay
  smooth on drawings with thousands of atoms. They add no Undo step and leave
  the drawing unchanged and saved; layout that depends on computed labels is
  repaired as the labels land.
  [PR #474](https://github.com/Ameyanagi/ReShiki/pull/474),
  [#488](https://github.com/Ameyanagi/ReShiki/pull/488) · @Ameyanagi.
- **Building the scene.** After an edit, the drawing's shapes look atoms and
  bonds up in an index made once per build, instead of scanning the whole
  drawing for every atom label, double bond, bond end and ring circle. A build
  takes about 160 ms instead of 600 ms on 7,700 atoms, and about 21 ms instead
  of 40 ms on 1,900 atoms.
  [PR #485](https://github.com/Ameyanagi/ReShiki/pull/485) · @Ameyanagi.
- **Redraws when zoomed in.** The canvas skips bonds, shapes, lines and label
  halos that lie entirely outside the window before turning them into
  triangles. Zoomed in on 7,700 atoms, drawing the scene takes about 1 ms
  instead of 8–12 ms, with the same pixels on screen.
  [PR #479](https://github.com/Ameyanagi/ReShiki/pull/479) · @Ameyanagi.
- **Zooming text.** The canvas outlines text at quarter-octave zoom steps and
  scales the small remainder, and a full text cache drops the outlines used
  least recently instead of all of them. A zoom frame of the 483-atom examples
  takes 1.5 ms instead of 11 ms and allocates 1.6 MB instead of 10 MB. At ½×,
  1×, 2× and the other steps text draws pixel for pixel as before; between
  steps, glyph edges can move by about one pixel.
  [PR #482](https://github.com/Ameyanagi/ReShiki/pull/482) · @Ameyanagi.
- **Hover.** Finding the atom label under the pointer, on every mouse move,
  skips atoms whose labels cannot reach the pointer instead of laying out
  every label. Each check takes about 1 ms or less instead of 0.7–1.0 s on
  7,700 atoms, and 0.1–0.3 ms instead of 17–20 ms on 1,900 atoms.
  [PR #473](https://github.com/Ameyanagi/ReShiki/pull/473) · @Ameyanagi.
- **Edits with many group labels.** Moving or editing atoms in a drawing with
  many collapsed groups (such as Boc, OTBS or CO2Et) no longer rescans every
  bond and atom once per group. A move on 7,700 atoms with 256 group labels
  commits in about 17 ms instead of 50 ms. Which groups stay collapsed is
  unchanged.
  [PR #481](https://github.com/Ameyanagi/ReShiki/pull/481) · @Ameyanagi.
- **Unsaved-changes check.** The unsaved-changes dot in the window title and
  tab strip compares the drawing with the saved one once per update instead of
  three times, without copying every atom and bond: about 0.3 ms instead of
  6.5 ms per update on 7,700 atoms. Undoing back to the saved drawing still
  clears the dot.
  [PR #475](https://github.com/Ameyanagi/ReShiki/pull/475) · @Ameyanagi.
- **Undo memory.** Each tab's Undo history keeps at most about 128 MB of
  drawing copies, dropping the oldest steps first but always keeping at least
  twenty. Drawings of up to about 1,700 atoms keep all 100 steps as before; a
  7,700-atom drawing keeps 22 steps in 122 MB instead of 100 steps in 553 MB.
  Redo is unchanged.
  [PR #472](https://github.com/Ameyanagi/ReShiki/pull/472) · @Ameyanagi.
- **Copy and export.** Copying as PDF or a picture, and exporting PNG, PDF or
  Office SVG, no longer loads and scans every installed font each time, and
  the default label font (Arial) is read from disk once. A PDF of the 483-atom
  examples takes about 40 ms instead of 350–550 ms, and of 7,700 atoms about
  2.4 s instead of 8.5–11 s. The files are the same as before.
  [PR #478](https://github.com/Ameyanagi/ReShiki/pull/478) · @Ameyanagi.
- **Turning 3D models and crystal repeats.** While you turn or tilt a
  space-filling or ball & stick model, its balls draw whole, back to front, at
  screen resolution; seams and sharp Retina rendering return when you let go.
  A dense 512-atom space-filling model turns at about 0.15–0.18 s per frame
  instead of 0.6–1.6 s. Each model's seams and each crystal repeat's lighter
  copies are kept, so an edit beside a 4 × 4 × 4 rock-salt repeat takes about
  1 ms instead of 0.43–0.48 s. The canvas keeps at most 64 MB of rendered 3D
  tiles instead of 128 MB, and none once no model is shown in 3D.
  [PR #484](https://github.com/Ameyanagi/ReShiki/pull/484) · @Ameyanagi.
- **2D layout of long chains and polymers.** Laying out a long chain or
  polymer from SMILES, a name or a structure without coordinates is about
  3–10× faster: polystyrene with 75 units takes 0.4 s instead of 2.8 s, and
  poly-alanine with 200 units 0.4 s instead of 3.8 s. Polystyrene with 100
  units now lays out instead of failing with "Depiction collision work or
  storage limit exceeded", and a still larger molecule whose overlap repair
  runs out of work keeps its layout before that repair instead of failing.
  [PR #477](https://github.com/Ameyanagi/ReShiki/pull/477) · @Ameyanagi.
- **R/S and E/Z labels on long stereo chains.** Stereo labelling keeps one
  centre's ranking data at a time instead of all of them, so a chain with 300
  stereocentres peaks at about 6 MB instead of 76 MB. Chains with 400 or more
  centres, which failed with "CIP resource limit exceeded" and showed no
  labels, are now labelled.
  [PR #476](https://github.com/Ameyanagi/ReShiki/pull/476) · @Ameyanagi.
- **Browser editor memory.** Fonts the browser editor keeps for later visits,
  such as the Japanese and Korean fallbacks, stay in browser storage until text
  needs them instead of being loaded into memory on every visit. With Noto Sans
  JP and KR saved (9.2 MB), the editor starts in 41.4 MiB instead of 59.0 MiB.
  The browser editor also drops the desktop worker allocator, so its
  WebAssembly is 140 KB smaller. The desktop app is unchanged.
  [PR #483](https://github.com/Ameyanagi/ReShiki/pull/483) · @Ameyanagi.

## Fixes

- Curved arrows move clear of a new H or R/S or E/Z label, and a linked name
  caption below a molecule clear of a new stereo label, as soon as the labels
  appear instead of on the next edit, pan or zoom. The move belongs to the
  edit, so one Undo still returns to the drawing before it, and opening a file
  makes the same move without marking it as changed
  ([#488](https://github.com/Ameyanagi/ReShiki/pull/488)).

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

- **Browser preview:** printing, the assistant and Office integration need
  the desktop app. NMR prediction, long 3D generation, cleanups and imports
  pause the page. Fonts installed on the computer are not listed
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
