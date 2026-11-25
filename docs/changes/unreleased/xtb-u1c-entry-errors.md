---
section: Changes
issues: []
---

- The xTB Calculate card offers GFN2-xTB, g-xTB 2.0.1 and GFN-FF. GFN-FF has no solvent, orbitals or open shells; g-xTB in a solvent adds the GFN2-xTB solvation energy, and the card says so. Advanced sets the SCF accuracy, electronic temperature and maximum iterations, "xtb 6.8 compatible" conventions, threads and Normal or Tight convergence; every setting is saved with the result, and Recalculate uses them.
- The results palette's Summary adds the partial charges (Show as labels puts them beside the atoms), the dipole moment, bond orders of 0.1 and above and the orbital levels around the HOMO–LUMO gap, which now stay after reopening the file.
- Start a calculation from the context row (Optimize with GFN2-xTB), the right-click menu (Calculate (xTB) ▸ Energy / Optimize), the command palette (Calculate (xTB)…) or with Cmd/Ctrl+R, which runs the last calculation again on the selected molecule.
- Failed calculations show an error card in the results palette: SCF not converged with "Retry at 1000 K", an element the method has no parameters for (with the atom), too many atoms, and a stopped calculation with "Run again" and "Copy details". GFN2-xTB lanthanide compounds whose valence spin cannot match their full electron count are refused with a message that suggests g-xTB or GFN-FF.
