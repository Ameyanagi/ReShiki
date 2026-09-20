# Moruno feature status and remaining work

Updated 2026-09-20 for Moruno 0.2.0. This inventory records supported workflows,
explicit limitations and planned work. A supported subset is not a claim of
complete coverage for every chemical structure or document.

The largest remaining everyday gaps include imported images, document styles
and broader clipboard interchange. Canvas text editing supports drafts and a
caret; full rich-text caret layout remains partial. Advanced reaction, query,
polymer and spectroscopy workflows also remain incomplete.

Evidence comes from Moruno source, regression tests, saved interchange fixtures
and desktop editing checks. Detailed feature documents record the tests and
applicable boundaries.

## Workspace and typography

| Feature | Moruno status | Behavior and remaining work |
| --- | --- | --- |
| Main toolbar with persistent drawing tools | Partial | Compact palette exists; some drawing-tool families remain unimplemented. |
| General toolbar | Partial | New/open/save/undo/redo/check/cleanup exist. macOS Print is available; recent documents and broader native commands are missing. |
| Style toolbar | Supported | Persistent formatting row; applied through stored style data and shared rendering. Desktop tested. |
| Font family selector | Supported | Searchable installed fonts; label and atom-label overrides. |
| Font size selector | Supported | 4–144 pt, selected labels/ranges/atom labels. |
| Bold / italic / underline | Supported | Whole labels and UTF-8 text ranges; keyboard shortcuts in text editor. |
| Formula formatting | Supported subset | Common formula digits/charges; ambiguous notation still needs manual scripts. |
| Editable subscript / superscript | Supported | Range or whole-label formatting; atom scripts remain chemistry-derived. |
| Left / center / right / justified text | Supported | Paragraph alignment and wrapping width, shared canvas/export layout. |
| Line spacing | Supported | 1, 1.2, 1.5 and 2 UI choices; stored per annotation. |
| Text color and custom color picker | Supported subset | Five swatches and RGB hex input with All selected / Text / Bonds scope; no spectrum/wheel picker. |
| Object/bond colors and color by element | Partial | The shared palette recolors selected labels, bonds, arrows and graphics in one Undo step; stroke/fill and per-bond controls also exist; automatic color by element remains missing. |
| Character map and special-character palette | Missing | Unicode can be entered as plain text, but there is no picker. |
| Floating/show-hide toolbars | Missing | Inspector can hide; no independent Main/General/Style/Object palettes. |
| Native menus and accessible commands | Missing | Moruno has an app menu; no native File/Edit/Object/Structure/Text menus or semantic canvas accessibility. |

Typography includes stored styles, selection-aware editing, text measurement and shared canvas/export layout. See the [desktop workflow, interchange checks and remaining limits](typography.md). Relevant files: `src/typography.rs`, `src/app/typography.rs`, `src/document.rs`, `src/style.rs`, `src/scene.rs`, `src/app/workspace.rs`, `engine/worker.py`.

## Drawing tools and interaction

