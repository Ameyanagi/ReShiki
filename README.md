# Moruno

A molecular drawing workspace built with Rust and Iced, with a local Python/RDKit chemistry worker.

Version 0.2 expands everyday editing, recovery and export. It is not yet a complete professional chemistry suite. The [capability report](docs/capabilities.md) separates tested workflows, partial support, and planned features. The [feature-status inventory](docs/feature-status.md) covers the missing toolbar, typography, drawing, chemistry and document features, and their implementation status.

The default drawing style is **JACS / ACS**: black bonds and labels, 10 pt Arial, 14.4 pt bonds, and 0.6 pt lines. Every new document restores those defaults, including after custom typography or bond lengths in another drawing. Canvas and exports share the preset; SVG/PDF retain physical publication dimensions and PNG uses 1200 dpi. See [style settings and sources](docs/jacs-style.md).

[Documentation](https://ameyanagi.github.io/moruno/) · [Downloads](https://github.com/Ameyanagi/moruno/releases) · [Development and pre-commit hooks](docs/development.md)

Desktop downloads require [uv](https://docs.astral.sh/uv/getting-started/installation/). See [installation](docs/getting-started.md) for platform commands and first-use setup.

## Run

Requires a current Rust toolchain and [uv](https://docs.astral.sh/uv/). Tested on Apple Silicon macOS with Rust 1.95, Iced 0.14, Python 3.12, and RDKit 2026.3.6. Release CI builds native macOS, Windows and Linux packages and verifies local uv setup and offline chemistry reuse. Full graphical workflows are currently verified on macOS; see [installation and platform limits](docs/getting-started.md).

```sh
cd ~/dev/moruno
uv sync --locked
cargo run --locked
```

Build a macOS app bundle:

```sh
python3 scripts/build_macos_app.py
open target/debug/Moruno.app

# Portable app; requires uv and sets up local chemistry on first use
python3 scripts/build_macos_app.py --portable
open dist/Moruno.app
```

The default development bundle uses this checkout's `engine/worker.py` and `.venv`. The `--portable` bundle includes the worker source and dependency lockfile. Users install **uv** first; Moruno then creates a local Python/RDKit environment asynchronously on first use. Initial setup requires internet access. Later use works offline and the signed app bundle stays unchanged. Add `--release` for an optimized Rust build. This local bundle is signed ad hoc. Tagged releases use Developer ID signing and Apple notarization; see [release setup](docs/releasing.md). `MORUNO_ROOT` overrides the development worker directory; `MORUNO_PYTHON` explicitly selects an external Python even in a portable bundle. All molecule processing runs locally, without a chemistry service account.

## Use

- Open **Assistant** to ask Codex for editable molecules or reaction schemes. Review, Apply or Reject the proposal; one Undo restores the previous drawing. [Assistant workflow and connection](docs/assistant.md).
- Click **C**, a bond, a ring or the reaction-arrow tool for floating visual choices. Arrows include editable elbows. Select a crossing bond and use Properties → Bond in front / Bond behind to control the small gap. [Tool palettes and crossing depth](docs/tool-palettes.md).
- The two-column palette keeps drawing tools visible. Hover an icon for its name and shortcut; options for the selected tool appear above the canvas.
- With a bond tool, click an empty spot to start a carbon chain, then click its endpoint to grow a zigzag. Branching uses the available space around existing bonds. Drag to choose a direction (15° snapping and JACS bond length by default), or release over an atom to connect. Toggle Length/Angles in the contextual controls, or hold Alt for free drawing; JACS / ACS resets the bond defaults. A bond preview appears only during a drag. With any single/double/triple bond tool, click a bond's middle to cycle **single → double → triple → single**. Wedge, hash and wavy tools apply their style instead. Use the atom tool to replace carbons with other elements.
- Draw a whole chain with `X`, or follow a curved path with `Shift+X`. Preview shows the new-carbon and bond counts before release. Straight chains accept an exact atom count; snaking chains accept a maximum. Counts include existing attachment endpoints. Ctrl starts bending a straight-chain gesture; retrace to shorten, Shift flips the starting side, Escape cancels. A whole chain is one Undo step. [Chain tools and desktop checks](docs/chain-tools.md).
- Choose an element and click an atom to replace it, or empty space to place an atom. The palette contains common elements; the symbol field accepts all 118 elements.
- Bond options include hollow wedges, parallel hashes, bold, dashed coordination, hydrogen, partial, crossed double, dative and quadruple bonds. The contextual menu has 17 presets; Properties changes selected bonds, their color and double-line position. Repeated clicks with a matching directional tool reverse the bond. Hydrogen bonds must start at an explicit, covalently bonded H and end at a supported acceptor. [Bond tools, chemistry and exchange limits](docs/bond-tools.md).
- Choose a 3–8 member ring and click or drag onto a bond to fuse, or an atom to share a vertex. Dedicated Chair A/B and Cyclopentadiene tools add molecular previews, free rotation, fusion and atom sharing; Alt/Option on an atom connects the whole ring by a new bond, and Shift moves cyclopentadiene double bonds. [Ring presets and validation](docs/ring-presets.md). The ring matches the target bond length and orients away from existing substituents. Drag from an attachment to choose its side; Escape cancels placement.
- To attach an already drawn, isolated cycloalkane, choose Select (`V`), grab its interior and drag until an edge meets a target single bond. The ring previews its rotation and scale, then reuses the shared atoms on release. Undo restores the separate ring. This currently supports standalone saturated carbon rings with 3–8 atoms.
- Select an existing fragment and choose **Move & attach…** to connect with a new bond, share an atom, or fuse a chosen edge. Pick the exact source atom/bond in the preview, then click or drag from the destination. Escape cancels; Undo restores the separate fragments. [Fragment joining](docs/fragment-joining.md).
- Choose Arrow (`A`) for eight presets, live previews, endpoint/bend handles, full/half heads, custom line/head dimensions, colors, dipole/no-go marks and unequal equilibrium arrows. Reverse, flip or straighten in Properties; Return applies numeric fields. [Arrow workflow and exchange limits](docs/arrows.md).
- Choose Text and click the canvas to type a multiline label. Double-click an existing label to revise it. Done or Cmd/Ctrl+Enter applies the draft as one Undo step; Escape cancels it. The Properties inspector also supports label editing. The Style toolbar provides searchable fonts, point size, bold/italic/underline, chemical formula formatting, subscript/superscript, alignment and color. Select part of the text to format a range. Line spacing and wrap width are in the inspector; size, hex color and wrap-width fields apply with Enter. [Typography workflow and verification](docs/typography.md).
- Use Chemical symbols for charges, radicals, lone pairs and free annotation marks. Attached charges/radicals update the atom chemistry; Properties can reposition, rotate or remove its marks. Orbitals offers seven shapes, outline/solid/gray phases, phase reversal and a live style preview. Drag from the node to set direction/size; group orbitals with a molecule to move them together. [Scientific symbols and exchange limits](docs/scientific-symbols.md).
- Draw rectangles, rounded rectangles, ellipses, lines, arcs, brackets, parentheses, braces and Bézier curves. Drag to preview; Shift constrains. Properties controls stroke/fill colors, width, solid/dashed/dotted lines and front/back order. Select a curve to edit its anchors and control points. [Graphics workflow and limits](docs/graphics.md).
- Import PNG, JPEG, TIFF or WebP through **Import → Picture…**. Resize, rotate, replace and layer embedded pictures with your drawing; macOS also supports picture paste. CDXML and editable Copy preserve pictures alongside molecules. See [pictures](docs/pictures.md).
- Copy/cut/paste and duplicate selected drawing objects. On macOS, Copy supplies editable drawing data plus PDF, PNG and SVG images; Cmd+Shift+C copies only an image of the selection (or the whole drawing when nothing is selected). Paste accepts supported native binary drawings, CDXML, MOL, SMILES and InChI. See [clipboard workflow and limits](docs/clipboard.md).
- Templates show structure thumbnails: choose one, preview it on the canvas, then click to place it or attach to a compatible atom/bond. Drag from the target to choose the side; Escape cancels. Choose Connect with a bond, Share an atom, or Fuse along a bond; click the exact source atom/edge in the preview. Attachment matches the existing bond length. Incompatible elements, bond orders or valences show a red target and leave the drawing unchanged.
- Search/filter 81 built-in templates and mark favorites. Collections include common heterocycles, bicyclic/cage rings, 20 amino acids, five nucleobases and two chair projections; amino-acid three-letter codes are searchable. Click an exact source atom or bond in the larger preview, or use Auto; Keep placing repeats insertion. Save selected fragments, captions, graphics and groups as custom templates, remember attachment anchors, edit/replace entries, and import/export `.moruno-templates` collections. [Library workflow and verification](docs/template-library.md).
- Select objects to reveal rotate, flip, align, distribute, and bond-direction controls in the Properties inspector. Double-click an atom to select its molecule.
- Use Lasso (`L`) for freeform selection, Shift to add, and Option/Alt to subtract a region. Group molecules with captions, arrows and graphics using Cmd/Ctrl+G; nested groups move and transform together. Option/Alt-click or drag edits a member; integral groups stay whole. Add frame fits and groups a bracket or box around the selection. Edge/center alignment and distribution respect groups. [Selection and grouping workflow](docs/selection-and-groups.md).
- With Select, a selected structure shows a box: drag a corner to resize proportionally, or the circular handle above it to rotate. Hold Shift to snap rotation to 15° steps. Escape cancels a drag; each completed drag is one Undo step. Cmd/Ctrl+A selects everything and activates Select. Resizing keeps JACS label typography and line widths.
- Hovered atoms show a circle; hovered bonds show circles at both ends. Select/move clicks on a bond select its two endpoint atoms, and dragging moves those atoms together. Click an atom to narrow the selection. Drag empty space for a rectangular selection. Charge, isotope, and deletion controls apply to the selected atoms or objects in the Properties inspector.
- Properties → Chemical abbreviations contracts common groups or a named selection while retaining all atoms, and expands them for editing. Common presets can replace a terminal atom. [Abbreviations](docs/abbreviations.md).
- Properties → Atom labels & numbering controls carbon/H display, numeric/letter sequences, R/S and E/Z labels, and indicator positioning. These display choices preserve molecular composition. Derived labels refresh after chemical edits; Check structure also refreshes formula, mass, descriptors and canonical SMILES without adding an Undo step. Clean up requires a selection and previews only selected atoms or selected molecules. Unselected atoms stay fixed; Show original, Apply and Cancel support review, and applying is one Undo step. [Atom labels](docs/atom-labels.md).
- Open Import to enter SMILES, InChI or supported structure text. Enter and Insert add to the drawing; Replace drawing and the example buttons replace it. Undo restores the prior state.
- Save/open `.moruno` retains the complete editable drawing. The Open dialog also accepts MOL, SMILES (`.smi`/`.smiles`), InChI, and supported CDXML drawings.
- Export SVG or PDF for a vector drawing, PNG at 1200 dpi, MOL/SMILES/InChI for molecular data, or CDXML for basic drawing interchange. MOL/SMILES/InChI cannot store annotation text or arrows.
- The Style palette applies color to **All selected**, **Text**, or **Bonds**. Select All includes bonds; click a bond’s middle to select it individually and Shift-click to add more. Each color change is one Undo step.
- Properties, Templates and Export have separate inspector tabs. The top-right inspector button hides the panel to expand the canvas. View controls toggle Grid, Rulers and Crosshair independently and choose mm, cm, inches or points. These canvas aids start off and follow the publication scale. Fit uses the actual available canvas size and leaves export dimensions unchanged.

| Shortcut                               | Action                                                                                                              |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| V / B or 1 / 2 / 3                     | Select / single / double / triple bond                                                                              |
| 4                                      | Quadruple bond                                                                                                      |
| X / Shift+X                            | Straight / snaking chain                                                                                            |
| Alt while drawing a bond or chain      | Temporarily release angle/length constraints                                                                        |
| L                                      | Freeform lasso                                                                                                      |
| Shift+R                                | Toggle aromatic ring mode (enabling selects a six-member ring)                                                      |
| R / A / T / E                          | Ring / arrow / text / eraser                                                                                        |
| C / N / O / S / P / F / H              | Replace the hovered or singly selected atom; show valence-derived H labels. With no target, choose an element tool. |
| S / D / T on a bond                    | Set single / double / triple; repeated D cycles centered / left / right lines                                       |
| A with a selected aromatic ring        | Toggle aromatic circle / alternating bonds                                                                          |
| Shift/Ctrl while dragging a template   | Snap its direction to 15°; Alt temporarily releases the constraint                                                  |
| Mouse Back / Forward; Alt+Left / Right | Navigate template categories and previews                                                                           |
| Cmd/Ctrl+I / E                         | Import / export panel                                                                                               |
| ?                                      | Shortcut reference                                                                                                  |
| Cmd/Ctrl+Z; Cmd/Ctrl+Shift+Z           | Undo; redo                                                                                                          |
| Cmd/Ctrl+B / I / U in the text editor  | Bold / italic / underline                                                                                           |
| Cmd/Ctrl+A                             | Select all drawing objects                                                                                          |
| Cmd/Ctrl+Shift+A                       | Invert selection                                                                                                    |
| Cmd/Ctrl+G / Cmd/Ctrl+Shift+G          | Group / ungroup one level                                                                                           |
| Cmd/Ctrl+C / X / V / D                 | Copy / cut / paste / duplicate                                                                                      |
| Cmd+Shift+C (macOS)                    | Copy image of selection, or whole drawing                                                                           |
| Cmd+Shift+V (macOS)                    | Paste the clipboard picture representation                                                                          |
| Cmd/Ctrl+Shift+S                       | Save as                                                                                                             |
| Delete/Backspace                       | Delete selection                                                                                                    |
| Cmd/Ctrl+S / O / N                     | Save / open / new                                                                                                   |
| Escape                                 | Select tool                                                                                                         |
| Wheel                                  | Zoom around the pointer                                                                                             |
| Right or middle drag                   | Pan                                                                                                                 |

Text fields handle their own editing shortcuts. Use the window close button for the unsaved-changes prompt; interrupted sessions can be restored from the five-second recovery snapshots. Recovery files live in the platform application-data directory (override with `MORUNO_DATA_DIR`). Restoration makes an unsaved copy.

## Verification

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
uv run --locked python -m unittest discover -s tests -p 'test_*.py'
cargo run --locked -- --engine-check
```

Tests cover molecular identity, tetrahedral and E/Z stereo, stable atom IDs, cleanup, error recovery, file exchange, history, zigzag chain growth, bond attachment, rapid pointer events, and stale asynchronous results. Desktop tests also exercised freehand drawing, validation, cleanup, movement, annotations, save/reopen, undo/redo, and cross-application XML exchange. See the [test record](docs/capabilities.md#desktop-test-record).

## Publication pages

Open **View → Page setup…** for paper sizes, margins and a page grid. Navigate or fit individual pages, center selected content without changing its physical scale, and export a multipage vector PDF. Page settings are saved with the drawing and are undoable. On macOS, **⌘P** opens the native print dialog at 100% scale; **Export → Print selection…** prints selected content on one sheet. The canvas stays editable while the print dialog is open. [Page workflow and limits](docs/publication-pages.md).

## Design

The Rust document and drawing scene are independent of RDKit. Chemistry requests pass through a versioned protocol and `ChemistryEngine` interface so a Rust backend can replace Python in stages. See [architecture](docs/architecture.md), [framework research](docs/framework-research.md), and [roadmap](docs/roadmap.md).

See [0.2 changes](docs/changes-0.2.md) and the [ruviz integration requirements](docs/ruviz-integration.md).

Production Rust forbids unsafe code and denies Clippy checks for `unwrap`, `expect`, panic/todo/unreachable macros, and unchecked collection indexing. Keep these checks enabled. Invalid input should return a recoverable error, and failed edits must preserve the drawing. Run `cargo clippy --all-targets -- -D warnings` before packaging. [Runtime safety audit](docs/runtime-safety.md).
