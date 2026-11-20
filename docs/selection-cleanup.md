# Selection cleanup

**Clean up** (toolbar, **Shift+Cmd/Ctrl+K**) redraws the selected structures with regular geometry, or every structure in the drawing when nothing is selected. It applies at once as one Undo step: **Cmd/Ctrl+Z** shows the original again and Redo the cleaned drawing. Text, arrows, graphics, groups, colors and label styles are retained.

By default, Clean up redraws each molecule at the median of its drawn bond lengths, as ChemDraw does, so one stretched bond does not change its size. Six-membered rings stand upright, with vertical sides and a vertex at the top, turning at most 30°; a flat ring turns whichever way leaves more ring-fusion bonds vertical. Each molecule of a salt or a multi-component selection turns on its own. Cages, and molecules whose six-membered rings point different ways (fluorene), keep their orientation. A ring of nine or more atoms that shares no atom with another ring, such as cyclododecane or 18-crown-6, is drawn as a regular polygon with its substituents pointing outward, as ChemDraw draws it. A trans double bond in the ring keeps its inward corner, and crowded neighbouring substituents can turn apart. Fused, spiro and bridged large rings keep their folded layout.

The ▾ beside the toolbar button sets what Clean up does next:

- **Orientation · Keep** fits the new coordinates to the drawn orientation without reflecting the structure, then stands six-membered rings upright. **Free** uses a conventional orientation, as for a new structure. **IUPAC fused rings** lays the largest fused ring system out in a horizontal row.
- **With a selection · Selected atoms** redraws only the selection. Unselected atoms remain fixed, including atoms bonded to the selection. **Whole molecules** redraws connected molecules touched by the selection. Other molecules remain unchanged, and each cleaned molecule retains its own center.
- **Bond length · Also set bond length to the style (14.4 pt)** redraws each molecule at the drawing style's bond length instead of keeping its size. The number follows the current drawing style. ReShiki remembers this choice, and **Clean up reaction** uses it too.
- **Clean up reaction** runs the reaction cleanup below.

Components containing multi-center or variable attachments are kept unchanged
with an explicit warning. This includes collapsed or expanded Cp/Cp* groups.
Independent ordinary molecules in the same selection can still be cleaned.
Automatic geometry cleanup of attachment-containing complexes is not yet
supported; retaining them does not claim molecular identifier or coordination
valence validation. Malformed attachment targets and solver errors remain errors.

Cleanup checks molecular identity, including stereochemistry. Fixed-atom constraints can prevent a satisfactory layout; an error leaves the drawing unchanged. If regenerated coordinates would assign previously unspecified alkene stereochemistry, an affected double bond receives a crossed depiction and the status bar explains why. Invalid chemistry outside a selected component does not prevent that component from being cleaned; a warning explains when whole-document properties cannot be computed.

A stereochemistry refusal suggests including the whole molecule only when some
of its atoms were held fixed. If the whole molecule was already included, the
message confirms that the drawing was left unchanged.

## Clean up reaction

**Clean up reaction** (**Shift+Cmd/Ctrl+X**, the Clean up ▾ menu, or the command palette) lines up the selected reaction, or every reaction in the drawing when nothing is selected. It applies at once as one Undo step.

- Reactants, products and `+` signs are centred on the arrow's line and spaced evenly, half a bond length apart.
- Every arrow takes the same length: 3.2 bond lengths, or longer when the text above or below an arrow needs it.
- Text and reagents above or below an arrow are centred on it as a block and keep their side and order. Conditions attached to an arrow stay attached.
- Arrows on one line form one row, such as A → B → C or A → B ← C. A molecule shared by two rows, such as the product of a horizontal step that starts a vertical one, stays where the first row put it.
- Recorded reaction roles decide which molecules sit on the arrow's line and which are reagents. Selecting only the arrow of a recorded reaction cleans the whole reaction; without recorded roles, select the molecules too. Without roles, a molecule or `+` sign within a bond length of the line is on it, unless a gap of more than 4 bond lengths separates it from the arrows. Text or a molecule clear of the line beside the arrow's middle is a reagent.
- With **Also set bond length to the style** on, each molecule with a visible 2D drawing in a cleaned reaction is first scaled to the style's bond length. Molecules shown only in 3D can still move with the layout while keeping the bond lengths of their hidden drawings.

Only straight arrows outside explicit groups set the row's axis and take the common arrow length. Captions just above or below a molecule, such as compound numbers and names, move with it. Curved arrows, arrows with anchored endpoints, and other text away from an arrow stay where they are.

## Verification

Automated coverage includes separate molecule centers, fixed unselected atoms, partial side-chain cleanup, selected-molecule expansion, abbreviations, unselected invalid chemistry, orientation control, tetrahedral and alkene identity, and cancellation of late cleanup results. General geometric optimization remains separate work.
