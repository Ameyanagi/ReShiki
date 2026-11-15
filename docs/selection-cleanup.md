# Selection cleanup

**Clean up** (toolbar, **Shift+Cmd/Ctrl+K**) redraws the selected structures with regular geometry, or every structure in the drawing when nothing is selected. It applies at once as one Undo step: **Cmd/Ctrl+Z** shows the original again and Redo the cleaned drawing. Text, arrows, graphics, groups, colors and label styles are retained.

The drawing keeps its size: each molecule is redrawn at the median of its drawn bond lengths, as ChemDraw does, so one stretched bond does not change it. Six-membered rings stand upright, with vertical sides and a vertex at the top, turning at most 30°; a flat ring turns whichever way leaves more ring-fusion bonds vertical. Each molecule of a salt or a multi-component selection turns on its own. Cages, and molecules whose six-membered rings point different ways (fluorene), keep their orientation.

The ▾ beside the toolbar button sets what Clean up does next:

- **Orientation · Keep** fits the new coordinates to the drawn orientation without reflecting the structure, then stands six-membered rings upright. **Free** uses a conventional orientation, as for a new structure. **IUPAC fused rings** lays the largest fused ring system out in a horizontal row.
- **With a selection · Selected atoms** redraws only the selection. Unselected atoms remain fixed, including atoms bonded to the selection. **Whole molecules** redraws connected molecules touched by the selection. Other molecules remain unchanged, and each cleaned molecule retains its own center.

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

Automated coverage includes separate molecule centers, fixed unselected atoms, partial side-chain cleanup, selected-molecule expansion, abbreviations, unselected invalid chemistry, orientation control, tetrahedral and alkene identity, and cancellation of late cleanup results. Reaction layout and general geometric optimization remain separate work.
