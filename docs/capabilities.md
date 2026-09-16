# Capability report

Assessment date: 2026-09-16. Compared against the installed version 26 reference editor using native computer control, screenshots and menus, then tested Moruno through its actual desktop UI. Menu availability is evidence that a command exists, not proof that its full behavior was reproduced.

**Moruno does not yet have full feature parity.** Version 0.2 adds everyday editing, recovery and standalone packaging. The table records the current boundary; the original desktop test record is retained below.

The subsequent [JACS / ACS default-style update](jacs-style.md) adds publication dimensions, measured atom labels, inset ring double bonds and 1200 dpi PNG output. That update passed 20 Rust tests and 10 Python tests; the historical counts below describe earlier builds.

The [workspace redesign](workspace-review.md) adds an original vector palette, contextual controls, inspector tabs, import drawer and tool shortcuts. That update passed 21 Rust tests and 10 Python tests, including actual-viewport fitting and preservation of manual camera movement.

The chain-growth fix adds graph-aware endpoint placement, a hover preview, and reliable attachment on short drags. It passed 27 Rust tests and 10 Python tests. The subsequent bond-click update passes 28 Rust tests and adds the single → double → triple → single cycle. See the desktop regression records below.

The 2026-09-17 [selection and ring-placement update](selection-and-ring-placement.md) replaces the idle bond preview with clear atom/bond markers, fixes ring orientation at substituents, and adds drag attachment and rescaling. [Selection handles](selection-transforms.md) add proportional resizing and free rotation. Current validation: 39 Rust tests and 10 Python tests.

| Capability | Moruno status | Evidence or limitation |
| --- | --- | --- |
| Freehand atoms and bonds | Implemented; desktop tested | Drew ethanol from an empty canvas; 3 atoms, 2 bonds, `CCO` |
| Chain growth and branching | Implemented; desktop tested | Endpoint clicks build a zigzag; either end can grow; branches use open angles; short drags extend; carbon bonds remain independent of the last atom label |
| Single/double/triple bonds | Implemented; desktop tested | With a plain bond tool, clicks on an existing bond cycle single → double → triple → single; new bonds use the selected tool's order |
| Solid/hashed wedges | Implemented | Molecular stereo round-trip tests; manual creation needs broader UI testing |
| Wavy/aromatic bonds | Implemented | Wavy palette tool; aromatic ring placement and rendering |
| Ring placement | Implemented; desktop tested | 3–8 members; attached rings orient into open space and match local bond length; click/drag placement and side selection; existing isolated cycloalkanes can snap onto single bonds |
| Charges and isotopes | Implemented | Selection controls; charged/isotopic structures covered by chemistry tests |
| Selection and transforms | Implemented; desktop tested | Molecule double-click, rectangle select, move, erase, selection box with proportional corner resize and rotation handle, Shift 15° rotation snapping, stereo-preserving flip, component align/distribute |
| Undo/redo | Implemented; desktop tested | Snapshot history, up to 100 changes |
| Pan, zoom, grid and fit | Implemented | Session camera only; no printable page layout |
| Text and reaction arrows | Partial; desktop tested | Multiline editable labels and five arrow styles; no reaction semantics or rich typography |
| Structure validation | Implemented; desktop tested | RDKit sanitization, invalid-valence error tests |
| 2D cleanup | Implemented; desktop tested | Preserves stable IDs, molecular identity, drawing center and nonchemical objects |
| Formula/mass/descriptors | Implemented | Formula, molecular/exact mass, cLogP, TPSA, HBD/HBA, rings |
| SMILES | Implemented | Import/export and copy canonical SMILES |
| InChI / InChIKey | Partial | InChI import/export in UI; key calculated by worker |
| MOL | Implemented molecular subset | Parse/export; stereo identity tests; no reactions or drawing annotations |
| CDXML | Partial; exchange desktop tested | Molecular import, basic molecule/text/arrow export; see restrictions below |
| Native save/open | Implemented; desktop tested | Versioned `.moruno` JSON, complete graph and drawing objects |
| SVG / PDF / PNG | Implemented; desktop and rendering tested | Vector SVG/PDF, PNG with 1200 dpi metadata; no TIFF or print dialog |
| Default drawing style | JACS / ACS | Black, 10 pt Arial, 14.4 pt bonds, 0.6 pt lines; shared canvas and physical export settings |
| Templates and named abbreviations | Partial | Twelve insertable molecular templates; no custom library or nickname expansion |
| Reaction cleanup/mapping | Planned | Arrows are drawing objects; no reactant/product grouping or automatic mapping |
| Query structures, R-groups, polymers | Planned | Unsupported constructs are rejected where detected |
| Enhanced/non-tetrahedral stereo | Planned | Rejected by the worker |
| Systematic chemical naming | Planned | No name-to-structure or structure-to-name engine |
| Spectra / NMR | Planned | No prediction, assignment or spectrum objects |
| Rich typography, curves, brackets | Partial | Multiline labels and curved arrows; no brackets or chemical rich-text editor |
| Publication layout and printing | Partial | Cropped vector PDF output; no pages, journal styles or print dialog |
| Native binary drawing format | Planned | No binary CDX importer/exporter |
| Clipboard drawing interchange | Implemented native subset | Native objects copy/cut/paste/duplicate; text SMILES, InChI, MOL and supported CDXML paste |
| Script/plugin API | Planned | Worker protocol exists; no stable public automation API |
| Accessibility | Incomplete | Canvas and Iced controls are not exposed as semantic macOS accessibility elements |
| Distribution and recovery | Implemented local subset | Standalone Apple Silicon bundle, five-second recovery snapshots, restore as a new copy; not notarized |

