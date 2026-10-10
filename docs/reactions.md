# Reaction roles and chemical exchange

Reaction roles connect editable molecules to a drawing arrow. They stay attached when objects move, and are saved in the native drawing. Position alone never changes a molecule from a reactant into a product.

## Define a reaction

1. Draw or import the molecules and a reaction arrow.
2. Open **Properties → Reaction roles…**, or **Export → Reaction roles & export…**.
3. Choose the arrow for this step. For several steps, each arrow has its own roles.
4. Select a molecule on the canvas and choose **Assign selected molecules** under Reactants, Products, or Reagents / catalysts. Selecting one atom assigns its complete connected molecule. You can assign several selected molecules together.
5. Click a structure thumbnail in the inspector to select that participant on the canvas. Reassigning it moves it to the chosen role. **Clear selected roles** removes its membership without deleting the molecule.

**Select reaction** selects its arrow, all participants, and associated captions. Native copy/paste and Duplicate retain a complete selected reaction and remap its object IDs. A partial molecule selection is copied as an ordinary drawing fragment.

Role edits are undoable. **Remove reaction roles** keeps all drawing objects. Deleting an arrow removes its reaction definition; deleting atoms prunes the corresponding membership. Extending an assigned molecule also extends its membership. A bond edit that joins separate participants is rejected: clear their roles first if you intend to combine them. Reversing a defined arrow swaps reactants and products in the same Undo step.

Assistant-generated schemes already contain reactant and product roles, including their coefficients. Reaction conditions remain editable captions. Text conditions do not automatically become chemical reagent structures; draw and assign a reagent when it should be included in reaction data.

## Copy a drawn reaction

Ordinary **Cmd/Ctrl+C** retains the selected editable drawing and adds chemical
text when preparation succeeds. With no selected arrow, it prepares molecular
SMILES and retains all selected disconnected molecular components. Inspect the
receiving editor's interpretation of a mixture or salt; copying all components
does not establish receiver acceptance.

For an unassigned scheme, select all complete participant molecules and exactly
one straight forward arrow. Copy-only inference assigns starting materials
toward the arrow's start and products toward its end, including vertical,
diagonal and right-to-left layouts. It groups complete connected components
and assigns each one once. Molecules touching or crossing the arrow midpoint,
unexplained central components, multiple or degenerate arrows, unsupported
arrow semantics and missing reaction sides prevent inference. Captions do not
automatically become chemical agents.

Existing explicit roles override geometry, including their coefficients and
agent membership. An incomplete explicit reaction selection cannot be
reinterpreted from its layout. Without the arrow, one complete explicit
participant can supply molecular text; partial participants and selections
spanning explicit participants are rejected for chemical text even if the
selection snapshot has pruned their reaction metadata. Unassigned no-arrow
molecules continue to use the molecular copy path.

The inferred roles exist only in the chemical export snapshot. The native
clipboard retains its existing selected-drawing metadata. Copy does not move
atoms, change the source document or create an Undo entry. Ambiguous or invalid
inference omits optional chemical text and reports the reason while ordinary
drawing Copy still succeeds. Chemical **Copy as** choices use the same selection
rules and report an error when the requested conversion cannot be prepared.

Ordinary reaction Copy uses bounded ChemDoodle JSON. Aromatic molecules use
validated canonical Kekulé bonds with preserved attached hydrogen counts.
Agents, non-unit coefficients, atom maps, stereochemistry, radicals, unsupported
bonds, collapsed abbreviations and disconnected components grouped into one
participant remain outside that JSON format. Save `.rsk` for the full drawing
and use RXN/reaction SMILES when the receiver supports those features.

