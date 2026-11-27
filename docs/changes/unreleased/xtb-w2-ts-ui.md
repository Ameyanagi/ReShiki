---
section: Changes
issues: []
---

- Reaction energies and barriers from the drawn scheme: select a reaction arrow
  and Predict ▸ xTB shows one Reaction (xTB) card with the reaction energy (ΔE, or
  ΔG at 298.15 K) and Find transition state. Atoms are matched from the drawing's
  map or automatically; a review sheet opens only when the match is uncertain. A
  barrier is shown only for a converged saddle with one imaginary frequency whose
  IRC connects reactant and product; otherwise the card says why. Path, Animate
  and Export TS input… (Gaussian, ORCA, GAMESS) follow the result. Desktop builds
  with xTB only (macOS and Linux).
- Single-structure transition-state searches (app and `reshiki --cli ts`) first
  fold a stretched starting conformer so the atoms of each forming bond are close,
  and report it.