| Feature | Moruno status | Behavior and remaining work |
| --- | --- | --- |
| Rectangular selection, molecule selection | Supported subset | Rectangle and atom double-click; selection circle, box, resize and rotation handles already exist. |
| Lasso selection | Supported | Freeform region with preview, Shift-add and Alt-subtract; actual native pointer gesture tested. |
| Invert selection | Supported | Properties command and Shift+Cmd/Ctrl+A; respects whole groups. |
| Basic single/double/triple bonds | Supported subset | Click-to-cycle, endpoint chain growth and drag attachment exist. |
| Solid wedge / hashed wedge / wavy bond | Supported subset | Existing tools; broader stereo behavior needs differential testing. |
| Plain hashed, dashed, dotted hydrogen bonds | Supported subset | Parallel hashes, dashed coordination and explicit-H interactions; local H-to-acceptor validation. CDXML encoding tested with saved interchange fixtures. |
| Bold and hollow wedge bonds | Supported | Shared rendering, click reversal and native/CDXML persistence; visible tetrahedral stereo tested after cleanup/reflection. |
| Dative / coordinate and quadruple bonds | Supported subset | RDKit DATIVE/QUADRUPLE orders, donor/acceptor direction and quadruple drawing; limited molecular format support. |
| Bold/dashed/double-dashed multiple bonds | Supported subset | Bold double, crossed double and two partial depictions. Partial 1.5 order does not implement full tautomer/query or aromatic resonance behavior. |
| User-controlled double-bond side | Supported | Automatic/center/left/right in Properties, native/CDXML persistence and flip/reverse handling. |
| Fixed-angle / fixed-length toggles | Supported | Independent controls, Alt temporary release, 15° increments, custom preferred length and chain angle, JACS default/reset. |
| Acyclic chain and snaking-chain tools | Supported subset | Whole-chain drag, count preview, exact/capped atom counts, source/endpoint attachment and retracing; no canvas auto-pan or global obstacle routing. [Desktop evidence](chain-tools.md). |
| Rings, 3–8 members | Supported subset | Adjustable ring tool; no full family of dedicated ring buttons. |
| Cyclohexane chair conformers | Supported drawing subset | Two equal-edge chair projections with live previews, rotation, atom/bond fusion and Alt connection through a new bond. No 3D stereochemical assignment or conformational-energy model. [Ring presets](ring-presets.md). |
| Dedicated cyclopentadiene tool | Supported | Dedicated palette tool, preview, atom/bond attachment, Alt connection and Shift double-bond phase; molecular identity tested. |
| Structure perspective / 3D manipulation | Missing | Existing rotation is a 2D transform. |
| Eraser | Supported subset | Atom, bond, annotation and arrow deletion. |
| Aromatic circle display | Supported subset | A toggles selected aromatic rings between circles and alternating bonds, retaining molecular identity and editable exchange. Saturated rings are unchanged. |
| Hover-target atom/bond hotkeys | Supported subset | C/N/O/S/P/F/H replace the hovered or singly selected atom, showing valence-derived hydrogens. S/D/T set a hovered or selected bond; repeated D shifts double lines. Focused text fields retain typing. |
| General join/merge of selected fragments | Supported subset | Move & attach offers exact source atom/bond selection, preview, connect/share/fuse, cancellation and one-step Undo. Attachment chemistry remains bounded. [Workflow and checks](fragment-joining.md). |

## Templates and labels

| Feature | Moruno status | Behavior and remaining work |
| --- | --- | --- |
| Structure thumbnails | Supported | Compact category rows opening four-column structure icons with names on hover; all 81 built-ins use molecular previews. |
| Preview before insertion | Supported | Selecting a template enters placement mode without changing the document. |
| Attach template to existing atom | Supported with restrictions | Explicit new-bond connection or compatible atom sharing; exact incoming open-valence alignment and local bond length. |
| Attach template to existing bond | Supported with restrictions | Reuses the two atoms and existing bond order; rotates/scales the fragment. Saturated ring templates can also attach to double bonds, creating cycloalkenes. Common neutral five/six-member aromatic fusion can reassign Kekulé orders. |
| Choose attachment side | Supported | Drag from the target to choose a side. |
| Choose a particular source atom/bond in the thumbnail | Supported | Interactive source preview selects exact atoms/bonds; custom entries remember anchors. No silent fallback when incompatible. [Workflow and chemistry checks](template-library.md). |
| Arbitrary drag-to-join after insertion | Supported subset | Move & attach handles existing separate fragments with a chosen source anchor. Direct Select-mode snapping remains limited to isolated saturated carbon rings. |
| Template browser/search/favorites | Supported | Name/collection/SMILES search, collection and favorite filters, fitted thumbnails and attachment detail view. |
| Custom template authoring and libraries | Supported subset | Save mixed selections, edit metadata, replace drawing, remember anchor, remove/undo, persistence and Moruno collection import/export. No third-party palette or cloud-library interchange. [Evidence](template-library.md). |
| Broad chemistry template content | Expanded subset | 81 built-ins: common heterocycles, bicyclic/cage rings, 20 amino acids, five nucleobases, small molecules and two chair projections. Chemical identities are checked against frozen PubChem records. Sugars, nucleotide/peptide assemblies, polymers and specialized families remain missing. |
| Abbreviations / nicknames | Supported subset | 29 common endpoint replacements, common-group contraction, custom labels over a selected single-attachment fragment, and expansion. Full graph retained; native/CDX/CDXML exchange tested. Multiattachment and nested labels remain unsupported. [Details](abbreviations.md). |
| Direct rich-text editing on canvas | Supported subset | Click-to-type and double-click revision, local draft history, recovery and one-step document Undo. Fonts/emphasis/colors appear at the caret; complex typography uses an Appearance preview. |
| Terminal-carbon / hydrogen display controls | Supported | Skeletal, terminal, internal or all carbons; implied-H visibility and five placement choices, scoped to drawing or selection. Chemistry remains unchanged. [Atom labels](atom-labels.md). |
| Automatic atom numbering / stereochemical labels | Supported subset | Numeric/prefix/Latin/Greek sequences, custom numbers, owned draggable indicators and computed tetrahedral R/S/r/s and alkene E/Z. No allene/atropisomer/enhanced-stereo labels. [Atom labels](atom-labels.md). |

