# ReShiki 0.15.0

ReShiki 0.15.0 adds enhanced stereo groups (abs, AND, OR), arrows with nodes
in the middle, and pictures that reopen in ReShiki as editable drawings. SVG
files and clipboard SVG draw their text as outlines, and a bundled
Arial-compatible font keeps labels in place on computers without Arial. The
chemistry follows RDKit 2026.09, names come from nazuchem 0.2.0, and many
SMILES, CXSMILES, MOL and RXN inputs that earlier versions refused now import
with a warning that says what changed. Installed applications still work on
your computer without Python, RDKit or uv.

[Release downloads](https://github.com/Ameyanagi/ReShiki/releases/tag/v0.15.0) ·
[Previous release](changes-0.14.md) ·
[Release validation](release-0.15.0-validation.md).

## File compatibility

ReShiki saves each drawing at the lowest document version that can hold it:
the version of the newest feature in the table that the drawing uses, or
**30** if it uses none of them. Most drawings saved in 0.15.0 therefore still
use version 30 and open in ReShiki 0.13.1 and 0.14.0. The newest version is
**37**.

| Version | Feature in the drawing                                                                                | Opens in |
| ------- | ----------------------------------------------------------------------------------------------------- | -------- |
| 31      | A reaction label with a custom gap to its arrow                                                       | 0.14.0   |
| 32      | ORTEP thermal ellipsoids or molecule-only display on a crystal model                                  | 0.14.0   |
| 33      | A 3D model generated from a structure in 0.14.0 or later, 3D atom labels, or a plain XYZ model        | 0.14.0   |
| 34      | An amino-acid residue collapsed with two attachment points, such as `-Ala-` between Boc and OMe       | 0.14.0   |
| 35      | An arrow with nodes in the middle, or a drawing style with a **Crossing clearance** other than 1.1 pt | 0.15.0   |
| 36      | A stereocentre in an AND or OR stereo group                                                           | 0.15.0   |
| 37      | An atom with three or four unpaired electrons, such as `[C]` or `[CH]` from SMILES                    | 0.15.0   |

ReShiki 0.14.0 and earlier say that a version 35–37 drawing was made with a
newer version and ask you to update. Older drawings still open in 0.15.0.

A `.reshiki-style` file has no document version. One with a **Crossing
clearance** other than 1.1 pt cannot be loaded by ReShiki 0.14.0 or earlier,
which report an unknown field `crossing_clearance_pt`. A moved polymer repeat
label does not raise the version; it opens in 0.14.0 at its automatic position.

If you share files with someone on an older version, keep a copy saved by
that version.

## Highlights

- **Enhanced stereo groups.** Mark stereocentres as absolute (`abs`), racemic
  (`and1`) or relative (`or1`) in the **Stereo groups** inspector. MOL V3000,
  RXN V3000, CXSMILES, CDXML/CDX and the ChemDraw clipboard keep the groups.
- **Arrows with nodes.** Mechanism arrows can follow an S-shaped path with
  editable nodes, like a pen path, and stay attached at both ends.
- **Pictures that reopen as drawings.** PNG, SVG and single-figure PDF
  pictures from ReShiki carry the drawing inside them. Paste, drop or open
  one in ReShiki to get the editable drawing back.
- **SVG text as outlines.** SVG export, clipboard SVG, the agent tools,
  `reshiki --cli` and the Python package draw text as outlines, so labels look
  the same in every viewer. **Keep text editable** writes live text.
- **Labels without Arial.** A bundled Arial-compatible font (Liberation Sans)
  measures and draws labels on Linux and other systems without Arial.
- **More inputs import.** SMILES features ReShiki cannot draw, atoms with three
  or four unpaired electrons, charged RXN participants, MOL superatom groups and
  atropisomer axes the drawing cannot show now import, with a warning where
  something was dropped or assumed.
- **RDKit 2026.09 and nazuchem 0.2.0.** Layout, CIP labels, atropisomer
  detection and kekulization follow RDKit 2026.09.1. Generated names follow the
  IUPAC 2013 recommendations more closely.
- **Coordination chemistry.** **Join** bonds every selected donor to its
  nearest metal, dative arrows never disappear, and **Relaxed** chemistry
  checks optimize metal complexes in 3D.
- **Office and clipboard.** On Linux, **Copy** also gives plain-text SMILES. On
  Windows, **Copy** pastes a picture into new Outlook and web apps, and edited
  Office drawings keep their size.

The changes below keep their links to their issues and reviews. All changes in
this release are by @Ameyanagi.

## Stereochemistry

- **Enhanced stereo groups.** Select stereocentres or whole molecules and open
  **Stereo groups** in the inspector. Choose **abs**, an existing group,
  **New AND group** or **New OR group**; **Renumber groups** closes gaps in the
  numbering. Centres with the same label flip together anywhere in the
  drawing, as in a MOL file or CXSMILES. Labels `abs`, `and1` and `or1` appear
  beside the centres; turn them off with **Show stereo groups (abs, and1,
  or1)** under **Atom labels & numbering**, or with **Show/Hide stereo
  groups** in the command palette. Pasting gives incoming groups unused
  numbers, so a pasted racemate never joins an existing group. MOL and RXN
  V3000 read and write `STEABS`, `STERACn` and `STERELn`, and a drawing with
  groups is written as V3000. SMILES output adds the CXSMILES group field, for
  example `C[C@H](N)C(=O)O |&1:1|`. CDXML and CDX read and write the ChemDraw
  group properties. Files with stereo groups used to be refused.
  [Atom labels](atom-labels.md#enhanced-stereo-groups) ·
  [PR #1141](https://github.com/Ameyanagi/ReShiki/pull/1141), part of
  [issue #766](https://github.com/Ameyanagi/ReShiki/issues/766).
- **Relative CXSMILES.** A relative flag, `|r|` or `r:` on chosen fragments of
  a molecule or reaction, imports as an AND group of the configured centres,
  as RDKit reads it. Before, `r:` was ignored, so a reaction reactant came in
  as a single enantiomer
  ([#1087](https://github.com/Ameyanagi/ReShiki/pull/1087),
  [#1141](https://github.com/Ameyanagi/ReShiki/pull/1141); fixes
  [issue #998](https://github.com/Ameyanagi/ReShiki/issues/998)).
- CXSMILES stereo groups with large numbers no longer merge into another
  group ([#1086](https://github.com/Ameyanagi/ReShiki/pull/1086); fixes
  [issue #1006](https://github.com/Ameyanagi/ReShiki/issues/1006)).
- **Wedge flags follow the SMILES.** A `wU` or `wD` flag on the `/` or `\`
  bond that states a double bond's E/Z keeps that E/Z. Without coordinates,
  the `@`/`@@` tag decides the configuration in reaction SMILES and next to
  wavy bonds, as it already did for molecules. An empty CX coordinate block,
  or one with every atom at the same point, keeps the stereo the SMILES states
  ([#1095](https://github.com/Ameyanagi/ReShiki/pull/1095),
  [#1100](https://github.com/Ameyanagi/ReShiki/pull/1100),
  [#1099](https://github.com/Ameyanagi/ReShiki/pull/1099),
  [#1084](https://github.com/Ameyanagi/ReShiki/pull/1084); fixes
  [issues #1042](https://github.com/Ameyanagi/ReShiki/issues/1042),
  [#1048](https://github.com/Ameyanagi/ReShiki/issues/1048),
  [#1049](https://github.com/Ameyanagi/ReShiki/issues/1049),
  [#1046](https://github.com/Ameyanagi/ReShiki/issues/1046)).
- **Atropisomer axes.**
  - New drawings choose wedges that show every atropisomer axis and
    stereocentre. RDKit's `AtropManyChirals` example now imports
    ([#1098](https://github.com/Ameyanagi/ReShiki/pull/1098); fixes
    [issue #1060](https://github.com/Ameyanagi/ReShiki/issues/1060)).
  - When no wedges can show an axis or centre, the structure imports without
    it and a warning names it, for example "The wedges in this layout cannot
    show the atropisomer axis C17–N19; ReShiki drew the structure without that
    axis". Earlier versions refused the input
    ([#1137](https://github.com/Ameyanagi/ReShiki/pull/1137)).
  - A crossed double bond no longer hides an axis at its atom, so RXN files
    with such an axis import with it
    ([#1093](https://github.com/Ameyanagi/ReShiki/pull/1093); fixes
    [issue #1064](https://github.com/Ameyanagi/ReShiki/issues/1064)).
  - A 3D MOL file whose drawing cannot keep its axis imports with a warning
    that editing, Clean Up and export will not include it
    ([#1092](https://github.com/Ameyanagi/ReShiki/pull/1092); fixes
    [issue #1057](https://github.com/Ameyanagi/ReShiki/issues/1057)).
  - Plain SMILES has no notation for an axis. The Properties panel, **Copy
    SMILES**, SMILES export, `reshiki --cli analyze` and the Python
    `Properties` now say that the axial configuration was dropped
    ([#1089](https://github.com/Ameyanagi/ReShiki/pull/1089); fixes
    [issue #1007](https://github.com/Ameyanagi/ReShiki/issues/1007)).
- Importing an atropisomer whose axis wedge sits on a ring amide nitrogen
  gives that nitrogen the same R/S label at import that **Analyze**, export and
  reopening the drawing compute from the wedges. Before, the label appeared
  only after the next analysis
  ([#1173](https://github.com/Ameyanagi/ReShiki/pull/1173)).
- An allene no longer keeps a false configuration when another component of
  the drawing makes CIP labelling fail
  ([#1085](https://github.com/Ameyanagi/ReShiki/pull/1085); fixes
  [issue #1061](https://github.com/Ameyanagi/ReShiki/issues/1061)).
- **CIP labels follow RDKit 2026.09.1.** R/S labels change on some cationic
  heteroaromatics, partially unsaturated rings and isotope-labelled centres;
  for example, the stereocentre of `F[C@H](c1[o+]cccc1)C(O)N` is now S
  ([#1130](https://github.com/Ameyanagi/ReShiki/pull/1130),
  [#1131](https://github.com/Ameyanagi/ReShiki/pull/1131)).

## Chemical names

- Generated names follow the IUPAC 2013 recommendations more closely, with the
  naming engine updated to [nazuchem 0.2.0](https://github.com/Ameyanagi/nazuchem/releases/tag/v0.2.0).
  Symmetric structures get multiplicative preferred names, such as
  `1,1'-oxybis(4-bromobenzene)` and `4,4'-(propane-2,2-diyl)diphenol`. Salts
  list their ions alphabetically, ignoring stereo, isotope and italic
  descriptors (`disodium tert-butoxide methoxide`). A charged ring or nitrogen
  becomes the parent (`4-(hydroxymethyl)-1-methylpyridin-1-ium`), and charged
  substituents use the preferred prefixes `(N,N-dimethylmethanaminiumyl)` and
  `diazyn-1-ium-1-yl`. Isotope labels inside a substituent are named, as in
  `(2H3)methoxybenzene`, where ReShiki refused before. Diamines cite `N^1` and
  `N^3`, a nitrile in a substituent is `cyano`, ring ketones use indicated
  hydrogen (`1,3-dihydro-2H-1,4-benzodiazepin-2-one`), and parentheses and
  fusion locants are simpler (`hydroxy(phenyl)acetic acid`,
  `2H-benzo[cd][1]benzothiophene`). [2.2]Paracyclophane now shows a von Baeyer
  name with its phane name as the alternative, like other cyclophanes
  ([#1167](https://github.com/Ameyanagi/ReShiki/pull/1167)).
- The naming engines now live in their own library,
  [nazuchem](https://github.com/Ameyanagi/nazuchem), which ReShiki builds in.
  Names and their status did not change with the move, and naming still works
  offline ([#1072](https://github.com/Ameyanagi/ReShiki/pull/1072)–[#1083](https://github.com/Ameyanagi/ReShiki/pull/1083)).
- `docs/feature-status.md` lists the opt-in PubChem name lookup that 0.14.0
  added ([#1097](https://github.com/Ameyanagi/ReShiki/pull/1097); fixes
  [issue #1004](https://github.com/Ameyanagi/ReShiki/issues/1004)).

## Import and export

- **Partial imports with warnings.** A SMILES, CXSMILES or reaction SMILES
  input with one feature ReShiki cannot draw now imports the rest and says what
  was dropped or assumed. Square-planar, trigonal-bipyramidal and octahedral
  centres (`@SP`, `@TB`, `@OH`) import without a configuration, an invalid
  `@AL` tag is dropped, CXSMILES query features (`u`, `rb`, `s`, query labels)
  are ignored, a `~` bond becomes single, and a CXSMILES zero-order (`Z`) bond
  is left out. A bare `*` with a CX suffix imports with no warning
  ([#1155](https://github.com/Ameyanagi/ReShiki/pull/1155),
  [#1165](https://github.com/Ameyanagi/ReShiki/pull/1165); part of
  [issue #1138](https://github.com/Ameyanagi/ReShiki/issues/1138)).
- **Three or four unpaired electrons.** SMILES like `[C]` and `[Si]` now import
  as written: a bare carbon or silicon atom with four unpaired electrons, and
  `[CH]`, `[N]` and `[B]` with three, as RDKit reads them. The formula, mass and
  InChI match the input (`[C]` is C, 12.011, `InChI=1S/C`), SMILES and MOL
  export keep these atoms exactly, and RDKit's MOL files for them open. Drawing
  a bond to such an atom recounts its unpaired electrons instead of reporting
  an error. CDXML cannot store three or four unpaired electrons, so ChemDraw
  files keep the hydrogen count and show a doublet or triplet. Drawings with
  these atoms save as document version 37.
  [Scientific symbols](scientific-symbols.md#persistence-and-exchange) ·
  [PR #1168](https://github.com/Ameyanagi/ReShiki/pull/1168).
- **RXN participants with a charge, isotope or radical** import like any other
  atom. Earlier versions refused the whole file as a query reaction
  ([#1152](https://github.com/Ameyanagi/ReShiki/pull/1152)).
- **MOL superatom groups.** MOL export writes each collapsed abbreviation,
  including two-sided amino-acid residues, as a superatom (`SUP`) group with
  its attachment points. MOL files with superatom groups open collapsed; a
  group that cannot be one label stays expanded, with a notice
  ([#1143](https://github.com/Ameyanagi/ReShiki/pull/1143); fixes
  [issue #870](https://github.com/Ameyanagi/ReShiki/issues/870)).
  [Abbreviations](abbreviations.md).
- **CDXML generic nicknames.** `A` and `a` import as labelled dummy atoms with
  a warning that their any-atom meaning is not kept, and `*` imports as a
  wildcard atom. Before, `A` refused the file and `a` became carbon
  ([#1131](https://github.com/Ameyanagi/ReShiki/pull/1131)).
- **File → Open** refuses files it has no reader for, such as `.sdf` and
  `.txt`, with "Can't import .sdf", instead of reading them as SMILES. `.smi`
  and `.smiles` still open as SMILES
  ([#1094](https://github.com/Ameyanagi/ReShiki/pull/1094); fixes
  [issue #1001](https://github.com/Ameyanagi/ReShiki/issues/1001)).
- **Layout.** 2D layout follows RDKit 2026.09.1: substituent placement,
  collision detection and ring order
  ([#1139](https://github.com/Ameyanagi/ReShiki/pull/1139),
  [#1129](https://github.com/Ameyanagi/ReShiki/pull/1129)). Porphyrins import as
  regular macrocycles, as after Clean Up
  ([#1114](https://github.com/Ameyanagi/ReShiki/pull/1114); fixes
  [issue #1113](https://github.com/Ameyanagi/ReShiki/issues/1113)). Rings next
  to a spiro centre stay regular instead of being squashed
  ([#1116](https://github.com/Ameyanagi/ReShiki/pull/1116); fixes
  [issue #1112](https://github.com/Ameyanagi/ReShiki/issues/1112)). Imported
  components such as an HCl beside an amine keep one bond length between
  their labels ([#1088](https://github.com/Ameyanagi/ReShiki/pull/1088); fixes
  [issue #1033](https://github.com/Ameyanagi/ReShiki/issues/1033)).
- Non-canonical SMILES output keeps atom maps, as in RDKit 2026.09.1
  ([#1125](https://github.com/Ameyanagi/ReShiki/pull/1125)).

## Arrows, drawing and styles

- **Arrows with nodes.** Select one arrow and choose **Edit points**. Click the
  shaft to add a node; drag nodes and their direction controls, and switch
  them between **Corner**, **Smooth** and **Symmetric** as on pen paths.
  **Delete node** removes one. Attached ends stay attached when molecules
  move. **Smooth**, **Symmetrise**, **Straighten**, **Reverse** and
  **Flip bend** work on these arrows. CDXML and CDX write them as one
  multi-segment ChemDraw curve. Drawings with such arrows save as document
  version 35.
  [Arrows](arrows.md#add-nodes-to-an-arrow) ·
  [PR #1140](https://github.com/Ameyanagi/ReShiki/pull/1140); fixes
  [issue #761](https://github.com/Ameyanagi/ReShiki/issues/761).
- **Move the repeat label.** With a polymer bracket selected, **Repeating
  unit → Position repeat label** shows a handle on its label (`n`). Drag it, or
  type **Label X** and **Label Y**; **Restore automatic label position** puts
  it back. The label follows the bracket when it moves.
  [Generic structures](generic-structures.md#move-the-repeat-label) ·
  [PR #1110](https://github.com/Ameyanagi/ReShiki/pull/1110); fixes
  [issue #1069](https://github.com/Ameyanagi/ReShiki/issues/1069).
- **Crossing clearance.** **Properties → Drawing style → Edit drawing style… →
  Advanced stroke settings → Crossing clearance** sets the gap where a rear
  bond passes behind a front one, from 0 to 12 pt. The default, 1.1 pt, draws
  as before. A drawing with another clearance saves as document version 35.
  The Python package takes `crossing_clearance`; binary CDX reopens at 1.1 pt.
  See [File compatibility](#file-compatibility).
  [Drawing styles](drawing-styles.md) ·
  [PR #1111](https://github.com/Ameyanagi/ReShiki/pull/1111); fixes
  [issue #1070](https://github.com/Ameyanagi/ReShiki/issues/1070).
- **Join coordinates donors.** With a metal in the selection, **Join**
  (⌘J / Ctrl+J) adds a donor → metal bond from every selected N, O, S or P to
  its nearest selected metal within two bond lengths, in one Undo step.
  Selecting all of Co(en)₃ and pressing Join draws all six contacts. The status
  bar names what was added and what was skipped. Two atoms of the same metal
  now share like any other pair
  ([#1128](https://github.com/Ameyanagi/ReShiki/pull/1128); fixes
  [issue #246](https://github.com/Ameyanagi/ReShiki/issues/246)).
  [Move and attach](fragment-joining.md).
- **Dative arrows.** A metal's automatic charge sits in a gap between its
  contacts, a dative contact is never dropped, and its arrowhead scales with a
  short contact
  ([#1124](https://github.com/Ameyanagi/ReShiki/pull/1124),
  [#1126](https://github.com/Ameyanagi/ReShiki/pull/1126)).

## Pictures, SVG and fonts

- **Pictures that reopen as drawings.** **Copy**, **Copy image** and **Copy as
  → PNG / SVG / PDF** put the ReShiki drawing inside their PNG, SVG and PDF
  pictures. Paste such a picture into ReShiki, drop the file on the window or
  open it with **File → Open** to get the editable drawing back. File export
  does the same while **Include editable drawing** is on, below the export
  button. It is on by default; turn it off for figures you send to a journal.
  The drawing inside holds exactly what the picture shows. Drawings over 2 MB
  compressed, **Export pages as PDF** and **EMF for Office** do not carry it.
  [Keep the drawing editable inside a picture](figure-export.md#keep-the-drawing-editable-inside-a-picture) ·
  [PR #1153](https://github.com/Ameyanagi/ReShiki/pull/1153),
  [PR #1154](https://github.com/Ameyanagi/ReShiki/pull/1154), part of
  [issue #462](https://github.com/Ameyanagi/ReShiki/issues/462).
- **SVG files look the same in every viewer.** **Export → Figure → SVG** and
  the agent tools `export`, `file_save` and `render` draw text as outlines, as
  `reshiki --cli` and the Python package do. Live `<text>` used the viewer's
  own fonts, so labels shifted between apps, and a renderer without Arial could
  drop them. To keep text editable, turn on **Keep text editable** below the
  export button (off by default; ReShiki remembers it), pass `svg_text: "live"`
  to the tools, `--svg-text live` to the CLI or `text="live"` in Python. An
  editable drawing inside the SVG still reopens in ReShiki. **SVG for Office**
  is unchanged.
  [Text in SVG files](figure-export.md#text-in-svg-files) ·
  [PR #1158](https://github.com/Ameyanagi/ReShiki/pull/1158),
  [PR #1163](https://github.com/Ameyanagi/ReShiki/pull/1163).
- The SVG that ReShiki puts on the clipboard draws its text as outlines on
  macOS, Linux and in the browser editor, as **Copy image** on Windows already
  did. **Copy**, **Copy image** and **Copy as → SVG** used live `<text>` there,
  so an app that pastes the SVG flavor substituted its own fonts. The SVG keeps
  its physical size, transparent background and the editable drawing inside
  it. [Clipboard](clipboard.md) ·
  [PR #1164](https://github.com/Ameyanagi/ReShiki/pull/1164).
- **Labels without Arial.** Every build carries Liberation Sans, which has
  Arial's character widths. When no Arial is installed, ReShiki measures and
  draws labels with it, in the desktop app, the CLI and the Python package, so
  Linux labels no longer come out wider with DejaVu Sans. In 0.14.0 a computer
  with no fonts at all lost every atom label. Where Arial is installed, nothing
  changes. [Missing fonts](typography.md#missing-fonts) ·
  [PR #1158](https://github.com/Ameyanagi/ReShiki/pull/1158),
  [PR #1081](https://github.com/Ameyanagi/ReShiki/pull/1081).

## Clipboard and Office

- On Linux, **Copy** (**Ctrl+C**) also puts the selection's SMILES on the
  clipboard as plain text, the same text **Copy SMILES** gives, so a plain-text
  box gets the SMILES. LibreOffice, Chromium pages and ReShiki still paste the
  drawing or picture, and when ReShiki quits with its window focused, GNOME
  still keeps the picture. A reaction, or a selection without molecules, gets
  no text. Windows and macOS are unchanged, because Excel and PowerPoint there
  would paste the text instead of the drawing.
  [Clipboard compatibility](clipboard-compatibility.md) ·
  [PR #1161](https://github.com/Ameyanagi/ReShiki/pull/1161).
- On Windows, **Copy** keeps a bitmap next to the editable Office object, so
  new Outlook, Outlook on the web, Gmail, Teams and other browser-based apps
  paste a picture. Word, PowerPoint and Excel still paste the editable drawing
  ([#1148](https://github.com/Ameyanagi/ReShiki/pull/1148); fixes
  [issue #1146](https://github.com/Ameyanagi/ReShiki/issues/1146)).
- On Windows, editing an embedded drawing in PowerPoint and saving no longer
  enlarges its frame and leaves a blank band at the right and bottom. Office
  pictures are recorded at a fixed resolution, so a new paste comes out at the
  drawing's real size on every display. Objects already embedded keep their
  size until you edit them
  ([#1127](https://github.com/Ameyanagi/ReShiki/pull/1127),
  [#1144](https://github.com/Ameyanagi/ReShiki/pull/1144); fixes
  [issues #1123](https://github.com/Ameyanagi/ReShiki/issues/1123),
  [#1132](https://github.com/Ameyanagi/ReShiki/issues/1132)).
- **Clipboard compatibility.** A new page lists, for each app on Windows,
  macOS, Linux and in the browser, what **Copy** and **Copy image** paste as,
  and the exact clipboard formats.
  [Clipboard compatibility](clipboard-compatibility.md) ·
  [#1147](https://github.com/Ameyanagi/ReShiki/pull/1147),
  [#1157](https://github.com/Ameyanagi/ReShiki/pull/1157).

## 3D models and crystals

- **Relaxed chemistry checks.** The 3D preview has **Chemistry checks:
  Strict** and **Relaxed**. When **Strict** refuses a metal complex or an atom
  with an unusual bond count, such as 18-crown-6 holding K⁺, the preview opens
  with **Strict checks failed** and the reason. Choose **Relaxed** to optimize
  the molecule as drawn; molecules with metals use UFF. The panel shows **Relaxed: chemistry not
  validated** and lists each check it skipped and each approximation
  ([#1145](https://github.com/Ameyanagi/ReShiki/pull/1145), part of
  [issue #371](https://github.com/Ameyanagi/ReShiki/issues/371)).
- After **Repeat 2 × 2 × 2**, rock salt's automatic view leaves a word space
  between Na and Cl labels on the same line
  ([#1091](https://github.com/Ameyanagi/ReShiki/pull/1091); fixes
  [issue #614](https://github.com/Ameyanagi/ReShiki/issues/614)).

## Python package and browser editor

- **Online name lookup and reusable styles in Python.**
  `reshiki.Molecule.from_name("aspirin", online=True)` asks PubChem; offline
  lookup stays the default. `reshiki.with_style(...)` makes a reusable style
  context for constructors, `parse`, `render`, `convert` and `render_many`
  ([#1081](https://github.com/Ameyanagi/ReShiki/pull/1081)).
  [Python](python.md).
- **Import warnings.** The Python package issues the app's import warnings as
  `reshiki.StructureWarning`; `render_many` returns them in `.warnings`. The
  browser library has `read(data, format)`, which returns `{ drawing, warnings }`,
  and the editor's `load()` resolves to `{ warnings }`
  ([#1135](https://github.com/Ameyanagi/ReShiki/pull/1135)).
  [Import warnings](python.md#import-warnings).
- Python wheels include the license and third-party notice files
  ([#1079](https://github.com/Ameyanagi/ReShiki/pull/1079); fixes
  [issue #1077](https://github.com/Ameyanagi/ReShiki/issues/1077)).

## Desktop

- Idle windows release GPU memory: a focused text field stops blinking after
  10 seconds, and antialiasing buffers shrink with the window. On macOS, with
  the Import tab open, the app now settles at about 206 MB instead of about
  310 MB
  ([#1096](https://github.com/Ameyanagi/ReShiki/pull/1096); fixes
  [issue #1002](https://github.com/Ameyanagi/ReShiki/issues/1002)).
- On Linux, starting ReShiki from the launcher while it runs brings the open
  window forward instead of opening a second one
  ([#1115](https://github.com/Ameyanagi/ReShiki/pull/1115), part of
  [issue #100](https://github.com/Ameyanagi/ReShiki/issues/100)).
- macOS 27: debug builds no longer crash when the Print panel draws its
  preview. Release builds were not affected
  ([#1136](https://github.com/Ameyanagi/ReShiki/pull/1136); fixes
  [issue #1134](https://github.com/Ameyanagi/ReShiki/issues/1134)).

## Known limits

- **Naming:** some structures still get a non-preferred name or no name,
  including cumulene E/Z and some phane, salt and fused-ring names. Open naming
  issues are tracked in the
  [nazuchem issue list](https://github.com/Ameyanagi/nazuchem/issues).
- **Stereo groups:** InChI export and naming refuse grouped structures instead
  of giving the drawn enantiomer's identifier. Canonical SMILES does not yet
  pick one representative for an AND or OR group, so two drawings of the same
  racemate can give different SMILES. A group cannot hold an atropisomer axis;
  its atom members import and a warning names the loss
  ([issue #766](https://github.com/Ameyanagi/ReShiki/issues/766)).
- **Stereochemistry:**
  - A wedge drawn from a ring amide N makes that N a stereocentre
    ([issue #1150](https://github.com/Ameyanagi/ReShiki/issues/1150)).
  - Pasting into an empty drawing turns off **Show stereodescriptors**
    ([issue #1151](https://github.com/Ameyanagi/ReShiki/issues/1151)).
  - A 3D MOL file with an atropisomer axis opens without the axis in the
    drawing, with a warning.
- **Layout:** layout can shorten an exocyclic bond, such as a C=O, to clear a
  clash, and Clean Up with **Keep orientation** can mirror a drawing
  ([issues #1122](https://github.com/Ameyanagi/ReShiki/issues/1122),
  [#1121](https://github.com/Ameyanagi/ReShiki/issues/1121)).
- **Arrows:** a multi-segment arrowed curve read from CDXML or CDX opens as a
  pen path with arrowheads, not as an arrow.
- **Relaxed 3D:** like RDKit's UFF, it bends the trans ligands of square-planar
  complexes such as cisplatin and makes K⁺–O contacts short. Haptic bonds and
  query atoms are not supported
  ([issue #371](https://github.com/Ameyanagi/ReShiki/issues/371)).
- **Fonts:** in the browser editor, an Arial added with `addFont` is still
  drawn with the bundled Liberation Sans
  ([issue #1160](https://github.com/Ameyanagi/ReShiki/issues/1160)).
- **Desktop:** on macOS, keyboard focus does not return to the drawing after
  the Print panel closes; click the drawing
  ([issue #1149](https://github.com/Ameyanagi/ReShiki/issues/1149)). Two
  drawings from the same Office document open as tabs with the same title
  ([issue #1119](https://github.com/Ameyanagi/ReShiki/issues/1119)).
- **Windows signing:** public Windows downloads are unsigned while the SignPath
  Foundation release certificate is pending
  ([issue #489](https://github.com/Ameyanagi/ReShiki/issues/489)). Linux
  downloads are unsigned.

## Build and verification

- **RDKit 2026.09.1.** The reference suites, oracles and captured goldens
  follow RDKit 2026.9.1; ReShiki keeps its own ring-finding basis
  ([#1162](https://github.com/Ameyanagi/ReShiki/pull/1162),
  [#1142](https://github.com/Ameyanagi/ReShiki/pull/1142); fixes
  [issue #1117](https://github.com/Ameyanagi/ReShiki/issues/1117)). The live
  reference failures on main were cleared before the release
  ([#1159](https://github.com/Ameyanagi/ReShiki/pull/1159)).
- **nazuchem.** The naming engines moved into
  [nazuchem](https://github.com/Ameyanagi/nazuchem), with their history,
  licenses and a frozen snapshot of the naming worker's responses that guarded
  the move ([#1067](https://github.com/Ameyanagi/ReShiki/pull/1067),
  [#1071](https://github.com/Ameyanagi/ReShiki/pull/1071)–[#1083](https://github.com/Ameyanagi/ReShiki/pull/1083),
  [#1167](https://github.com/Ameyanagi/ReShiki/pull/1167)).
- **CI.** Linux jobs run in prebuilt images, native tests are partitioned with
  cargo-nextest, and lint and golden replay run in one job per platform
  ([#1104](https://github.com/Ameyanagi/ReShiki/pull/1104)–[#1108](https://github.com/Ameyanagi/ReShiki/pull/1108),
  [#1102](https://github.com/Ameyanagi/ReShiki/pull/1102),
  [#1103](https://github.com/Ameyanagi/ReShiki/pull/1103),
  [#1133](https://github.com/Ameyanagi/ReShiki/pull/1133)). Nightly builds
  accept their version suffix again
  ([#1101](https://github.com/Ameyanagi/ReShiki/pull/1101)). Two Windows tests
  no longer fail on a brief file lock or a slow debug build
  ([#1109](https://github.com/Ameyanagi/ReShiki/pull/1109),
  [#1166](https://github.com/Ameyanagi/ReShiki/pull/1166)). Source files are
  formatted with `bun fmt`
  ([#1068](https://github.com/Ameyanagi/ReShiki/pull/1068)).
- **0.14.0 release validation.** The publication record of the tagged source,
  tag run and public downloads.
  [Validation record](release-0.14.0-validation.md) ·
  [PR #1066](https://github.com/Ameyanagi/ReShiki/pull/1066).
- **Release preparation.** Update the version, the release notes and the
  documentation for 0.15.0.