## Format restrictions

Native `.moruno` is the only supported complete save format for Moruno's object model. Molecular formats intentionally represent the molecular graph, not page annotations or reaction arrows. CDXML import accepts a single page with molecules, plain text and forward arrows. Other arrow styles, unknown drawing tags and multiple pages are rejected to avoid silently discarding them. It does not preserve arbitrary external formatting, attributes or unsupported chemistry. A successful small-file exchange does not establish compatibility with every external document.

Query atoms, radicals, enhanced stereo groups and unsupported bond/stereo classes are rejected where RDKit exposes them. The finite test set cannot establish chemically complete support. Label typography, atom-label collision avoidance and dense drawings need more work.

## Desktop test record

1. Opened the controlled `ethanol.mol` fixture in the installed editor. Selected and cleaned the molecule, ran structure checking ("No errors found."), and verified undo cleared the edited state. Saved `reference-ethanol.cdxml` through its native Save As dialog.
2. Imported that XML in Moruno's chemistry backend: canonical `CCO`, formula `C2H6O`.
3. Launched the Moruno app. Imported the ethanol example, validated it, moved the selected molecule, and cleaned its layout. Saved `ui-ethanol.moruno` through the native file dialog.
4. Started a blank Moruno document, placed a six-membered ring, and replaced a ring vertex with oxygen.
5. Found a fast-drag bug through computer use. Fixed event-specific cursor tracking, then drew ethanol from scratch with two connected bond drags and terminal oxygen replacement. Structure checking returned `CCO`, `C2H6O`, 46.069 g/mol.
6. Drew a reaction arrow and placed the text `oxidation`. Verified Cmd+Z removed the label and Cmd+Shift+Z restored it. Saved `ui-drawn-ethanol.moruno`, SVG and CDXML through the desktop export dialogs.
7. Opened the exported CDXML in the installed editor. The molecule, label and arrow all appeared. Its structure check returned "No errors found."
8. Native reopening initially showed a disabled Open button for a selected valid file. Removed the extension filter and successfully reopened `ui-drawn-ethanol.moruno`; all 3 atoms, 2 bonds, text and arrow reappeared. Version 0.2 testing also observed delayed button enablement without filters, so the cause is not established.
9. Verified the unsaved-change prompt when closing a disposable test drawing, and used Discard to close it.
10. Entered invalid pentavalent-carbon SMILES in Moruno. Import failed with a visible error while the saved drawing stayed intact. A subsequent structure check recovered normally and the drawing saved successfully.

Fixtures are under `tests/fixtures/`. The reference-produced file is retained as interoperability test data. Tests use synthetic examples; no existing user research documents were edited.

Original 0.1 checks: 10 Rust tests and 9 Python tests passed. Version 0.2 passes 17 Rust tests and 9 Python tests, including rings, transforms, recovery and PDF/PNG output. Formatting and Clippy checks pass.

## Observed reference features beyond this implementation

