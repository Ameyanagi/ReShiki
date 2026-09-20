# Contextual drawing shortcuts

With the pointer over an atom, press **C, N, O, S, P, F or H** to change its element. A singly selected atom also works. Carbon becomes an explicit CH₄, CH₃, CH₂, CH or C label according to its bonds; nitrogen and oxygen use their corresponding valence-derived hydrogens. Chemical edits refresh the derived labels automatically. Collapsed abbreviations must be expanded before editing their internal atoms.

Hover a bond, or select its two endpoints, then press **S** for single, **D** for double or **T** for triple. Repeating D on an ordinary double bond cycles the second line through left, right and centered positions. Clicking with the matching double-bond tool has the same effect. Appearance-only shifts preserve chemical order and stereo. Each edit is one Undo step.

Without an atom/bond target, element keys choose their drawing tool, D chooses the double-bond tool, and T chooses text. Focused text inputs retain normal typing.

## Aromatic circles

Select all atoms of an aromatic ring and press **A**, or use **Toggle aromatic circle** in Properties. Press A again to restore alternating bonds. Without a ring-sized selection, A selects the arrow tool.

This changes an existing aromatic ring’s representation and preserves its molecular identity; saturated rings are not converted into different molecules. Circles follow the atom coordinates, color, movement, copying and deletion of their rings. Selected rings within a larger structure can be toggled independently. Native drawing files retain aromatic bond orders; editable drawing exchange carries those bonds plus the corresponding owned circle graphic. SVG, PDF and PNG use the same scene renderer as the canvas.

Automated checks cover benzene, oxygen/nitrogen five-membered rings, a fused aromatic system, partial fused-ring selection, H counts, Undo, copying, cleanup, and editable binary/XML round trips. Circle geometry uses the smallest closed aromatic cycles; unusual nonplanar or strongly distorted ring systems require further layout work.
