# COSMolKit 0.3.0 -> 0.5.0: ReShiki before/after comparison

- Before: `before.json` (main a454aa8c, COSMolKit 0.3.0 fork rev 252a1f3), confirmed by `before-logs/before-rerun.json`.
- After: `after.json` (`chore/cosmolkit-0.5` a176a9c1, COSMolKit 0.5.0 rev 501f7b2), confirmed by `after-rerun.json`.
- Harness: `harness/bump_compare.rs` copied **unchanged** into `ck-impl/crates/io/examples/`. It compiled against the migrated tree without edits, so the before side did not need a rerun. The copy was deleted afterwards. `git status` in ck-impl is clean and no `./target` was created (`CARGO_TARGET_DIR=ct-impl`).
- Raw diff: `diff-raw.txt`, made with the HOWTO snippet (energy relative tolerance 1e-6, `elapsed_ms` ignored). It has 209 lines: 2 version fields and 207 geometry lines. **No InChI line differs.**
- Determinism: `after.json` and `after-rerun.json` are identical except for the `user-c36-8conf` ETDG-fallback prior counters. Those are timing-sensitive because they hit counter 11, the 10 s timeout, which matches what the baseline report says.
- RDKit oracle scripts are in `rdkit/` (`embed_check.py`, `fixture_check.py`, `extra_check.py`, `taxol8.py`). They ran with `uv run --no-project --with rdkit==<v> python -I`, with v = 2026.3.6 (the version COSMolKit 0.5.0 aligns to), 2026.3.1 (the 0.3.0-era version) and 2025.9.6/2026.9.1 as controls. They use ReShiki's embed parameters: ETKDGv3, seed 0x52534b, maxIterations 100, timeout 10, enforceChirality, prune 0.1, sequential seeds, and an effectively infinite non-bonded threshold.

## Summary

| Area | Cases | Differences | Classification |
|---|---|---|---|
| InChI generation | 86 | none (status, InChI, key, AuxInfo, message, log, diagnostics and read-back are all identical) | n/a |
| InChI reading/key | 17 | none | n/a |
| Version fields | 2 | `cosmolkit_*_crate` 0.3.0 -> 0.5.0 | (a) |
| Force-field evaluation on identical coordinates | C60 seeded (3 fields), benzene, naphthalene, ethanol, cyclohexane, alanine, butene, DCE, aspirin, acetate, NMe4+ | none | n/a |
| Geometry: small molecules | 15 SMILES x 3 fields | N-methylacetamide and caffeine (minimizer end point noise); pyridine and caffeine (embedding now matches RDKit 2026.03.6); SF6 MMFF error text | (a) / (c) |
| Geometry: Taxol | 1 and 8 conformers x 3 fields | different embedding, so different selected conformer and energies | (a), plus one (d) counter-attribution mismatch |
| Geometry: C36 | 1 and 8 conformers x 3 fields | different MMFF embedding (graph change); final energies identical | (a) / (c) |
| Geometry: C60 | unseeded x 3 fields | MMFF embeds 1 conformer instead of 2, the same as UFF and RDKit; final energy identical | (a) / (c) |
| Failure-counter format | all | 15 slots now; **no** case reports index 12, 13 or 14 | (a) |
| Wall time (dev profile, sequential, not part of the diff) | | total 87.9/85.8 s -> 71.8/72.3 s; Taxol 1-conformer generate 9.1 s -> 1.8 s; C36 1-conformer 9.3 s -> 8.4 s | (c) |

**Regressions (b): none found.**
**Unexplained (d): one minor item, D1 below.**

## Differences in detail

### V1. Crate version fields: (a)
`/cosmolkit_geometry_crate` and `/cosmolkit_inchi_crate` change from 0.3.0 to 0.5.0. These are the intended bump (`CRATE_VERSION`/`COSMOLKIT_VERSION`).

