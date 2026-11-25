---
section: Changes
issues: []
---

- Opening a multi-frame XYZ file (a trajectory, an xtb or CREST conformer file) or an SD file of 3D conformers of one molecule asks which frame to open: First, Last, Lowest energy, or any frame picked in the list with its energy and ΔE. "Keep all frames as an ensemble" also keeps up to 32 of the lowest frames with the molecule, shown in the results palette's Conformers tab. XYZ comments that are only a number with at least four decimals (CREST) are read as the frame's energy in hartree, and ReShiki's own conformer exports (XYZ and SDF) reopen with their energies, charge and spin.
