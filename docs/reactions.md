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

## Import and export

Open `.rxn` or `.rsmi` files, or paste RXN text or reaction SMILES into Import. Reaction SMILES uses `reactants>agents>products`, for example:

```text
CC(=O)O.CCO>OS(=O)(=O)O>CCOC(C)=O.O
```

The importer lays out editable molecules, separators and an arrow, with chemical agents above the arrow. Existing molecular coordinates within each RXN participant are retained; page and arrow layout are recreated.

Choose one arrow in the reaction inspector to export it. Export controls remain visible while the participant list scrolls. Both sides must contain a participant.

| Format                    | Retained data                                                                                             | Limits                                                                                                                     |
| ------------------------- | --------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `.reshiki`                | Complete drawing, reaction roles, coefficients, captions, styles and groups                               | Use this for continued editing.                                                                                            |
| RXN V3000                 | Reactants, products, separate agents, atom maps, supported molecular stereochemistry and atom coordinates | Arrow appearance, captions and page layout are not stored.                                                                 |
| Reaction SMILES (`.rsmi`) | Reactants, products, agents, atom maps and supported molecular stereochemistry                            | Coordinates and drawing appearance are not stored. Disconnected components can become separate participants when reopened. |

Coefficients greater than one export as repeated chemical participants. Repeated mapped participants need distinct atom maps; the exporter rejects duplicate map numbers on one side. Query atoms, enhanced stereo groups, hydrogen interactions, partial and quadruple bonds are not supported by this reaction-exchange workflow and produce an error rather than a flattened structure. The expanded export is limited to 10,000 atoms.

Molecular MOL/SMILES/InChI and current CDXML/CDX drawing exchange do not retain these reaction roles. The native clipboard representation preserves them when pasting a complete reaction into ReShiki. Save `.reshiki` alongside an exchange file to retain the full scheme.

## Current limits and verification

This is explicit role assignment and chemical exchange. Automatic reaction interpretation, whole-scheme cleanup, an atom-mapping editor, balancing, yields, quantities, and coefficient editing in the inspector remain future work. The existing cleanup command still operates only on your selected atoms or molecules and leaves the reaction arrow and unselected molecules fixed.

Regression checks cover role reassignment, native save/copy/delete, ID remapping, growth, rejected cross-role joins, undo/redo, arrow reversal, assistant roles, coefficients, selected-step export, atom maps, tetrahedral and E/Z stereochemistry, cleanup, and abbreviation replacement. Offscreen native UI checks cover 1280 × 820 and 1040 × 680 layouts and export-button mouse events without opening a desktop window. Full desktop acceptance testing of this inspector remains pending.

The exchange implementation uses the [RDKit reaction APIs](https://www.rdkit.org/docs/source/rdkit.Chem.rdChemReactions.html).