### I. InChI: no differences
All 86 generation cases and 17 read cases are byte-identical. That covers stereo-any, either-double, wavy, `ctu`, the 11 perchlorate variants, Tl–Ra, the 1024-carbon chain, the 1023/1024 He atoms and invalid SMILES. Upstream risk inchi/risks/2 said the BondStereo::Any neighbour swap *could* change stereo-any output, and inchi/risks/4 said error text could change. Neither shows up in this corpus. The long and failing inputs still give status 2 with an empty InChI. Cl valence 5 still fails to prepare with the same text.

### G1. N-methylacetamide (`CC(=O)NC`) and caffeine: minimizer end-point noise: (a)
- Generate initial and final energies are identical (NMA MMFF94 -17.3145 -> -24.649061; MMFF94s -18.1744 -> -25.254659). These equal RDKit's minimum (-24.649061 / -25.254659).
- The returned coordinates differ slightly:
  - NMA: `evaluate_generated.gradient_rms` MMFF94 2.08e-4 -> 7.08e-4, MMFF94s 8.4e-5 -> 9.1e-4.
  - Caffeine: MMFF94 1.5e-4 -> 2.6e-3, MMFF94s 2.1e-4 -> 1.4e-3, UFF 1.1e-3 -> 9.4e-4.
- As a result, `relax_perturbed.initial_energy` moves by at most 0.0016 (NMA MMFF94s 12.7863 -> 12.7879) and heavy RMSD vs generated stays below 1e-4 Å.
- `common UFF` on NMA moves -6.0e-5 (relative 5e-6). Caffeine `common UFF` moves +3.9e-4, because the shared MMFF94 geometry moved slightly.
- Cause: identical starts and identical field energies, but a slightly different stopping point. That points to the minimizer moving to `cosmolkit-forcefields` `PreparedForceField::minimize` (analysis geometry/patches/0 evidence; risks/5). All runs still converge (`converged=true`) to the same energy within 1e-6.

### G2. Pyridine and caffeine: embedding now reproduces RDKit 2026.03.6: (a), and (c) for caffeine MMFF
- **Pyridine:** generate `initial_energy` changes from 20.9392 to 19.3567 (MMFF94/94s) and from 15.0038 to 12.5179 (UFF). Final energies are unchanged (15.523431 / 10.429881).
- **Caffeine:** generate `initial_energy` changes from -102.0662 to -105.3033 (MMFF94), from -102.7641 to -105.9748 (MMFF94s) and from 88.2579 to 85.4155 (UFF). Final energies are unchanged.
- **Before vs RDKit:**
  - Before UFF and pyridine equal RDKit 2026.03.1 exactly (15.0038, 20.9392; caffeine UFF 88.2580).
  - Before caffeine MMFF (-102.0662) matched **no** RDKit version (2026.03.1 gives -105.7327). The 0.3.0 path embedded on MMFF's kekulized graph.
- **After vs RDKit:** after equals RDKit 2026.03.6 exactly for all six values (19.3567, 12.5179, -105.3033, -105.9748, 85.4155).
- **Causes:**
  - Upstream "align chemistry with RDKit 2026.03.6" (commit 28002d4, inchi/risks/3) changed ETKDG coordinates for aromatic N heterocycles.
  - The MMFF calculation-graph change (geometry/risks/0) now makes MMFF embedding equal RDKit's embed-then-MMFF workflow.
- Relax-from-perturbed initial energies (±0.85) and input RMSD (<7e-4 Å) follow from the different generated coordinates.

### G3. SF6 MMFF error text: (c) (cosmetic message change)
- Before: `MMFF parameters are unavailable for this molecule: unsupported feature forcefield.mmff.mol_properties: RDKit MMFF heavy atom typing is not ported for this atom`
- After: `MMFF parameters are unavailable for this molecule`
- 0.5.0 `mmff_properties_with_params` now types the atom and returns `Ok(MmffProperties{valid:false})` instead of a "not ported" error. ReShiki's `check_field_parameters` (solver.rs:198) then reports the generic message.
- RDKit 2026.03.6 agrees: `MMFFHasAllMoleculeParams(SF6) == False`.
- The request still fails as before, now for the RDKit-equivalent reason. The detailed text is lost, but the old text was a porting-gap message rather than chemistry. UFF SF6 and SnMe4 are unchanged.