The installed Structure menu exposed structure/reaction cleanup, label expansion/contraction, hydrogen/aromatic display toggles, multicenter/variable attachments, R-logic, reaction atom mapping and spectrum assignment. File/Edit menus exposed extensive templates and additional image, drawing and clipboard formats. These observations guided the roadmap; they are not claims that Moruno implements those commands.

## Version 0.2 desktop and packaging checks

- Placed two fused aromatic rings and checked `C10H8`, 10 atoms, 11 bonds.
- Copied and pasted the selected molecule; moved, rotated, reflected and aligned the copy. Structure checking returned `C20H16` for the two disconnected molecules.
- Placed an equilibrium arrow and a multiline label, then saved `tests/fixtures/ui-expanded.moruno`.
- Started the app with a controlled interrupted-session recovery fixture. The banner offered restoration; restoring retained both molecules, the arrow style and both label lines as an unsaved new document.
- Double-clicked an atom and verified that precisely its ten-atom connected molecule was selected.
- Exported PDF from the actual GUI, rendered it with Poppler, and visually checked bonds, spacing, arrowheads and text. The PDF remained vector-based.
- Saved the recovered drawing as a version 2 document and reopened it twice through the native Open dialog. Both molecules, multiline label and equilibrium arrow remained intact.
- Exported PNG through the actual GUI, checked its 2893 × 1411 pixel dimensions and approximately 300 dpi metadata, and visually inspected a preview.
- The standalone chemistry check succeeded from `/tmp` with `MORUNO_ROOT=/nonexistent`. This verifies use of the bundled Python/RDKit worker without the project's `.venv`.

The recovery fixture was synthetic; this was not a forced termination of a user's document. The standalone build is approximately 233 MB on the tested Apple Silicon Mac. No ruviz files were modified; integration requirements are in `ruviz-integration.md`.

## Chain-growth regression check

Reproduced through native computer use: endpoint clicks used a fixed direction, so repeated extensions appeared as one straight line and could overlap an existing bond. A short drag could also resolve its target to the source atom and make no change.

The fix chooses 120° turns from the neighboring graph, alternates along a terminal chain, and considers free angular space and nearby geometry for branches. Triple-bond and cumulative-double-bond junctions remain linear. Hover and drag previews use the same placement rules as committed bonds. Drag attachment excludes the source atom and also checks the snapped endpoint for existing atoms. Bond tools add carbon; element replacement stays in the atom tool.

In the rebuilt standalone app:

- Started an empty document and clicked six bonds to produce seven connected carbons with a visible zigzag. Check returned `CCCCCCC`, `C7H16`.
- Dragged an endpoint only six screenshot pixels; one full-length bond appeared, making 8 atoms and 7 bonds. Undid this extension.
- Grew the opposite endpoint and clicked an internal vertex to branch. Check returned `CCCCC(C)CCC`, `C9H20`, with 9 atoms and 8 bonds.
- Replaced the branch tip with O, switched back to the bond tool and extended it. The new endpoint was carbon; Check returned `C9H20O`.
- Exercised Undo/Redo, restored the all-carbon example, checked it again and saved `artifacts/chain-growth-check.moruno` through the native dialog. Reading the file confirmed nine carbon atoms and eight bonds. The artifact is local and ignored by Git.

All 27 Rust and 10 Python tests pass, as do formatting, Clippy and bundle signature verification. Six added regression tests cover connected-chain identity and geometry, Undo/Redo, growth from either end, branching and occupied positions, linear junctions, atom-label carryover, short pointer gestures, and attachment to existing atoms. Automatic chain placement is a local geometry heuristic; a single drag still creates one bond. Drag explicitly to choose another direction in a crowded drawing.

## Bond-click cycle regression check

With any single/double/triple bond tool active, clicking the middle of an existing bond cycles its order through 1 → 2 → 3 → 1. Wedge, hash and wavy tools continue to apply their selected style. Drawing a new bond still uses the selected order, and endpoint clicks still grow the chain.

Verified in the rebuilt desktop app: created a two-carbon single bond, clicked its midpoint three times, and inspected a screenshot after each click showing double, triple and single respectively. The atom/bond counts stayed at 2/1. The user's existing 22-atom drawing was preserved in a separate native file before restarting and reopened after testing. All 28 Rust tests, formatting and Clippy pass; the new regression covers each plain bond tool, unchanged atom positions, the complete cycle, Undo and wedge application.
