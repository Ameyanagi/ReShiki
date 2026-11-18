# ReShiki 0.16.0

ReShiki 0.16.0 keeps drawings in your GitHub repositories with a chemistry-aware
Compare, adds a Settings page with copy and export backgrounds and resolutions,
puts name and property captions under molecules, and lets Python, the command
line and agents predict NMR shifts and write them onto atoms. Clean up applies
at once, and large drawings stay responsive.

[Release downloads](https://github.com/Ameyanagi/ReShiki/releases/tag/v0.16.0) ·
[Previous release](changes-0.15.md) ·
[Release validation](changes/release-0.16.0-validation.md).

## File compatibility

ReShiki saves each drawing at the lowest document version that can hold it:
the version of the newest feature in the table that the drawing uses, or
**30** if it uses none of them. Most drawings saved in 0.16.0 therefore still
use version 30 and open in ReShiki 0.13.1, 0.14.0 and 0.15.0. The newest
version is **39**.

| Version | Feature in the drawing                                                                                | Opens in |
| ------- | ----------------------------------------------------------------------------------------------------- | -------- |
| 31      | A reaction label with a custom gap to its arrow                                                       | 0.14.0   |
| 32      | ORTEP thermal ellipsoids or molecule-only display on a crystal model                                  | 0.14.0   |
| 33      | A 3D model generated from a structure in 0.14.0 or later, 3D atom labels, or a plain XYZ model        | 0.14.0   |
| 34      | An amino-acid residue collapsed with two attachment points, such as `-Ala-` between Boc and OMe       | 0.14.0   |
| 35      | An arrow with nodes in the middle, or a drawing style with a **Crossing clearance** other than 1.1 pt | 0.15.0   |
| 36      | A stereocentre in an AND or OR stereo group                                                           | 0.15.0   |
| 37      | An atom with three or four unpaired electrons, such as `[C]` or `[CH]` from SMILES                    | 0.15.0   |
| 38      | A property caption linked to its molecule (**Show properties**)                                       | 0.16.0   |
| 39      | A property caption with a **Name** line, or a molecule name you typed                                 | 0.16.0   |

ReShiki 0.15.0 and earlier say that a version 38 or 39 drawing was made with a
newer version and ask you to update. Older drawings still open in 0.16.0,
unless they are over its new limits: 64 KB of text in one text box, 1 MB of
text in the drawing, or pictures stored in more than 64 MB (see
[Files, import and export](#files-import-and-export)).

NMR shift labels written by Python, the command line or the agent tools are
atom numbers and do not raise the version. ReShiki 0.15.0 opens such a drawing
and shows the shifts at its usual atom-number positions. A PNG from 0.16.0
that carries an editable drawing keeps it after the image data; 0.15.0 still
finds it there and reopens it when its version is 37 or lower.

On macOS and Linux, 0.16.0 moves its data folder to `~/.reshiki/` on the first
launch (see [Desktop](#desktop)). After the move, an older version looks in the
old folder, so it starts with default settings and without your saved
templates, themes and recovery drafts. Where an older version shares the data
folder, as on Windows, it keeps your settings but drops the new copy and export
settings when it next saves them.

If you share files with someone on an older version, keep a copy saved by
that version.

## Highlights

- **Drawings on GitHub.** Sign in from the GitHub button in the status bar,
  open drawings and other chemistry files with **Open from GitHub…**, and
  **Send** your changed drawings as one commit with an SVG preview each. A
  drawing changed on GitHub meanwhile is not overwritten unless you choose
  **Keep mine**.
- **Compare that knows chemistry.** **History** and the **changed on GitHub**
  dialog tell chemical edits from moves, flips and rescaling, and name each
  change with the drawing's own atom numbers, such as **OH added at C4**.
- **Captions under molecules.** **Show properties** puts the name, formula,
  Mol. Wt. and other properties under a molecule and keeps them up to date.
  Type your own name, such as `Compound 3a`. CDXML and CDX keep the captions
  as ChemDraw linked text.
- **NMR from scripts.** Python, `reshiki --cli` and the MCP tools predict ¹H
  or ¹³C shifts and write them beside the atoms, as journals print them.
- **Settings page.** **Cmd+,** (**Ctrl+,**) opens every preference on one
  page. Copies and exports get their own background (transparent, canvas,
  white, black or a color) and PNG resolution; copies default to 300 dpi.
- **Clean up at once, rings upright.** Clean up applies as one Undo step
  without a preview, keeps the drawing's size, and stands six-membered rings
  upright. Large rings such as 18-crown-6 stay round.
- **Files as themselves.** **Open** shows a ChemDraw or MOL file as that
  format, **Save** asks whether to keep it, and SD files open. ChemDraw
  diagrams with several molecules open in their authored arrangement.
- **Faster large drawings.** Selecting, aligning, redrawing and panning large
  drawings take a fraction of the time, and undo history shares one memory
  budget across tabs.

The changes below keep their links to their pull requests and issues.
Siddhant Sharma ([@ssiddhantsharma](https://github.com/ssiddhantsharma))
designed scripted NMR prediction and atom labels in
[issue #1203](https://github.com/Ameyanagi/ReShiki/issues/1203). All other
changes in this release are by @Ameyanagi.

## GitHub

- **Keep drawings on GitHub.** The desktop app signs in to GitHub with a code,
  opens drawings with **Open from GitHub…** and sends them with **Send**.
  **Save** stays on this computer. **Send** makes one commit of the changed
  drawings in the repository, with an SVG preview beside each; a drawing that
  also changed on GitHub waits for your choice. The status bar
  shows what is not on GitHub yet, and closing such a drawing asks **Send to
  GitHub** or **Later**. A drawing changed on GitHub meanwhile is a question,
  not an error: GitHub's and yours show side by side with the differences
  highlighted, then **Keep GitHub’s**, **Keep mine** or **Keep both**.
  **History**, a clock tab in the inspector, lists saves and commits by day and
  opens, restores or compares any of them
  ([#1199](https://github.com/Ameyanagi/ReShiki/pull/1199),
  [#1206](https://github.com/Ameyanagi/ReShiki/pull/1206)).
  [Keep drawings on GitHub](github.md) ·
  [Privacy policy](privacy-policy.md#github-sign-in-and-send).
- **Where things are.** The GitHub button sits at the left end of the status
  bar, before the repository chip, and its menu opens upward. It has a dot
  while anything is not sent. GitHub drawings carry a colored badge with the
  repository's initials on each tab (right-click to change the color), the
  repository in the window title and a status bar chip such as
  **● lab-notes / scheme · 1 unsent** with **Send**. **Send** opens a small box
  above the chip with the message selected; Enter sends. Sign-in shows its code
  at once, closes by itself and goes on with what started it
  ([#1206](https://github.com/Ameyanagi/ReShiki/pull/1206),
  [#1221](https://github.com/Ameyanagi/ReShiki/pull/1221)).
- **Open from GitHub….** The picker opens the repository used last, lists
  drawings by name with who changed them, and makes a new repository, drawing
  or folder in place. It switches between **Tree** (the first time) and
  **Tiles**, then opens in the view you used last. The tree lists and searches
  the repository as GitHub returns it and previews the selected file. Besides
  drawings it lists `.mol`, `.sdf`, `.cdxml`, `.cdx`, `.rxn` and `.smi` files;
  those open read only, and **Save** makes a `.rsk` copy. **Insert**
  (Shift+Enter, or **Insert into drawing** on a file's right-click menu) puts a
  file's structures into the drawing in front as one Undo step. In **Tiles**,
  folders of more than 200 files show the first 200, and the search finds the
  rest. Each repository row ends
  in **↗**, which opens it on GitHub
  ([#1211](https://github.com/Ameyanagi/ReShiki/pull/1211),
  [#1215](https://github.com/Ameyanagi/ReShiki/pull/1215),
  [#1221](https://github.com/Ameyanagi/ReShiki/pull/1221)).
- **Other people's repositories.** Paste a github.com link into **Open from
  GitHub…**, the command palette or the window, or choose **+ Add public
  repository…**. Public repositories open signed out too. A repository you may
  not send to is **Read only**: edit and save here, then **Save a copy…** into
  one of yours. A link to another branch or a commit opens that version as an
  **Old version** tab; **Open latest** goes to the drawing as it is now.
  Right-click a GitHub tab to **Copy GitHub link**
  ([#1209](https://github.com/Ameyanagi/ReShiki/pull/1209)).
- **Newer on GitHub.** When someone changes an open drawing on GitHub, the
  status bar chip shows **Newer on GitHub** with **Update** as you come back to
  the window. **Update** shows their drawing in one Undo step and says who
  changed it; if you changed the drawing too, the chip shows **Review**
  ([#1210](https://github.com/Ameyanagi/ReShiki/pull/1210)).
- **Faster picker and Send.** A folder's files show as soon as it is listed,
  and who changed each file and its preview fill in as they arrive. **Open from
  GitHub…** keeps what GitHub sent across restarts (up to 200 MB in the data
  folder, per account), so a repository you opened before asks GitHub only
  whether its branch moved. **Send** uploads several drawings at once, and
  opening, **Update** and a conflict choice no longer pause the window
  ([#1219](https://github.com/Ameyanagi/ReShiki/pull/1219),
  [#1229](https://github.com/Ameyanagi/ReShiki/pull/1229),
  [#1233](https://github.com/Ameyanagi/ReShiki/pull/1233)).
- A GitHub tab's right-click menu, the command palette and the read-only chip
  have **Show in Finder** (**Show in Explorer** on Windows, **Open containing
  folder** on Linux) for the drawing's working copy
  ([#1216](https://github.com/Ameyanagi/ReShiki/pull/1216)).

## Compare

- **Chemistry and layout apart.** **Compare** in **History** and the
  **changed on GitHub** dialog paint only real chemical edits. Moving a
  molecule, or the whole scheme, is one **Moved** line. A badge says
  **Chemically identical**, **1 chemical change** or **Couldn't verify all
  chemistry**; **Chemistry**, **Layout** and **Everything** filter the marks
  and the list. **Line up moved molecules** shows them where the other version
  has them, and **What changed** lists each change with its SMILES before →
  after. The open drawing is on the right, framed in green as **CURRENT**; the
  compared version is on the left, in dashed grey as **COMPARED**
  ([#1222](https://github.com/Ameyanagi/ReShiki/pull/1222)).
- **Flips, scaling and atom numbers.** **MIRRORED** and **SCALED** lines tell
  a flipped or scaled molecule, shape or scheme from a redraw. A molecule
  flipped with the Flip tool keeps its configuration and is not a chemical
  change; a mirror image drawn without swapping wedges reads **Mirror image:
  every stereocenter inverted**. Chemical lines use the drawing's own atom
  numbers, such as **OH added at C4** or **Cl → Br at C2**
  ([#1236](https://github.com/Ameyanagi/ReShiki/pull/1236)).
- **Matched by structure.** A molecule that was both edited and given new
  ids, as after importing a scheme again, is matched by its structure: adding
  a nitro group is one line, **NO₂ added**, marked **matched by structure (ids
  changed)**. Unrelated, small and unclear pairs still show as removed and
  added ([#1238](https://github.com/Ameyanagi/ReShiki/pull/1238)).
- A wide reaction scheme stacks the two drawings when that shows them larger,
  a long SMILES wraps between atoms, and clicking a change zooms both drawings
  to it with its whole molecule in view
  ([#1235](https://github.com/Ameyanagi/ReShiki/pull/1235),
  [#1237](https://github.com/Ameyanagi/ReShiki/pull/1237)).

## Captions, names and NMR

- **Properties under a molecule.** Right-click a molecule and choose **Show
  properties**, or click the pin beside a row in **Molecular properties**.
  The caption shows Formula, Mol. Wt., Exact Mass, cLogP, TPSA or elemental
  analysis, one line each, and updates with every edit in the same Undo step.
  Editing its text makes it ordinary text; deleting the molecule deletes it.
  CDXML and CDX save it as ChemDraw linked property text, and ChemDraw files
  with linked property text, which used to be refused, open with the link. A
  caption with cLogP or TPSA is saved as plain text
  ([#1197](https://github.com/Ameyanagi/ReShiki/pull/1197); fixes
  [issue #1175](https://github.com/Ameyanagi/ReShiki/issues/1175)).
- **The name above the properties.** **Show properties** has a **Name** row,
  and **Show chemical name** shows the same row: the name in bold, with the
  other properties below it in one caption. The automatic name follows
  structure edits in the same Undo step. Type a name such as `Compound 3a` in
  the new **Name** field of **Molecular properties**, or double-click the name
  on the canvas; ↺ returns to the automatic name. A custom name stays when the
  structure changes, and the panel notes that it changed. CDXML and CDX save
  the name as plain text above the linked lines
  ([#1230](https://github.com/Ameyanagi/ReShiki/pull/1230)).
- **Predict NMR from scripts.** `Molecule.predict_nmr("13C")` in
  [Python](python.md#predict-nmr), `reshiki --cli predict-nmr` on the
  [command line](agent-cli.md#predict-nmr) and the `predict_nmr`
  [MCP tool](agent-api.md#tools) return the rows the **Predict NMR…** palette
  lists, with the broad-match flag, and the method, data version and data
  credit that **Export…** writes. It runs offline
  ([#1231](https://github.com/Ameyanagi/ReShiki/pull/1231); designed by
  @ssiddhantsharma in
  [issue #1203](https://github.com/Ameyanagi/ReShiki/issues/1203)).
- **NMR shifts on the atoms.** `with_labels(shifts)` in
  [Python](python.md#label-the-atoms), `convert --nucleus 13C --labels value`
  on the [command line](agent-cli.md#nmr-labels) and the `label_atoms`
  [MCP tool](agent-api.md#tools) write the shifts as atom numbers, so `.rsk`,
  SVG, PNG, PDF, CDXML and CDX files keep them. By default a label is the bare
  shift journals print (`1.28`, with "δ (ppm)" stated once in the caption);
  `#3`, `1.28 ppm` and `#3 · 1.28 ppm`, as in **Show labels**, are also
  offered. The command line gives ¹H shifts 2 decimal places and ¹³C 1;
  Python gives 1 only when every row is a ¹³C shift, and MCP `label_atoms`
  gives 2. Pass `decimals` (0 to 4) to choose.
  Labels sit right beside their atoms, clear of bonds and other labels, with a
  thin leader line only when there is no room. `to_rdkit()` and `from_rdkit()`
  carry atom numbers as RDKit's `atomNote`. **Show labels** in the app stays
  an on-screen overlay
  ([#1234](https://github.com/Ameyanagi/ReShiki/pull/1234); designed by
  @ssiddhantsharma in
  [issue #1203](https://github.com/Ameyanagi/ReShiki/issues/1203)).

## Drawing and Clean up

- **Clean up applies at once.** It cleans the selection, or the whole drawing
  when nothing is selected, as one Undo step; Undo and Redo compare the result
  with the original, so the preview, **Show original**, **Apply** and
  **Cancel** are gone. The drawing keeps its size, and six-membered rings turn
  upright by at most 30°. The ▾ beside the toolbar button sets the orientation
  and what a selection cleans
  ([#1207](https://github.com/Ameyanagi/ReShiki/pull/1207)).
  [Selection cleanup](selection-cleanup.md).
- **Upright rings, kept size.** The ring tool draws at the document's bond
  length, so a Nature-style drawing gets 10.8 pt rings, and hexagons, squares
  and octagons stand upright with vertical sides, as in ChemDraw. New
  structures from SMILES or names, and the bundled templates, also stand
  six-membered rings upright
  ([#1207](https://github.com/Ameyanagi/ReShiki/pull/1207)).
- **Rings join the atoms they land on.** A ring placed on an atom or bond now
  shares an existing atom under one of its other corners instead of stopping
  with “Overlaps an existing atom”, so a ring can be fused into the bay between
  neighbouring rings. The shared atom still needs a free valence for each bond
  the ring adds to it. A ring that is already drawn is refused with “The ring
  already exists”
  ([#1241](https://github.com/Ameyanagi/ReShiki/pull/1241)).
  [Selection and ring placement](selection-and-ring-placement.md).
- **Large rings stay round.** A ring of nine or more atoms that shares no atom
  with another ring, such as cyclododecane or 18-crown-6, is cleaned up as a
  regular polygon with its substituents pointing outward. A trans double bond
  keeps its inward corner. Fused, spiro and bridged large rings are unchanged
  ([#1208](https://github.com/Ameyanagi/ReShiki/pull/1208)).
- **Clean up** with **Keep orientation** no longer mirrors the drawing
  ([#1179](https://github.com/Ameyanagi/ReShiki/pull/1179); fixes
  [issue #1121](https://github.com/Ameyanagi/ReShiki/issues/1121)). With
  **Selected atoms**, it no longer folds the selected atoms onto the rest of
  the molecule, as a selected spiro ring could from a squashed layout; a
  warning says when the result is crowded
  ([#1186](https://github.com/Ameyanagi/ReShiki/pull/1186); fixes
  [issue #1181](https://github.com/Ameyanagi/ReShiki/issues/1181)).
- A pasted drawing wider than a quarter of the view lands one bond length to
  the right of the drawing and scrolls into view, instead of on top of it.
  Switching the canvas between light and dark is no longer an Undo step and no
  longer marks the drawing as changed
  ([#1205](https://github.com/Ameyanagi/ReShiki/pull/1205)).
- Pasting a MOL file, SMILES or a ChemDraw structure, and **Insert** on the
  Import tab, no longer turn **Show stereodescriptors** off for the new atoms.
  The paste is one Undo step
  ([#1176](https://github.com/Ameyanagi/ReShiki/pull/1176); fixes
  [issue #1151](https://github.com/Ameyanagi/ReShiki/issues/1151)).
- [Keyboard drawing](3d-keyboard-drawing.md) keys that add atoms (`1`, `2`,
  `4`, `5`, `8`, `9`, `0`, `z`, `K`, `k` and group keys on an atom with two or
  more bonds) now work on a molecule that has a reaction role. They were
  refused with "Assign complete molecules to reaction roles". Clicking an
  aromatic ring atom of such a molecule with the bond tool works again too
  ([#1244](https://github.com/Ameyanagi/ReShiki/pull/1244)).
- **Move & attach** no longer refuses a molecule with a name caption, a depth
  appearance or a 3D model, or an arrow with condition text; they move with
  the fragment. Dragging a ring onto a bond no longer fuses it when a ring atom
  carries a radical, lone pair or other mark, or a stereo group, which fusing
  used to delete without notice
  ([#1179](https://github.com/Ameyanagi/ReShiki/pull/1179)).
- When **Align** or **Distribute** moves an arrow attached to another aligned
  object, the arrow moves as drawn and that end detaches. Before, the result
  depended on which object was farther left or higher
  ([#1191](https://github.com/Ameyanagi/ReShiki/pull/1191)).
- **ATOMS** has its own line for **\*** (dummy atom) and a new **R** button,
  which draws an R group: a `*` atom labeled R, even when your abbreviations
  define a group named R. **Fe** left the palette; type `Fe` in the Atom
  tool's symbol field
  ([#1196](https://github.com/Ameyanagi/ReShiki/pull/1196)).
- After an edit that ReShiki cancels, such as a paste that would join
  separate reaction participants, removed objects are no longer selected.
  **Invert selection** shows the newly selected bond's color in **Properties**
  ([#1176](https://github.com/Ameyanagi/ReShiki/pull/1176),
  [#1186](https://github.com/Ameyanagi/ReShiki/pull/1186)).

## Settings, copy and export

- **Settings** gathers the app-wide preferences on one page: appearance,
  drawing, copy and export, names and checks, the assistant and updates. Open
  it with **Cmd+,** (**Ctrl+,** on Windows and Linux), the gear button at the
  right of the toolbar, or **Settings** in the command palette. **Search
  settings** finds a setting by name, each change saves at once, and each
  section has **Restore defaults**. The controls you use today stay where they
  are ([#1192](https://github.com/Ameyanagi/ReShiki/pull/1192)).
  [Settings](settings.md).
- **Backgrounds.** Copied pictures and exported files can have a
  **Transparent**, **Canvas**, **White**, **Black** or **Custom** background,
  with separate defaults: copies stay transparent and exports keep the canvas.
  On white, black or a color, black and white ink swap so a dark drawing stays
  readable. The **Export** tab has the same **Background** choice, and the
  right-click menu has **Copy image on** for one copy.
  [Background](figure-export.md#background).
- **Resolution.** Copies and exports each have a PNG resolution of 150, 300,
  600 or 1200 dpi. Copies now default to 300 dpi, which pastes faster and
  smaller; exports stay at 1200 dpi. The size in centimeters is the same at
  every setting. **Settings → Copy & export** previews each choice, and the
  **Export** tab has a **Resolution** menu with the size in centimeters and
  pixels. [PNG resolution](figure-export.md#png-resolution).
- **Put the editable drawing in pictures** now also applies to **Copy**,
  **Copy image** and **Copy as**. With it off, copied pictures carry no
  drawing, and on Windows **Copy** pastes into Office as a picture. Pasting
  between ReShiki windows still keeps the drawing, except in the browser
  editor. **Editable drawing** in **Copy image on** changes it for one copy,
  and **Include editable drawing** on the **Export** tab for that tab's
  exports. A PNG's editable drawing now sits after the image data
  ([#1192](https://github.com/Ameyanagi/ReShiki/pull/1192)).

## Files, import and export

- **Open** shows a ChemDraw (`.cdxml`, `.cdx`) or MOL file as itself: the tab
  reads `aspirin · MOL`, with no unsaved dot until you edit. The first **Save**
  asks once whether to **Keep as MOL** (or CDXML, CDX) or **Save as
  ReShiki**. **Save as…** lists **ReShiki (.rsk)**, **ChemDraw (.cdxml)**,
  **ChemDraw (.cdx)** and **MOL**; the browser still saves `.rsk` only
  ([#1205](https://github.com/Ameyanagi/ReShiki/pull/1205)).
- **Open** reads SD files (`.sdf`): the records' structures open side by side
  in one unsaved drawing named after the file, and the status says how many
  were left out when they do not all fit. **Save** keeps it as a `.rsk` and
  leaves the SD file as it is
  ([#1212](https://github.com/Ameyanagi/ReShiki/pull/1212)).
- **ChemDraw import.** Diagrams with several molecules open correctly when the
  document uses a non-default bond length, and pasting keeps the authored
  arrangement. Supported pages are arranged vertically on one canvas, with a
  notice about page settings that cannot be kept. Hidden ChemDraw objects stay
  hidden, atom labels keep inherited bold, italic, underline and color, and
  stationery files (`.cds`) with artwork open as drawings. A failed editable
  copy no longer silently falls back to SMILES. Opening, copying, exporting,
  recalculating properties and background label updates keep linked arrows and
  captions where they are, in `.rsk` drawings too. Generic choices and polymer
  repeats save as editable CDXML/CDX without expanding them first
  ([#1214](https://github.com/Ameyanagi/ReShiki/pull/1214)).
  [Corpus and repair record](changes/chemdraw-import-corpus-2026-11-17.md).
- Files that start with a UTF-8 byte-order mark import; an RXN file used to
  fail with "Missing RXN header". A MOL or RXN `ZCH`, `HYD` or `IMPL_H` value
  out of range imports without that value and with a warning, instead of
  wrapping (a charge of 200 became -56). SMILES and SMARTS text is trimmed the
  same way on every platform, so a pasted structure followed by a no-break
  space no longer fails on Linux arm64. Saving over a file keeps its
  permissions, and saving through a symbolic link writes to the linked file
  (macOS and Linux) ([#1179](https://github.com/Ameyanagi/ReShiki/pull/1179)).
- **Limits that protect the app.** A text box holds at most 64 KB of text and
  a drawing 1 MB of text in all; a file over these limits is refused. A CDXML or ChemDraw style file nested more than 64 levels
  deep, with more than 256 attributes on one element, or with entity or other
  DTD declarations is refused with a message. A `.rsk` file whose pictures
  exceed 64 MB or 64 million pixels is refused before any picture is decoded.
  A PNG is at most 32,767 pixels on a side: export lowers the resolution or
  says the drawing is too large
  ([#1184](https://github.com/Ameyanagi/ReShiki/pull/1184)). Opening a named
  pipe or a folder fails at once with `not a regular file`, and a structure
  file over 16 MB fails with `File exceeds 16 MB` after reading 16 MB of it
  ([#1185](https://github.com/Ameyanagi/ReShiki/pull/1185)).

## Agent tools, command line, Python and JavaScript

- **One result shape (breaking).** Every MCP tool returns `{value, warnings,
validation, versions}`. `info`, `document_new`, `document_list`,
  `document_close` and `inspect` used to return their fields next to
  `versions`; read them under `value` now. `compose` reports `bounds` as
  `{min, max}`. `operation_api` is 2.
  [Changes in operation API 2](agent-api.md#changes-in-operation-api-2)
  ([#1188](https://github.com/Ameyanagi/ReShiki/pull/1188)).
- **Errors you can act on.** A tool error is `{code, message, retryable}`,
  with `reason`, `param` or `current_revision` where they apply. On the
  command line an error is `reshiki: error[code]: message`. In Python, errors
  are `reshiki.ReShikiError` (a `ValueError`) with `.code`, and in JavaScript
  every rejected `Error` has `.code`
  ([#1188](https://github.com/Ameyanagi/ReShiki/pull/1188)).
- **Names and conversion.** `import` reads a chemical name with
  `format: "name"`, and the new `name` tool returns a drawing's systematic name
  with its status, aliases and alternatives, offline. A name's axial
  configuration is kept, so `(1M)-` and `(1P)-[1,1'-binaphthalene]-2,2'-diol`
  import as two structures. The new `convert` tool reads structure text and
  returns a file or a PNG preview in one call. The tool list every MCP client
  loads is shorter: `compose` no longer lists the sketch diagram fields, which
  the new `schema` tool returns when an agent needs them
  ([#1177](https://github.com/Ameyanagi/ReShiki/pull/1177),
  [#1188](https://github.com/Ameyanagi/ReShiki/pull/1188)).
- **The same names everywhere.** `rsk` and `reshiki` both name a ReShiki
  drawing in MCP, the command line, Python and JavaScript. `cdx`, `rxn` and
  `rsmi` can be written by `export`, `file_save`, `reshiki --cli convert` and
  Python. The SVG text option is `svg_text` (`svgText` in JavaScript)
  everywhere, and text is outlined unless it is `live`. A tool call may leave
  out an argument that can be `null`, the server's instructions name the
  granted folders, and `reshiki --cli name --json` also prints `warnings` and
  `versions` ([#1177](https://github.com/Ameyanagi/ReShiki/pull/1177),
  [#1188](https://github.com/Ameyanagi/ReShiki/pull/1188)).
- **Python.** `export()` is the new name of `render()`, which stays.
  `save(path, overwrite=False)` raises `FileExistsError` instead of replacing a
  file. `properties()` rounds to 9 decimal places, as the command line and MCP
  do, and the writers have typed overloads. A PNG, SVG or PDF that carries a
  ReShiki drawing opens as that drawing in Python and `reshiki --cli`
  ([#1188](https://github.com/Ameyanagi/ReShiki/pull/1188),
  [#1228](https://github.com/Ameyanagi/ReShiki/pull/1228), part of
  [issue #1183](https://github.com/Ameyanagi/ReShiki/issues/1183)).
  [Python](python.md).
- **JavaScript (breaking).** The headless library moves to its own npm
  package, `@ameyanagi/reshiki-headless`: run
  `bun add @ameyanagi/reshiki-headless` and import from it instead of
  `@ameyanagi/reshiki`; the functions are unchanged. `@ameyanagi/reshiki` now
  holds only the editor, 15.1 MB instead of 20.6 MB
  ([#1202](https://github.com/Ameyanagi/ReShiki/pull/1202)).
  [Moving from 0.15](../packages/headless/README.md#moving-from-015).
  `convert(…, "svg")` and `renderSvg` outline text by default; pass
  `{ svgText: "live" }` for the earlier output
  ([#1188](https://github.com/Ameyanagi/ReShiki/pull/1188)).
- **Browser editor.** An Arial added with `addFont` is used instead of the
  bundled Liberation Sans stand-in. Changing the `src` or `format` attribute of
  `<reshiki-editor>` loads the new drawing after a cancelable `beforeload`
  event. "Opening…" and "Preparing … export…" show before the work holds the
  page ([#1228](https://github.com/Ameyanagi/ReShiki/pull/1228); fixes
  [issue #1160](https://github.com/Ameyanagi/ReShiki/issues/1160), part of
  [issues #431](https://github.com/Ameyanagi/ReShiki/issues/431) and
  [#430](https://github.com/Ameyanagi/ReShiki/issues/430)). `frame.html`
  answers only the page that embeds it and loads fonts only from that page's
  origin or its own ([#1185](https://github.com/Ameyanagi/ReShiki/pull/1185)).
- **Budgets.** `apply` refuses an edit over the object or session weight
  limit before it reconciles the drawing, `inspect` time grows with the
  drawing's size (45,000 bonded atoms: 1.0 s instead of 30.7 s in a
  development build), session documents count text toward their weight, and
  InChI export fails with `InChI has no identifier for this drawing` instead
  of an empty file ([#1184](https://github.com/Ameyanagi/ReShiki/pull/1184),
  [#1185](https://github.com/Ameyanagi/ReShiki/pull/1185)).

## Interface and accessibility

- The interface uses the platform's own sans-serif: Helvetica Neue on macOS,
  Segoe UI on Windows and Noto Sans on Linux, so "…" no longer reads as "···".
  CJK text comes from the system's CJK font for your preferred languages.
  Drawing labels keep their own fonts
  ([#1200](https://github.com/Ameyanagi/ReShiki/pull/1200)).
- The command palette opens on a **Start here** group before any command has
  run. Help rows that are also commands are marked **Run ↩**. Keyboard focus is
  a ring around the control. Screen readers reach the document and side panel
  tabs, the text style buttons, the font, journal preset and color theme
  lists, and the light/dark canvas button; **Tab** reaches the **Font** box
  ([#1200](https://github.com/Ameyanagi/ReShiki/pull/1200)).
- The **ATOMS** buttons and **Help** stay in view at the bottom of the left
  palette at every window size, tools are paired by family, and the right
  panel keeps one width, 320 px, on every tab. Panel actions look like
  buttons, a chemistry error shows under the summary, and the **View** bar
  wraps on a narrow window instead of hiding **Done**
  ([#1196](https://github.com/Ameyanagi/ReShiki/pull/1196)).
- The context row lists selection commands first and folds the last ones into
  **⋯** when short; its tooltips show the shortcut. **Check drawing**
  findings stay as a count on its button until the drawing changes, and
  clicking one selects and shows its atoms. The status bar stays on one row.
  Hints name ⌥ on macOS and Alt elsewhere
  ([#1195](https://github.com/Ameyanagi/ReShiki/pull/1195)).
- Each command has one name on every surface, such as **Align center**,
  **Recalculate** (was Check structure) and **Abbreviate selection…** (was
  Create group label…). Menu items use sentence case, status messages state
  the result, and the app writes US English
  ([#1198](https://github.com/Ameyanagi/ReShiki/pull/1198)).

## Performance

- With everything selected and **Properties** open, a frame of a 7,700-atom
  drawing takes about 20 ms instead of 580 ms; an unchanged large drawing
  redraws in about 3 ms instead of 14 ms; and the **Templates** tab draws each
  thumbnail once (2 ms instead of 44 ms per frame)
  ([#1189](https://github.com/Ameyanagi/ReShiki/pull/1189)).
- Undo history has one memory budget for all tabs, about 256 MB, where each
  tab had 128 MB; tabs behind the front one give way first. Five tabs with a
  7,700-atom drawing each hold about 320 MB of history instead of 700 MB
  ([#1189](https://github.com/Ameyanagi/ReShiki/pull/1189)).
- In drawings of many separate structures, redrawing, measuring a selection,
  **Align** and **Distribute** take time in step with their size: **Align
  left** on 7,728 atoms is about 20 times faster. A long peptide redraws
  about a quarter faster
  ([#1191](https://github.com/Ameyanagi/ReShiki/pull/1191)).
- When the structures a ring, template or bond preview changes stay clear of
  the rest of the drawing, the preview can redraw only those structures. Panning a
  zoomed-out drawing moves a prepared picture instead of redrawing every
  bond ([#1204](https://github.com/Ameyanagi/ReShiki/pull/1204)).
- The executable is smaller: embedded data is compressed (2.66 MB to
  0.50 MB), TLS uses ring instead of aws-lc, and the macOS arm64 executable
  goes from 81.4 MB to 72.6 MB. The update check and online name lookup no
  longer offer the post-quantum hybrid key exchange
  ([#1202](https://github.com/Ameyanagi/ReShiki/pull/1202)).

## Desktop

- **Data folder.** On macOS and Linux, ReShiki keeps its data in
  `~/.reshiki/`: settings, templates, recovery drafts and GitHub working
  copies. The first launch moves the old folder
  (`~/Library/Application Support/dev.reshiki.ReShiki/` on macOS,
  `~/.local/share/reshiki/` on Linux) there in one step; if it cannot, ReShiki
  keeps using the old folder and says why in the status bar. Windows keeps
  `%LOCALAPPDATA%\reshiki\ReShiki\data\`. **About ReShiki** shows the data
  folder with **Open**, and the
  [install guide](https://reshiki.com/guide/install/#uninstall) says how to
  uninstall completely ([#1216](https://github.com/Ameyanagi/ReShiki/pull/1216)).
- **Updates.** An update downloads to `updates/<version>/` in the data folder
  and is verified there, instead of a hidden folder beside the app. ReShiki
  removes it once installed, with the previous version, and cleans up what an
  interrupted update left ([#1225](https://github.com/Ameyanagi/ReShiki/pull/1225)).
  It checks the update again on restart and installs nothing if a file
  changed. On Linux an archive with a link is refused; on Windows a signed
  ReShiki installs only an installer signed by the same publisher
  ([#1185](https://github.com/Ameyanagi/ReShiki/pull/1185)).
- **Settings and drafts from a newer version.** Interface or assistant
  preferences this version cannot read are no longer lost: ReShiki keeps a
  copy as `appearance.json.bad` or `assistant-preferences.json.bad` before the
  first changed setting replaces them, and does not save the setting if the
  copy fails. Recovery drafts made with a newer ReShiki are no longer hidden
  ([#1176](https://github.com/Ameyanagi/ReShiki/pull/1176),
  [#1179](https://github.com/Ameyanagi/ReShiki/pull/1179),
  [#1180](https://github.com/Ameyanagi/ReShiki/pull/1180)).
- On macOS, closing the Print panel returns keyboard focus to the drawing
  (fixes [issue #1149](https://github.com/Ameyanagi/ReShiki/issues/1149)). On
  Windows, several drawings from the same Office application get numbered
  tabs (fixes [issue #1119](https://github.com/Ameyanagi/ReShiki/issues/1119)).
  **Expand generic structures** is available again after an expansion
  finishes behind the recovery drafts dialog, and a drawing opened from Office
  or LibreOffice no longer stays busy when its document closes
  ([#1176](https://github.com/Ameyanagi/ReShiki/pull/1176)).
- **Office and Linux hardening.** LibreOffice and Microsoft 365 previews refuse
  drawings over 100,000 objects and run under a 2 GiB memory limit. The
  LibreOffice extension starts the installed ReShiki before one on `PATH`. The
  Microsoft 365 companion serves its task pane only to the address in the
  manifests `setup.js` generates; run `setup.js` with a new `--directory` and
  sideload its manifests again. On Linux without `XDG_RUNTIME_DIR`, ReShiki no
  longer uses a folder under `/tmp` to find its window
  ([#1185](https://github.com/Ameyanagi/ReShiki/pull/1185)).
- **Assistant.** It defaults to GPT-6.2 Sol (`gpt-6.2-sol`) when the connected
  Codex account offers it; a model you chose stays chosen
  ([#1223](https://github.com/Ameyanagi/ReShiki/pull/1223)). **Accept all
  edits** waits for **Apply** when a draft replaces objects outside your
  selection, and a request stops if Codex runs anything but the drawing tools
  or grows too long ([#1185](https://github.com/Ameyanagi/ReShiki/pull/1185)).
  [Assistant](assistant.md#applying-and-undoing).
- **Star ReShiki.** After five exports or copies, on a later day than the
  first launch, a small card asks whether you enjoy ReShiki; it waits until the
  window has been idle for 30 seconds. Without a star, it asks again at most
  once, in a later feature release and at least 90 days later. The first
  GitHub sign-in can also ask, a few minutes later. Signed in to
  GitHub, ReShiki first asks GitHub whether you already starred it, and
  **★ Star on GitHub** stars ReShiki with your account; signed out,
  **★ Star on GitHub ↗** opens GitHub in your browser. A star is made only when
  you click
  ([#1193](https://github.com/Ameyanagi/ReShiki/pull/1193),
  [#1206](https://github.com/Ameyanagi/ReShiki/pull/1206),
  [#1220](https://github.com/Ameyanagi/ReShiki/pull/1220)).
  [Privacy policy](privacy-policy.md#github-star-request).

## Known limits

- **Naming:** some structures still get a non-preferred name or no name,
  including cumulene E/Z and some phane, salt and fused-ring names. Open naming
  issues are tracked in the
  [nazuchem issue list](https://github.com/Ameyanagi/nazuchem/issues).
- **Stereo groups:** InChI export and naming refuse grouped structures, and
  canonical SMILES does not yet pick one representative for an AND or OR
  group. A group cannot hold an atropisomer axis
  ([issue #766](https://github.com/Ameyanagi/ReShiki/issues/766)).
- **Stereochemistry:** a wedge drawn from a ring amide N makes that N a
  stereocentre ([issue #1150](https://github.com/Ameyanagi/ReShiki/issues/1150)).
  A 3D MOL file with an atropisomer axis opens without the axis in the
  drawing, with a warning.
- **Layout:** layout can shorten an exocyclic bond, such as a C=O, to clear a
  clash ([issue #1122](https://github.com/Ameyanagi/ReShiki/issues/1122)).
- **Arrows:** a multi-segment arrowed curve read from CDXML or CDX opens as a
  pen path with arrowheads, not as an arrow.
- **Relaxed 3D:** like RDKit's UFF, it bends the trans ligands of square-planar
  complexes such as cisplatin and makes K⁺–O contacts short. Haptic bonds and
  query atoms are not supported
  ([issue #371](https://github.com/Ameyanagi/ReShiki/issues/371)).
- **GitHub:** desktop only; the browser editor has no GitHub sign-in. ReShiki
  sends only `.rsk` drawings and their `.svg` previews, to each repository's
  default branch. A change to a drawing's style or settings is listed in
  Compare, not painted. See [GitHub limits](github.md#limits).
- **NMR and pictures from scripts:** the headless JavaScript package has no
  NMR prediction yet
  ([issue #1232](https://github.com/Ameyanagi/ReShiki/issues/1232)). Pictures
  written by Python and `reshiki --cli` carry no editable drawing
  ([issue #1183](https://github.com/Ameyanagi/ReShiki/issues/1183)).
- **Browser editor:** opening a large file, exporting a figure, a long Clean
  up and 3D calculations still hold the page while they work
  ([issue #430](https://github.com/Ameyanagi/ReShiki/issues/430)).
- **Browser demo:** it does not work offline and cannot be installed
  ([issue #1226](https://github.com/Ameyanagi/ReShiki/issues/1226)).
- **Linux:** toolbar icons were reported blank under Xvfb software rendering
  ([issue #1120](https://github.com/Ameyanagi/ReShiki/issues/1120)).
- **Updates on macOS:** while a Keychain prompt for the GitHub sign-in is
  unanswered, **Update and restart** cannot quit ReShiki and ends with
  "Application did not exit; nothing installed." Answer the prompt, then
  update again ([issue #1240](https://github.com/Ameyanagi/ReShiki/issues/1240)).
- **Nightly builds:** there are no Intel Mac nightlies; stable releases still
  include Intel Mac downloads
  ([#1218](https://github.com/Ameyanagi/ReShiki/pull/1218)).
- **Windows signing:** public Windows downloads are unsigned while the SignPath
  Foundation release certificate is pending
  ([issue #489](https://github.com/Ameyanagi/ReShiki/issues/489)). Linux
  downloads are unsigned.

## Build and verification

Build, CI and release checks are in the [release validation record](changes/release-0.16.0-validation.md#build-and-verification).