See [the SciFinder handoff guide](scifinder-handoff.md) for CAS Draw's molecular
SMILES field/**Add to Editor** and ChemDoodle's reaction **Open**/**Load** field.
The October 5 check used an optimized arm64 QA build at `f648465` with Edge on
macOS: ordinary reaction Copy reached ChemDoodle's Open text field, and Load
showed the complete benzene-to-cyclohexane reaction with correct roles. Direct
Cmd+V in its drawing canvas inserted an older internal six-atom Cyclohexane
instead. CAS Draw's canvas did not import the no-arrow two-ring SMILES or MOL
clipboard data. Real Cmd+V of the two-ring SMILES into its text field followed
by Add accepted both molecules. Center Structure revealed the complete Benzene
with three double bonds and Cyclohexane with six single bonds; the screenshot
showed `C6H12 (84.16) . C6H6 (78.11)`. MOL was checked only for text-field
delivery, without Add/conversion. No searches were submitted, and
vertical/reversed layouts remain covered locally rather than by these live
checks.

The accepted workflow is ReShiki Cmd+C → Cmd+V into ChemDoodle's Open text
field → Load. Both complete participants and their reactant/product roles were
preserved in the tested benzene-to-cyclohexane fixture.
[PR #144](https://github.com/Ameyanagi/ReShiki/pull/144) added this
workflow; direct canvas paste and CAS reaction integration are outside this
change. The guide records the exact actions, baseline comparison and limits.
Local fixtures can be prepared without publishing clipboard data or contacting
CAS:

```sh
cargo run --locked --example scifinder_clipboard_qa -- /tmp/reshiki-scifinder-clipboard
```

## Import and export

Open `.rxn` or `.rsmi` files, or paste RXN text or reaction SMILES into Import. Reaction SMILES uses `reactants>agents>products`, for example:

```text
CC(=O)O.CCO>OS(=O)(=O)O>CCOC(C)=O.O
```

The importer lays out editable molecules, separators and an arrow, with chemical agents above the arrow. Existing molecular coordinates within each RXN participant are retained; page and arrow layout are recreated.

Choose one arrow in the reaction inspector to export it. Export controls remain visible while the participant list scrolls. Both sides must contain a participant.

| Format                    | Retained data                                                                                             | Limits                                                                                                                     |
| ------------------------- | --------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `.rsk`                    | Complete drawing, reaction roles, coefficients, captions, styles and groups                               | Use this for continued editing.                                                                                            |
| RXN V3000                 | Reactants, products, separate agents, atom maps, supported molecular stereochemistry and atom coordinates | Arrow appearance, captions and page layout are not stored.                                                                 |
| Reaction SMILES (`.rsmi`) | Reactants, products, agents, atom maps and supported molecular stereochemistry                            | Coordinates and drawing appearance are not stored. Disconnected components can become separate participants when reopened. |

Coefficients greater than one export as repeated chemical participants. Repeated mapped participants need distinct atom maps; the exporter rejects duplicate map numbers on one side. Query atoms, enhanced stereo groups, hydrogen interactions, partial and quadruple bonds are not supported by this reaction-exchange workflow and produce an error rather than a flattened structure. The expanded export is limited to 10,000 atoms.

Molecular MOL/SMILES/InChI and current CDXML/CDX drawing exchange do not retain these reaction roles. The native clipboard representation preserves them when pasting a complete reaction into ReShiki. Save `.rsk` alongside an exchange file to retain the full scheme.

## Atom mapping and label placement

Mapped reaction SMILES already use atom classes such as `[CH3:1][CH2:2][OH:3]>>[CH3:1][CH:2]=[O:3]`. Imported positive maps are visible by default. **Show maps** controls their ink, while the chemical map numbers remain in the document and RXN/reaction SMILES export. A map label derives its text from the atom's map number and has its own position/style; custom atom numbers such as `Cα` remain independent and can appear alongside it.

Select one atom and use **Atom map → Set**, or right-click **Edit atom map…** / **Clear atom map**. Enter a positive map up to 2147483647; 0 clears it. Edits check uniqueness on that reaction side and element/isotope agreement with an existing counterpart. Select one reactant atom and one product atom, then **Pair selected atoms** to assign a shared map. Existing different labels must be cleared before pairing.

**Auto-map** prepares a detached proposal in Rust. It keeps existing maps as anchors, matches exact elements/isotopes, and excludes agents. It maximizes the number of corresponding atoms, then favors conserved heavy-atom adjacency, all adjacency, unchanged bond classes, fewer changed atom environments, and smaller charge/H/radical differences. Bond order, charge, attached hydrogens, degree and ring membership can change in a reaction and never exclude candidate pairs. Coordinates and stereochemistry are preserved; they are not used to infer a reaction mechanism.

Review the preview and click a **Map** row to select its pair in the drawing. **Apply reviewed proposal** changes the document in one Undo step. The review distinguishes one best map under this graph score, several equal-best maps, and a search-budget result whose optimality/uniqueness is unknown. A budget result still contains a complete feasible atom correspondence, and stays detached until accepted. Unmatched atoms retain their existing maps or remain unmapped; omitted coproducts are not invented. A changed drawing invalidates the proposal.

The default search limits are 128 explicit atoms per side, 768 bonds, 16384 candidate pairs, 250000 search states, and 20000000 charged matching work units. Work is charged before candidate comparisons, maximum-matching scans, scoring, bounds and search choices. If that budget ends before a full feasible incumbent exists, Auto-map reports an error and creates no proposal. Chemical validation/preparation is bounded by the input caps; this budget is not a wall-clock deadline. Unit coefficients are required for Auto-map: draw each participant explicitly before mapping repeated molecules. A mapping edit also checks other reaction steps containing a shared intermediate, including their coefficients, so it cannot make an earlier exportable repeated participant unmappable. Same-side duplicate map classes can be valid imported SMILES data; the editor preserves them, displays them, and asks for unique anchors before Auto-map. The existing reaction exporter continues to reject duplicate maps on one side.

**Move map labels** opens the shared indicator handles. Drag a map label to set its own offset; **Reset map label positions** restores automatic collision-aware placement. Indicator size/font controls in Atom Labels also apply to map labels. These choices survive native save, Undo, native copy and figure export. RXN/reaction SMILES retain chemical maps; save `.rsk` for custom numbers and map label styling.

**Align mapped structures** is a separate command. It rotates/translates each whole product participant toward its mapped reactant coordinates using a common offset for the reaction's drawing lanes. It preserves bond lengths, depth, wedge appearance, chemical stereo and connectivity, without reflections or scaling. Molecules without a mapped counterpart stay in place. Alignment does not create maps and is one Undo step.

Concrete reaction SMILES and reaction SMARTS serve different purposes: query expressions such as `[N!0H:3]` are not concrete molecular atoms. The [Daylight SMILES specification](https://www.daylight.com/dayhtml/doc/theory/theory.smiles.html) explains atom classes, and the [RDKit Book](https://www.rdkit.org/docs/RDKit_Book.html#reaction-smarts) explains reaction SMARTS. The [mapped reaction example](https://future-chem.com/rdkit-chemical-rxn/#toc4) motivates correspondence and coordinated drawing. Gaussian/TS export remains future work; this editor supplies persistent chemical atom maps and visible labels.

## Current limits and verification

Persistent reactions use explicit role assignment and chemical exchange. The bounded single-arrow inference above applies only to copying; broader reaction interpretation, whole-scheme cleanup, balancing, yields, quantities, and coefficient editing in the inspector remain future work. The existing cleanup command still operates only on your selected atoms or molecules and leaves the reaction arrow and unselected molecules fixed.

Regression checks cover role reassignment, native save/copy/delete, ID remapping, growth, rejected cross-role joins, undo/redo, arrow reversal, assistant roles, coefficients, selected-step export, atom maps, tetrahedral and E/Z stereochemistry, cleanup, and abbreviation replacement. Offscreen native UI checks cover 1280 × 820 and 1040 × 680 layouts and export-button mouse events without opening a desktop window. The [0.6 desktop walkthrough](release-0.6-validation.md) also inspected the native role cards for an esterification with two reactants and two products. Windows/Office images remain from the earlier verification.

Reaction parsing, layout and export run in Rust. Independent tests compare the results with the original [RDKit reaction APIs](https://www.rdkit.org/docs/source/rdkit.Chem.rdChemReactions.html).
