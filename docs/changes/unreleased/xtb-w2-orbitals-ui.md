---
section: Changes
issues: []
---

- The xTB results palette has an Orbitals tab: a level diagram (occupied levels with electron arrows, HOMO, LUMO and the gap marked, degenerate levels side by side, α and β columns for open shells) and the list around the gap. Choosing an orbital, with the list, the diagram, HOMO / LUMO or ↑ / ↓, shows its surface on the molecule's 3D model; Isovalue (with presets 0.02, 0.05, 0.10 a.u.), Look (Solid, Shaded, Outline, Mesh), Mesh lines and Opacity change it at once. Place on canvas keeps the orbital in the drawing as one Undo step; it saves, prints and exports with the model. xTB orbitals are recomputed when shown, so files stay light.
- Opening a Molden, Gaussian fchk, Gaussian log or ORCA output keeps the calculation's result with its molecule and opens it in the results palette; Molden and fchk orbitals appear in the Orbitals tab. A job cut off after its last result opens at that result's geometry.