Template placement offers explicit new-bond connection, shared-atom and fused-bond modes, with exact source highlighting. Fusion can reassign Kekulé bonds in common neutral five- and six-membered aromatic cycles. General aromatic systems, merging at stereocenters, mapped atoms, explicit-H attachment sites and unsupported valences remain restricted. Cleanup requires selected atoms, previews only those atoms or their connected molecules, and keeps unrelated drawings fixed before Apply; Check refreshes labels and properties without changing the layout.

## Arrows, graphics and document composition

| Feature | Moruno status | Behavior and remaining work |
| --- | --- | --- |
| Forward/equilibrium/resonance/retro arrows | Supported subset | Eight presets with live previews, endpoint/bend editing, Reverse/Flip/Straighten and publication exports. See [arrows](arrows.md). |
| Arrowhead variants and dimensions | Supported subset | Full/half/absent heads at either end, solid/hollow/angled heads, sizes/notches, colors/strokes, unequal equilibrium, dipole and cross/hash no-go. Broad hollow bodies remain missing. |
| Electron-pushing / radical-pushing arrows | Supported drawing subset | Editable quadratic arrows with full/half heads; no chemical electron-flow semantics, atom/bond attachment or mechanism generation. |
| Editable Bézier curves / pen tools | Supported subset | Drag-created curves with anchor/control-point editing; no freehand pen or point insertion/deletion. |
| Rectangles, rounded rectangles, ellipses, arcs, lines | Supported | Editable shapes, drag preview, affine transforms and shared exports. |
| Fills, shading, shadows, line styles | Supported subset | Separate stroke/fill colors, width, solid/dashed/dotted; no gradients, shading or shadows. |
| Brackets, parentheses, braces, daggers | Supported subset | Paired or single-sided graphical brackets, parentheses, braces and free dagger symbols. No polymer meaning. |
| Chemical symbols and attachment markers | Supported subset | 18 free symbols; atom-owned charges, radicals and lone pairs with positioning/rotation. H-dot/H-dash and attachment symbols are annotations only. [Scope and exchange](scientific-symbols.md). |
| Orbitals | Supported subset | Seven node-based shapes, outline/solid/flat-gray phases, phase reversal, inspector preview and vector exports. No gradient shading or 3D orbital model. |
| TLC / gel electrophoresis plates | Missing | No chromatography objects. |
| Tables | Missing | No editable table object. |
| Imported pictures | In progress | Embedded picture model and layered rendering groundwork; native save/reopen and SVG/PDF/PNG rendering are covered by regression tests. Import/paste UI and external editable picture exchange remain missing. |
| Group / ungroup / integral groups | Supported | Nested groups with whole-molecule boundaries, member editing, integral selection and native/CDXML persistence. Molecular growth extends membership; joining grouped molecules unites overlapping groups and retains captions. |
| Add frame | Supported | Fitted brackets, parentheses, braces and boxes; grouped with selection in one undoable action. |
| Bring forward / send backward | Supported subset | Graphics can go to back/front relative to chemistry and other graphics; no general object stacking UI. |
| Alignment | Supported subset | Left/right/top/bottom edges and X/Y centers using visible bounds; groups move together. Page controls center complete connected molecules or all drawing objects without scaling. |
| Distribution | Supported subset | Horizontal/vertical group and component arrangement. |
| Numeric rotate / scale dialogs | Partial | Free rotation, proportional handles, 30° buttons exist; no numeric entry or 3D rotation. |
| Rulers / crosshair | Supported | View controls independently toggle rulers, pointer crosshair and grid. Rulers and coordinate readout use mm, cm, inches or points at the physical drawing/export scale, following zoom and pan. Canvas aids do not change or export with the document. |
| Actual-size view | Missing | Zoom is a viewport magnification, not calibrated screen-to-paper size. |
| Page size / margins / multipage layout | Supported subset | Standard/custom paper, four margins, a uniform page grid, canvas guides, navigation and multipage vector PDF; no automatic pagination or per-sheet sizes. [Publication pages](publication-pages.md). |
| Print / page setup | Supported macOS subset | Native asynchronous print dialog, page ranges, scale, Save to PDF and selection printing; preserves publication dimensions and placement. Physical printer output and other platforms remain unverified. [Workflow and checks](publication-pages.md). |
| Editable document/object styles | Partial | Stored text and graphic overrides; no editable document-wide bond/label preset. |
| Journal stationery / reusable settings | Missing | No selectable journal formats or custom stationery. |
| Document annotations / attached data | Missing | No structured document metadata or object-data attachments. |

