# Fast geometry optimization for xTB: research and combination plan

Status: research draft for lane L4b (stacked on `xtb/l4-optimizer`, #1415). Results of the
benchmark rounds are added in [Results](#results) as they come in.

Goal (owner, 2026-11-25): "Do more research on optimizers and try to combine the good points
of each optimizer so that the optimization is finished very, very fast." Measured as the
fewest wall-clock seconds to a converged GFN2-xTB minimum, without losing robustness.
Program gate: ≤ 1.5× the gradient count of `xtb --opt`; target ≤ 1.0×.

Everything here is written from the papers and public documentation. No code from xtb,
tblite, axtb, ASE, Sella, pysisyphus or geomeTRIC was read or ported.

## 1. Where the time goes

Wall time = (number of energy+gradient evaluations) × (time per evaluation)

- (Hessian evaluations × their cost) + optimizer overhead.

For GFN2-xTB on 20–70 atoms one gradient costs roughly 10–100 ms single-threaded, while the
ANC optimizer of #1415 spends a median 2 ms per step (MMFF benchmark in #1415). So the
levers, in order of expected size:

1. **Fewer evaluations**: better coordinates and a better starting Hessian.
2. **Cheaper evaluations**: SCF restarts from the previous wavefunction, looser SCF early,
   a cheap pre-optimization (GFN-FF) so the expensive method starts near its minimum.
3. **Optimizer overhead**: only matters for MMFF/GFN-FF and very small molecules.

## 2. Survey

Each entry: what it buys, what it costs, and whether we try it.

### 2.1 Coordinate systems

| Coordinates                          | Idea                                                                                                                                    | Buys                                                                                                                                              | Costs                                                                                                                                                                                                                       |
| ------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Cartesian                            | Steps in x, y, z                                                                                                                        | Simple, no back-transformation, robust for any bonding                                                                                            | Strong coupling between coordinates; with a unit or diagonal Hessian 3–6× more steps than internals (Baker 1993; Tsypin et al. 2026, Table 5: Cartesian BFGS/L-BFGS/FIRE need 250–690 % of Sella's force calls on GFN2-xTB) |
| Approximate normal coordinates (ANC) | Cartesian steps expressed in the eigenvectors of a model Hessian (xtb's ANCopt, Grimme et al. 2017; xtb docs)                           | Most of the decoupling of internals with no back-transformation; robust for metals and odd bonding                                                | Model Hessian quality limits it; the basis goes stale as the geometry moves (xtb resets it every 20 cycles)                                                                                                                 |
| Redundant internals                  | Bonds, angles, dihedrals as coordinates, step via generalized inverse of B (Peng, Ayala, Schlegel & Frisch 1996; Pulay & Fogarasi 1992) | Near-diagonal Hessian, few steps for covalent molecules (Baker 1993: natural internals need fewer cycles than Cartesians and reach lower minima)  | Iterative back-transformation; trouble with linear bends, fragments, metal coordination                                                                                                                                     |
| Delocalized internals (DLC)          | Non-redundant combinations = eigenvectors of G = BBᵀ (Baker, Kessi & Delley 1996)                                                       | Same step quality as redundant, exact dimension 3N−6                                                                                              | Same back-transformation issues                                                                                                                                                                                             |
| HDLC                                 | DLC per residue + Cartesians for residue positions (Billeter, Turner & Thiel 2000)                                                      | Linear-scaling for large systems                                                                                                                  | Worse than TRIC for small molecules (Tsypin et al. 2026: geomeTRIC HDLC 213–378 % of Sella)                                                                                                                                 |
| TRIC                                 | DLC from primitives plus per-fragment translation and rotation coordinates (Wang & Song 2016)                                           | Handles noncovalent complexes without artificial bonds; 1.3–2× fewer steps than Cartesian ANC-like methods on flexible and multi-fragment systems | Rotation coordinates and their derivatives; more code                                                                                                                                                                       |

Data point that frames everything: Tsypin et al. (arXiv:2610.06577, 2026) ran 27
optimizer configurations from six open-source packages on GFN2-xTB. Sella's internal
coordinates were best (≈ 38 force calls per molecule on drug-like test molecules);
pysisyphus redundant internals with a Lindh or Fischer Hessian and TRIC were within 5–40 %;
geomeTRIC TRIC/DLC 30–96 % more; every Cartesian method 2.5–7× more. Their LLM-evolved
variant (AutoSella-F) cut a further 23–35 % mainly through (i) a better physical model
Hessian with noncovalent contact terms and out-of-plane coordinates, (ii) fragment handling
and (iii) Hessian updates that reuse the last few step/gradient pairs after correcting them
for the changing model Hessian.

**We try:** TRIC-style delocalized internals with RFO and BFGS (Method `Internal`) against
ANC. Expected: fewer evaluations on chains, rings and drug-like molecules; ANC likely stays
safer for metal complexes (no bond perception needed).

### 2.2 Initial Hessian

| Option                                                              | Buys                                                                                                                                          | Costs                                                                                                           |
| ------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| Unit / scaled identity                                              | Nothing to compute                                                                                                                            | Many more steps (Tsypin et al.: pysisyphus redundant/unit 3–4.5× the Lindh variant)                             |
| Schlegel (1984)                                                     | Simple rules per internal coordinate                                                                                                          | Needs bond perception; weaker than Lindh for nonbonded contacts                                                 |
| Fischer & Almlöf (1992), Swart & Bickelhaupt (2006)                 | Distance-damped, smoother over geometry changes                                                                                               | Similar to Lindh in practice (Tsypin et al.: Fischer ≈ Lindh, Swart 10–20 % worse)                              |
| Lindh (1995)                                                        | Distance-weighted stretch/bend/torsion, no bond perception, PSD; what #1415 uses (xtb adds a D2-type dispersion term, "S6 in model hess. 20") | Ignores the actual electronic structure; floppy torsions are guessed                                            |
| Exact Hessian at the start ("calcfc")                               | Correct curvature, including soft modes and coupling; Newton-like first steps                                                                 | GFN2 analytic Hessian costs many gradients (grows with N); only worth it when it saves more steps than it costs |
| Cheap-level exact Hessian (GFN-FF analytic) for a GFN2 optimization | Real force-field curvature for almost free                                                                                                    | GFN-FF curvature differs from GFN2 (especially conjugation, metals); costs one GFN-FF Hessian                   |

**We try:** `InitialHessian::{Model, Potential, Auto { max_atoms }}` with the Hessian coming
from the potential (GFN2 analytic, or GFN-FF analytic in a GFN2 run) and a cost rule from the
measured Hessian/gradient time ratio.

### 2.3 Hessian updates

- **BFGS** (what #1415 uses, skipped when yᵀs ≤ 0): keeps positive definiteness, the
  standard for minimization.
- **Damped BFGS** (Powell 1978; Nocedal & Wright 2006, Procedure 18.2): always applies an
  update, mixing y with Hs so curvature stays positive. Helps when many updates would be skipped.
- **SR1/BFGS blend** (Bofill 1994; Farkas & Schlegel 1999 for minimization,
  φ = √φ_Bofill): SR1 learns negative/soft curvature faster; Farkas & Schlegel found it
  better than pure BFGS for large molecules.
- **Multi-pair / model-corrected updates** (AutoSella-F, Tsypin et al. 2026): keep the last
  few pairs and re-apply them on a refreshed model Hessian. Related to our ANC rebuild,
  which carries curvature over.

**We try:** `Update::{Bfgs, DampedBfgs, Bofill}`.

### 2.4 Step control and acceleration

- **RFO** (Banerjee, Adams, Simons & Shepard 1985) with a trust radius (Fletcher; Nocedal &
  Wright ch. 4): what #1415 uses. Our radius starts at 0.3 bohr (min 0.001, max 0.5).
- **GDIIS** (Császár & Pulay 1984; safeguards by Farkas & Schlegel 2002): interpolates the
  last points to cancel the gradient; strong near convergence, unsafe far away.
- **GEDIIS** (Li & Frisch 2006): DIIS on an energy model with convex coefficients, used
  far from convergence in Gaussian's default hybrid; the paper reports fewer steps than RFO
  alone, largest for big flexible molecules.
- **Line searches** (Armijo / Wolfe): what our L-BFGS fallback uses; cost extra gradients.

**We try:** `Acceleration::{None, Gdiis, Gediis}` on top of RFO, and trust-radius tuning
(start, growth) if the data show many short steps.

### 2.5 Other families

- **Preconditioned L-BFGS** (Packwood, Kermode, Mones, Bernstein, Woolley, Gould, Ortner
  & Csányi 2016, J. Chem. Phys. 144, 164109): an exponential or force-field preconditioner
  makes L-BFGS scale well for large systems; O(N) memory. For our ≤ 500-atom limit the
  dense ANC/internal Hessian is affordable and stronger, so we keep L-BFGS only as fallback.
- **FIRE** (Bitzek, Koskinen, Gähler, Moseler & Gumbsch 2006): MD-like, robust, but
  2.4–7× more force calls than internal quasi-Newton on GFN2-xTB (Tsypin et al. Table 5).
  Not tried.
- **Surrogate / Gaussian-process optimizers**: GPMin (Garijo del Río, Mortensen &
  Jacobsen 2019, npj Comput. Mater. 5, 120) and GPR in internal coordinates (Meyer &
  Hauser 2020, J. Chem. Phys. 152, 084112) report fewer force calls than BFGS-type methods for small molecules,
  but the GP solve is O(n³) in (steps × 3N) and becomes the bottleneck above ~30 atoms or
  ~50 steps. With a 10–100 ms xTB gradient the overhead eats the gain for drug-sized
  molecules. Not tried in this round; reconsider for DFT-level potentials.

### 2.6 Cheaper evaluations

- **SCF restart** from the previous geometry's wavefunction: axtb's `Calculator` does it by
  default (stateful per molecule). Measured by the harness's `--no-restart` switch.
- **Adaptive SCF accuracy**: xtb ties SCF accuracy to the optimization level (normal 1.0,
  tight 0.2; xtb docs). We add `Accuracy::Coarse` while the gradient is far above the
  threshold (> 20× max-gradient criterion) and switch to Normal for the end game.
- **Multi-level cascade**: GFN-FF (≈ 1 ms per gradient) pre-optimization, then GFN2 from
  that geometry. Expected big wall-time win from rough starts; little from MMFF starts
  (already near a minimum). Risk: GFN-FF and GFN2 minima can differ in conformation.

## 3. Plan of experiments

All on the same starts (ETKDG unminimized = rough; ETKDG + MMFF94 = how ReShiki starts; given
and noised XYZ for ions, radicals and metals), GFN2-xTB single-threaded, with GFN-FF and MMFF94
as secondary potentials:

1. Baseline: #1415 ANC-RFO, Cartesian L-BFGS, `xtb --opt normal` (official 6.8.0).
2. One knob at a time against the baseline: Hessian start (Lindh / GFN-FF / GFN2 for small N),
   coordinates (ANC / TRIC), update (BFGS / damped / Bofill), acceleration (GDIIS / GEDIIS),
   adaptive SCF accuracy, GFN-FF pre-optimization, SCF restart on/off.
3. Combine the winners; check failures and same-minimum rate; pick the default.

Benchmark harness: `reshiki/optbench` on branch `reshiki-bench` of the axtb fork (LGPL there;
it calls the MIT optimizer as a library).

## 4. Design partner input (gpt-6.2-astra)

Recorded after each round: its predictions, proposals, and which won or lost.

### Before any benchmark (independent forecast from the survey and the #1415 code)

- Expects internals to give ~20 % fewer gradients than ANC overall, most on chains and rings
  (20–40 % from rough starts), least on small rigid Baker molecules (0–2 calls). It cites a
  2026 TURBOMOLE overview (J. Chem. Phys. 165, 162503, doi:10.1063/5.0351594, Table II;
  paywalled, not checked by us) where on 1,752 distorted GFN2 starts the median gradient
  count was 36 for xtb ANCopt, 29 for geomeTRIC TRIC and 19 for a Cartesian optimizer with a
  Lindh-type Hessian and an adaptive trust radius, i.e. step control can matter as much as
  the coordinate system.
- Main weakness it sees in #1415: the trust radius bounds the whole 3N displacement
  (0.3 bohr start, 0.5 max), so a 70-atom molecule moves at most ~0.03 Å RMS per atom per
  step. Proposal: radius per atom (RMS start 0.08 bohr, growing to 0.2, single-atom cap
  0.4 bohr). Also: reject steps with ratio < 0.05, rebuild on poor predictions instead of
  every 30 steps, sweep the curvature floor (1e-4 to 1e-2), add contact curvature to Lindh.
- GFN-FF Hessian as initial curvature for drug-like molecules and chains (15–35 % fewer
  GFN2 gradients on rough starts); GFN-FF pre-optimization only for rough starts.
- Farkas–Schlegel SR1/BFGS blend: small effect (206 vs 213 steps in internals, worse in
  Cartesians per Bakken & Helgaker Tables V–VI); GDIIS/GEDIIS: 0 to a few calls.
- Overhead: loosen the Davidson residual (1e-12 |g| today), time rebuilds and back-transforms.

## Results

(Filled in by the benchmark rounds.)

## References

- J. Baker, J. Comput. Chem. 14, 1085 (1993). doi:10.1002/jcc.540140910
- J. Baker, A. Kessi, B. Delley, J. Chem. Phys. 105, 192 (1996). doi:10.1063/1.471864
- V. Bakken, T. Helgaker, J. Chem. Phys. 117, 9160 (2002). doi:10.1063/1.1515483
- A. Banerjee, N. Adams, J. Simons, R. Shepard, J. Phys. Chem. 89, 52 (1985). doi:10.1021/j100247a015
- S. R. Billeter, A. J. Turner, W. Thiel, Phys. Chem. Chem. Phys. 2, 2177 (2000). doi:10.1039/A909486E
- E. Bitzek et al., Phys. Rev. Lett. 97, 170201 (2006). doi:10.1103/PhysRevLett.97.170201
- J. M. Bofill, J. Comput. Chem. 15, 1 (1994). doi:10.1002/jcc.540150102
- P. Császár, P. Pulay, J. Mol. Struct. 114, 31 (1984). doi:10.1016/S0022-2860(84)87198-7
- Ö. Farkas, H. B. Schlegel, J. Chem. Phys. 111, 10806 (1999). doi:10.1063/1.480484
- Ö. Farkas, H. B. Schlegel, Phys. Chem. Chem. Phys. 4, 11 (2002). doi:10.1039/B108658H
- T. H. Fischer, J. Almlöf, J. Phys. Chem. 96, 9768 (1992). doi:10.1021/j100203a036
- E. Garijo del Río, J. J. Mortensen, K. W. Jacobsen, npj Comput. Mater. 5, 120 (2019).
  doi:10.1038/s41524-019-0254-2 (paper only)
- S. Grimme, C. Bannwarth, P. Shushkov, J. Chem. Theory Comput. 13, 1989 (2017) (ANCopt).
  doi:10.1021/acs.jctc.7b00118; xtb documentation, "Geometry Optimization",
  https://xtb-docs.readthedocs.io/en/latest/optimization.html
- E. D. Hermes, K. Sargsyan, H. N. Najm, J. Zádor, J. Chem. Theory Comput. 18, 6974 (2022) (Sella). doi:10.1021/acs.jctc.2c00395
- X. Li, M. J. Frisch, J. Chem. Theory Comput. 2, 835 (2006). doi:10.1021/ct050275a
- R. Lindh, A. Bernhardsson, G. Karlström, P.-Å. Malmqvist, Chem. Phys. Lett. 241, 423 (1995). doi:10.1016/0009-2614(95)00646-L
- R. Meyer, A. W. Hauser, J. Chem. Phys. 152, 084112 (2020). doi:10.1063/1.5144603
- J. Nocedal, S. J. Wright, Numerical Optimization, 2nd ed., Springer (2006).
- D. Packwood et al., J. Chem. Phys. 144, 164109 (2016). doi:10.1063/1.4947024
- C. Peng, P. Y. Ayala, H. B. Schlegel, M. J. Frisch, J. Comput. Chem. 17, 49 (1996).
- M. J. D. Powell, in Numerical Analysis, Lecture Notes in Mathematics 630, 144 (1978).
- P. Pulay, G. Fogarasi, J. Chem. Phys. 96, 2856 (1992). doi:10.1063/1.462844
- H. B. Schlegel, Theor. Chim. Acta 66, 333 (1984). doi:10.1007/BF00554788
- S. Spicher, S. Grimme, Angew. Chem. Int. Ed. 59, 15665 (2020) (GFN-FF). doi:10.1002/anie.202004239
- M. Swart, F. M. Bickelhaupt, Int. J. Quantum Chem. 106, 2536 (2006). doi:10.1002/qua.21049
- A. Tsypin et al., "Optimizing the Optimizer: Language Models Discover Faster Molecular
  Relaxation Algorithms", arXiv:2610.06577 (2026).
- L.-P. Wang, C. Song, J. Chem. Phys. 144, 214108 (2016). doi:10.1063/1.4952956
