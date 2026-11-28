# Diruthenium tetrahydride calculation fixture

The target is neutral Cp*Ru(μ-H)₄RuCp*, C₂₀H₃₄Ru₂, where Cp* = η⁵-C₅Me₅.
Each XYZ file contains two Ru atoms, 20 carbon atoms, and 34 hydrogen atoms.
The gas-phase calculations use charge 0 and `--uhf 0` (singlet). An asymmetric
starting geometry tests whether the symmetry of the original model affects the result.

The starting coordinates were constructed from published distances. The
comparison with the crystal therefore uses individual distances and ligand
contacts. A crystallographic RMSD would require the experimental coordinates.

## Experimental reference and provenance

- Suzuki, Omori, Lee, Yoshida, and Moro-oka reported the four-hydride bridge.
  [Original structure report (1988)](https://doi.org/10.1021/om00100a032).
- Table 1 for complex 2 gives a Ru–Ru separation of 2.4630(5) Å, Ru–C distances
  of 2.171(2)–2.192(2) Å, ring C–C distances of 1.429(3)–1.440(3) Å, and
  Ru–H distances of 1.62(3)–1.73(3) Å. X-ray measurements of H have appreciably
  larger uncertainties than the metal separation.
  [Suzuki et al. (1994), p. 1132](https://doi.org/10.1021/om00016a017).

## Seeds and atom order

`diruthenium_tetrahydride_seed.xyz` is an idealized seed informed by the
experimental distances. Its construction places Ru at z = ±1.2315 Å.
The two regular Cp* rings have C–C = 1.435 Å, Ru–C = 2.1826 Å,
radially directed ring–methyl C–C = 1.500 Å, and tetrahedral C–H = 1.090 Å.
The ring radius is `1.435 / (2 sin(π/5))` and its height relative to Ru is
`sqrt(2.1826² − radius²)`. Ring phases are 0 and π/5. Four hydrides lie
at z = 0 on a square of radius `sqrt(1.67² − 1.2315²)` with phase π/4.
The constructed seed assumes square hydride positions, regular carbon rings,
staggered ligand alignment, and idealized methyl orientations.

`diruthenium_tetrahydride_asymmetric_seed.xyz` adds a uniform displacement
from −0.08 to +0.08 Å independently to every coordinate using Python's
`random.Random(1988)`, in file order. It removes exact symmetry before
optimization. Both optimized fixtures come from this asymmetric seed.

The files use the same one-based atom numbering.

| Atoms | Meaning |
|---|---|
| 1–2 | Ru centers |
| 3–7 / 28–32 | First / second Cp* ring C, cyclic order |
| 8–12 / 33–37 | Corresponding methyl C |
| 13–27 / 38–52 | Three H per corresponding methyl group |
| 53–56 | Four initial bridging H |

## Real ReShiki calculation results

Calculated on 2026-11-28 with ReShiki 0.17.1, the pure Rust axtb engine
0.2.0 at revision `9d0388c9e9d81c43c0e7bba01c0ccf706a88fc91`, default
SCF settings and `axtb` conventions, gas phase. The default electronic
temperatures reported by the worker are 300 K for GFN2 and 0 K for g-xTB.
The two methods use different absolute energy scales.

| Tight singlet optimization | GFN2-xTB | g-xTB |
|---|---:|---:|
| Energy / Eh | −66.7269150921 | −972.2711497753 |
| Optimization steps | 138 | 54 |
| Ru–Ru / Å | about 2.3963 | 2.37534 |
| Error vs experimental Ru–Ru / Å | −0.0667 | −0.0877 |
| Ru–ring C / Å | about 2.1923–2.2635 | 2.1622–2.1853 |
| Ring C–C / Å | about 1.4285–1.4386 | 1.4284–1.4327 |
| Hydride coordination | 2 terminal + 2 bridging | 4 bridging |

**GFN2 changes the hydride connectivity.** H53 and H55 become terminal.
Their nearest Ru is about 1.563 Å away and their other Ru about 2.825 Å.
H54 and H56 remain bridging at about 1.705 Å to both metals. The optimized GFN2 isomer is stored in `diruthenium_tetrahydride_gfn2.xyz`. Its projected Hessian has 162 vibrational modes, with no imaginary frequencies. The lowest are 7.93, 35.70 and 44.31 cm⁻¹.

The g-xTB result, `diruthenium_tetrahydride_gxtb.xyz`, preserves all four
bridges (eight Ru–H contacts 1.7411–1.7434 Å). The computed Ru–Ru distance is 0.088 Å shorter than the experimental distance, and the computed Ru–H distances span a narrower range. Its analytic Hessian has
162 vibrational modes, zero negative frequencies in the measured run,
and lowest modes 0.87, 2.72, and 44.71 cm⁻¹. The idealized seed at normal
convergence had a −2.25 cm⁻¹ torsion. Tight relaxation of the asymmetric
seed removed that soft mode.

Both the symmetric and asymmetric starts converge to the same respective
singlet structures. Normal-convergence energies differ by less than
2 × 10⁻⁷ Eh for g-xTB and 3 × 10⁻⁸ Eh for GFN2. Independent triplet
optimizations (`--uhf 2`) from the asymmetric seed give −972.2186165749 Eh
(g-xTB) and −66.6991869926 Eh (GFN2), above their singlets by 0.0525332
and 0.0277281 Eh respectively. This supports the chosen singlet among the
two tested multiplicities.

## Reproduce

From the repository root, use an executable built with the `xtb` feature:

```sh
reshiki --cli calculate --task opt+freq --method gfn2 --convergence tight \
  --charge 0 --uhf 0 --threads 1 \
  tests/fixtures/xtb/diruthenium_tetrahydride_asymmetric_seed.xyz
reshiki --cli calculate --task opt+freq --method gxtb --convergence tight \
  --charge 0 --uhf 0 --threads 1 \
  tests/fixtures/xtb/diruthenium_tetrahydride_asymmetric_seed.xyz
cargo test --release --features xtb --test it \
  xtb::metal_clusters::diruthenium_cluster_calculates_and_checks_both_local_minima
```

The full native calculation regression runs in optimized builds. Debug builds
ignore it because the unoptimized engine makes metal optimization and the
Hessian too slow for the CLI watchdog. A separate fast fixture test runs in
debug builds and checks the atomic numbers, electron count, declared electronic
state, finite coordinates, and stored two- versus four-bridge geometries.

The optimized integration test runs these real calculations, checks atomic identity,
neutral singlet metadata, finite energies, physical metal/ring distances,
two GFN2 and four g-xTB hydride bridges, and both Hessian mode counts. Its −10 cm⁻¹
cutoff avoids requiring the numerical sign of sub-3 cm⁻¹ torsions to be
identical on every computer. This regression checks the calculation and
geometry pipeline. The crystal distances give an independent comparison.

The existing `xtb_wheel` CI job also runs
`crates/python/tests/test_xtb.py` against its installed release wheel pair. Its
diruthenium test uses the stored g-xTB geometry as a cheap four-bridge start,
performs real GFN2 and g-xTB optimizations through `reshiki.xtb.optimize`, and
checks the resulting two- versus four-bridge coordination. It does not repeat
the full native Hessian or numerical reference assertions. To run just that
installed-wheel check locally:

```sh
python -m pytest crates/python/tests/test_xtb.py -k diruthenium
```

## Editable 2D drawings

The [g-xTB drawing](../../../website/public/examples/diruthenium-gxtb.rsk)
and [GFN2-xTB drawing](../../../website/public/examples/diruthenium-gfn2.rsk)
come from the two optimized XYZ files. The editor rotated each model by −55°
about Y and ran **Show as 2D from the 3D view**. Each native file retains the
56 original atom coordinates. The [previous drawing](../../../docs/images/metal-clusters/diruthenium-baseline.png)
uses the same g-xTB file, view and export settings.
