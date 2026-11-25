---
section: Changes
issues: []
---

- Predict → xTB has a Conformers task. ETKDG makes up to 300 starting structures, minimized with MMFF94s (or MMFF94, UFF, GFN-FF or GFN2-xTB); Butina clustering on symmetry-aware heavy-atom RMSD (fine / normal / coarse) picks representatives within an energy window, and GFN2-xTB (or the chosen xTB method) optimizes and ranks up to 20 of them (lowest 1 or 3 per cluster, up to 60). The card shows live progress with Cancel, which keeps the conformers already re-ranked. The lowest conformer is applied to the 3D model as one Undo step after the stereo check.
- The results palette's Conformers tab lists the kept conformers with ΔE, the start's ΔE, Boltzmann population at 298 K, RMSD to the lowest, cluster size and a stereo check. Clicking a row shows that conformer on the 3D model, "All conformers" overlays the others faintly behind it, "Use this conformer" applies it as one Undo step, and Export XYZ… / Export SDF… write the ensemble with energies. At most 32 conformers are saved, as compact frames.
