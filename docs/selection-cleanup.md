# Selection cleanup

Clean up requires selected atoms. Double-click an atom to select its connected molecule, or use the rectangle/lasso tools for a smaller region.

- **Selected atoms** redraws only the selection. Unselected atoms remain fixed, including atoms bonded to the selection.
- **Selected molecules** redraws connected molecules touched by the selection. Other molecules remain unchanged, and each cleaned molecule retains its own center.
- **Keep orientation** fits the new coordinates to the original orientation without reflecting the structure. Disable it to use the generated orientation.

The canvas first shows a preview. Use **Show original** to compare, then **Apply** or **Cancel**. Apply creates one Undo step. Cancelling an in-flight refresh prevents its late result from reopening the preview. Text, arrows, graphics, groups, colors and label styles are retained.

Cleanup checks molecular identity, including stereochemistry. Fixed-atom constraints can prevent a satisfactory layout; an error leaves the drawing unchanged. If regenerated coordinates would assign previously unspecified alkene stereochemistry, an affected double bond receives a crossed depiction and the preview explains why. Invalid chemistry outside a selected component does not prevent that component from being cleaned; a warning explains when whole-document properties cannot be computed.

Automated coverage includes separate molecule centers, fixed unselected atoms, partial side-chain cleanup, selected-molecule expansion, abbreviations, unselected invalid chemistry, orientation control, tetrahedral and alkene identity, and cancellation of late worker results. Reaction layout and general geometric optimization remain separate work.