## Chemistry, reactions and specialist workflows

| Feature | Moruno status | Behavior and remaining work |
| --- | --- | --- |
| Structure validation / 2D cleanup | Supported subset | RDKit-backed selection-only layout preview, fixed unselected atoms, per-molecule position/orientation preservation, original comparison, Apply/Cancel and one-step Undo. Reaction layout remains missing. |
| Continuous warnings on the drawing | Partial | Background chemistry refresh reports errors in Properties/Labels. No per-atom warning overlays. |
| Formula, masses, logP, TPSA, HBD/HBA, ring count | Supported subset | Current inspector shows these calculated values. |
| Periodic table window | Supported drawing subset | The atom-tool palette provides 118 elements; query-atom lists remain missing. |
| Reaction interpretation and cleanup | Missing | Arrows do not define reactants, products, reagents or reaction roles. |
| Atom mapping / clear reaction map | Missing UI/workflow | Atom map numbers can survive molecular interchange, but no mapping tool or reaction model. |
| Stoichiometry / balancing | Missing | No quantities, equivalents, yield or reaction analysis. |
| Query atoms/bonds, atom lists, R-logic | Missing | Unsupported query constructs are rejected where detected. |
| Variable / multicenter attachments | Missing | No representation or editing tools. |
| Generic-structure enumeration / SDF expansion | Missing | No enumeration or SDF workflow. |
| Polymers / repeating units | Missing semantics | Graphical brackets now exist; no repeat-unit definition, counts or polymer chemistry. |
| Radicals | Supported subset | One/two unpaired electrons and owned radical marks; see [scientific symbols](scientific-symbols.md). |
| Enhanced stereo groups | Missing | Worker explicitly rejects these. |
| Non-tetrahedral stereo | Missing | Current supported stereo is narrower. |
| Name → structure / structure → name | Missing | No systematic nomenclature or name-resolution workflow. |
| NMR prediction / spectra assignment | Missing | No NMR prediction or spectrum-assignment workflow. |
| Mass-fragmentation tools | Missing | No fragmentation model or editing workflow. |
| HELM / biomolecular workflows | Missing | No biological sequence or monomer assembly workflow. |
| Database/service integrations | Missing | No external structure lookup or connected chemistry service. |

## Files, clipboard, automation and application behavior