### G4. Taxol, 1 conformer: different embedding, so a different selected conformer: (a)
- **Failure counters:** `0:37 6:5` -> `0:6` for all three fields. RDKit gives exactly `0:37 6:5` under 2026.03.1 and exactly `0:6` under 2026.03.6. This is the upstream RDKit 2026.03.6 ETKDG alignment.
- **MMFF94/94s generate:**
  - Initial energy 498.945 -> 481.313; final 251.715 -> 252.773 (+1.06).
  - Still `converged=false` at the 500-iteration cap.
  - `evaluate_generated.gradient_rms` 0.090 -> 2.18.
  - `relax_perturbed` goes further downhill to 250.939, heavy RMSD vs generated 0.019 -> 0.300 Å.
  - RDKit 2026.03.6 minimizing its own conformer 0 for 500 iterations also does **not** converge and ends at **250.9394**, the same basin ReShiki's relax reaches.
  - So the after conformer is a harder start that 500 iterations do not finish. That is a seed-dependent outcome of the new embedding, not a code regression. Before, the 0.3.0 run also did not converge.
- **UFF generate:** `converged` false -> true, final 273.724 -> 274.647, gradient 0.77 -> 0.0027. Relax now stays put (RMSD 0.286 -> 0.0004 Å).
- **`common UFF`:** 371.6 -> 468.8. This is UFF evaluated on the MMFF94-generated geometry, which is now a less-relaxed conformer. It is a geometry change, not a UFF change.
- **Pre-existing note, not migration-related:** ReShiki's Taxol initial energies do not equal RDKit's for either version (before 498.9 vs RDKit 2026.03.1 471.5; after 481.3 vs RDKit 2026.03.6 480.8), while the counters match exactly. Small molecules match exactly, so the residual difference sits in ReShiki's Taxol request (eight explicit stereo H) or in conformer post-processing, on both sides.

### G5. Taxol, 8 conformers: (a), with D1
- **Counters:** `0:174 2:3 6:8 10:1` -> `0:160 2:2 6:6 10:1`. Both runs were stable and not timing-sensitive. Before equals RDKit 2026.03.1 exactly.
- **MMFF94/94s:** final energy 244.636 -> 243.061 (-1.58); `converged` false -> true; gradient 0.0145 -> 0.0028.
- **UFF:** final energy 242.551 -> 246.542 (+3.99), converged both times.
- **`common UFF`:** 351.35 -> 363.18, from the geometry change.
- Lowest-of-8 selection energies are within the spread RDKit 2026.03.6 itself produces for these seeds (MMFF lowest 243.49, UFF lowest 245.70), so no direction indicates a regression.

### D1. Taxol 8-conformer counter attribution differs from RDKit 2026.03.6: (d), minor and diagnostic only
- After: `0:160 2:2 6:6 10:1`. RDKit 2026.03.6: `0:160 2:2 6:5 8:1 10:1`.
- The totals per stage match. One failed attempt is booked as FinalChiralBounds (6) by COSMolKit 0.5.0 and as LinearDoubleBond (8) by RDKit. The enum indices are identical (`cosmolkit-conformer/src/params.rs:9-25`).
- This looks like a final-check ordering difference inside upstream COSMolKit 0.5.0, not a ReShiki change.
- It doesn't change which conformers succeed (8/8 both, same total attempts). Only the `embedding_failure_counters` diagnostic is affected, and ReShiki's retry logic reads only indices 5 and 11.
- The analysis does not name it. It is worth an upstream issue if exact RDKit counter parity matters.

### G6. C36, 1 and 8 conformers: (a), and (c) for MMFF
- **ETDG fallback** still triggers for every field and conformer count.
- **Prior ETKDG counters:**
  - MMFF: `0:13 5:87` -> `0:16 5:84`.
  - UFF: `0:17 5:83` -> `0:16 5:84`.
  - RDKit: 2026.03.1 `0:17 5:83`; 2026.03.6 `0:16 5:84`.
  - After, all three fields equal RDKit 2026.03.6. Before, UFF equalled 2026.03.1 and MMFF matched no RDKit version, because of the kekulized MMFF graph (geometry/risks/0).
