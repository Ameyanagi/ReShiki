---
section: Changes
issues: []
---

- xTB results open in a palette docked under the status bar (Dock right moves it into Predict → xTB; Collapse and Close as for NMR). Summary lists the energy, the HOMO–LUMO gap with the near-degenerate warning, the optimization's steps and RMSD from the start, with Copy table, Export… (TSV) and "Show as labels" for the energy on the drawing. Steps plots energy against step; clicking a step previews it on the 3D model, and "Use this geometry" applies it as one Undo step after the stereo check. Log shows the settings and every SCF the worker ran. When the molecule changes after a run, the results are greyed under "The molecule changed after this calculation" with Recalculate, which runs again with the same settings; Undo makes them current again.
- Each result shows "Saved in file" and a "Keep full results in this file (+size)" toggle. Saving a document whose calculation results take more than 5 MB asks once, with "Make lighter" to drop the kept optimization steps and gradients.