| Feature | Moruno status | Behavior and remaining work |
| --- | --- | --- |
| Native editable documents | Supported subset | `.moruno` retains its supported objects. |
| Binary CDX | Supported clipboard subset | Bounded reader/writer bridges the supported CDXML subset. File Open/Export UI does not yet expose CDX. [Clipboard limits](clipboard.md). |
| CDXML | Partial | One page, molecules, supported rich-text runs/paragraphs and styled straight/quadratic arrows, including text-only/graphics-only documents and supported editable vector paths. Nested mixed-object groups now work for supported children; many styles remain unsupported. See [arrow exchange limits](arrows.md), [group exchange limits](selection-and-groups.md) and [graphics](graphics.md). |
| MOL / SMILES / InChI | Present molecular subset | No reaction semantics or drawing annotations in these exports. |
| RXN / SDF / SLN and wider chemical interchange | Missing | No corresponding app workflow. |
| SVG / PDF / PNG | Supported subset | Shared scene, publication dimensions and 1200 dpi PNG. |
| Additional raster/3D formats | Missing | No TIFF or 3MF workflow. |
| Native copy/cut/paste | Supported subset | macOS uses a private drawing representation with full Moruno object data; Cut deletes only after successful clipboard write. Other platforms retain the text representation. |
| Editable structure clipboard interchange | Supported macOS subset | Native binary drawing Copy/Paste, with image alternatives. Desktop round-trip checked for a 13-atom, 13-bond molecule with separate bond/label colors. Advanced chemistry, Office and other platforms remain unverified. [Evidence](clipboard.md). |
| Copy as image / alternate identifiers | Partial | Cmd+Shift+C and Export → Copy image provide PDF/PNG/SVG and a physically sized raster drawing object on macOS. Copy SMILES exists; separate MOL/CDXML/InChIKey commands remain missing. |
| Multiple document windows / tabs | Missing | One document per Moruno process. |
| Recent documents / native file associations | Missing | No recent-file list or OS open-document handling. |
| Unsaved-change prompts / recovery | Supported subset | Existing prompts and five-second recovery; not retested in this audit. |
| Version browsing / revert | Missing | Undo/recovery are not native document-version browsing. |
| AppleScript / public scripting interface | Missing | Only an internal worker protocol exists; there is no public document scripting interface. |
| Plugin/add-in integration | Missing | No stable extension interface. |
| Accessibility / keyboard-only drawing | Incomplete | Iced workspace lacks semantic macOS controls for most drawing operations. |
| Release distribution | Partial | Local standalone Apple Silicon bundle; no notarized installer/updater or verified cross-platform packaging. |

## Next work

1. Broader clipboard compatibility, canvas image import, Office verification and cross-platform exchange.
2. Broader chemical nicknames and full rich-text caret layout.
3. Document styles, stationery, calibrated actual-size view, advanced page composition and printing on other platforms.
4. Reaction roles, atom mapping, query/polymer data, naming and spectra.
5. Native menus, accessibility, document tabs and release distribution.

Each feature needs an editing workflow, save/reopen, Undo/Redo and appropriate
export checks. JACS/ACS remains the new-document default. Runtime safety checks
must continue to pass; invalid input must preserve the user's drawing.

## Assistant and visual palettes — 2026-09-20

Implemented a Codex proposal panel with local chemistry validation, editable reaction layout, current drawing styles, review/apply/reject and one-step Undo. Results are guarded against intervening drawing changes. The toolbar now opens visual atom/bond/ring/arrow palettes; arrows include an editable elbow, and crossing bonds have automatic gaps and explicit depth controls. See [assistant](assistant.md) and [tool palettes](tool-palettes.md) for limits and validation. Desktop interaction checks for these additions remain pending.

## Assistant and interface refinement

- Japanese interface typography uses an installed Japanese sans-serif face; glyph fallback is measured consistently in drawing exports.
- Tool-family palettes use small icon grids with names on hover.
- Assistant model search, per-model reasoning/service settings and saved explicit choices are available. Automatic model selection follows the newest available GPT generation. Initial settings are Low reasoning and Standard service.
- Review edits and Accept all edits support asynchronous generation and one-step Undo for each applied proposal.
- Canvas inspection supplies live document data and a rendered image; proposal preview supplies a validated scheme image. Existing scheme objects can be replaced by ID while unrelated content is retained.
- Scheme layout supports stoichiometric coefficients, readable R groups, disconnected component spacing, participant rotation and aligned captions.