- **Generate initial energy:**
  - MMFF 573.314 -> 594.549. RDKit 2026.03.6 ETDG gives 593.16.
  - UFF 654.993 -> 602.838 (1 conformer) or 689.423 (8 conformers). RDKit 2026.03.6 gives 601.94.
- **Final energies** are identical to 1e-6 (MMFF 219.2840, UFF 284.9087), and equal RDKit's (219.284 / 284.9087).
- **8-conformer prior counters** (`0:41 5:297 11:1` -> `0:85 5:315 11:1`) hit the 10 s timeout. They are timing-sensitive and also vary between the two after runs (`0:79 5:301` vs `0:85 5:312`).
- Gradient and relax deltas (≤ 1e-3 gradient RMS, ≤ 5e-4 Å) follow from the different start.

### G7. C60 unseeded: (a) / (c)
- Cage-ETDG still triggers (`trial_failure_counters=5:1`).
- MMFF94/94s now embed **1** of 8 requested conformers instead of 2 (after RMS pruning). UFF embedded 1 both before and after, and RDKit (both versions) also returns `[0]` for the same cage-ETDG call. MMFF now uses the same sanitized graph as UFF (geometry/risks/0).
- MMFF initial energy 990.255 -> 1070.986. The final energy is identical (945.16987 -> 945.16988).
- UFF: initial energy 1002.184 -> 1001.876, final energy identical (987.79823), from small upstream ETDG coordinate changes.
- Gradient RMS deltas: MMFF +0.004, UFF -0.017. `common UFF` -0.0032 (relative 3e-6), from the geometry change.
- **C60 seeded with reference XYZ:** no differences at all, for all three fields and every phase. This is the cleanest evidence that MMFF94, MMFF94s and UFF energy, gradient and minimization on fixed input are unchanged.

### F1. Failure-counter vector 12 -> 15: (a)
Expected per geometry/risks/2. No case in either after run reports index 12, 13 or 14.

## Baseline open questions resolved
- **"MMFF94 = MMFF94s for Taxol":** this is real. RDKit 2026.03.1 and 2026.03.6 also give identical MMFF94/MMFF94s energies for Taxol (for example 480.8383 for both).
- **Wall time:** the probe concern (C36 1-conformer 3 s -> 5 s) does not reproduce in sequential dev-profile runs. The after run is faster overall (about 72 s vs 86–88 s); see the summary table.

## Not covered here
- The RDKit-reference InChI suites (`--features rdkit-reference`, `RESHIKI_REQUIRE_INCHI_HELPER=1`) are still not run. The harness InChI corpus shows no output change, which lowers that risk but does not replace them.
- The harness does not record coordinates, so per-atom coordinate parity with RDKit was judged through energies and failure counters only.

## Follow-up on D1 (Taxol 8-conformer counters)

COSMolKit 0.5.0 (`crates/cosmolkit-conformer/src/generation.rs`, embed-points
final checks) runs the checks in RDKit's order: ETK minimisation, then
`doubleBondGeometryChecks` (LINEAR_DOUBLE_BOND, tol 1e-3), then
`finalChiralChecks` (FINAL_CHIRAL_BOUNDS). The order is the same, so the one
attempt counted as cause 6 instead of 8 comes from slightly different
coordinates for that trial, not from different check ordering. Upstream marks
this block `RDKit❗❌` (not a line-for-line port). No ReShiki change: the totals
and the 8/8 success match, and ReShiki's retry logic reads only causes 5 and 11.

Follow-up commits 723d636e (opt-level 3 for cosmolkit-cx, -ringdecomposer,
-search, -smiles in dev/test) and 8106714a (README wording, error message
text on an unreachable path) do not change runtime output, so the harness was
not re-run. Test-profile `reshiki-geometry` timings: embedding tests 66.3 s ->
64.2 s (49.5 s on a second run), c60 1.99 s -> 1.52 s (1.05 s on a second run).
