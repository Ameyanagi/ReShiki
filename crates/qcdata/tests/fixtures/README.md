`claisen-irc.json` contains the existing 14-atom Claisen verification run's
121 traced IRC geometries, two relaxed endpoints, and 15 NEB images. It was derived from `w2-ts/runs-final/
claisen/{irc,path}.xyz` and `w2-ts/runs-final/claisen.json` in the owner-provided
`xtb-program` scratch evidence. The arrays retain the report's full-precision
hartree energies; the ångström coordinates use qcdata's lossless deflate of
single-precision coordinates. The masses are the search's natural-abundance
table values: C 12.011, H 1.008, O 15.999 amu.

The source arrays include the relaxed endpoints at their first and last indices.
The fixture tests separate those endpoints when constructing an IRC result. The
ordered trace runs through one saddle. Both branches exhausted their downhill step budgets
before the subsequent endpoint relaxations; their `completed` flags therefore
remain false even though the relaxed endpoint classifications connect the
requested reaction. The fixture is computed numerical output, included under
the repository's MIT license; it contains no engine source code.

The opt-in real-worker discovery regression reuses this fixture and the native
`Worker` and scheme discovery backend. It reevaluates the 15 saved NEB images
with GFN2-xTB, refines and independently checks their saddle, follows both IRC
branches, and allows one additional search to connect an unexpected relaxed
reactant conformer to the requested reactant. Run it with an xTB-enabled ReShiki
executable; the test does not open the GUI:

```sh
cargo build --locked --release --features xtb --bin reshiki
RESHIKI_XTB_EXECUTABLE="$PWD/target/release/reshiki" \
RESHIKI_DISCOVERY_EVIDENCE=/tmp/claisen-discovery.json \
cargo test --locked --features xtb --bin reshiki \
  app::xtb::ts::job::discovery::evidence::captured_claisen_retains_the_unexpected_conformer_and_searches_its_gap \
  -- --ignored --exact --nocapture
```

An existing optimized xTB-enabled executable can be supplied instead of
rebuilding it. The test harness itself may remain a debug build.
`RESHIKI_DISCOVERY_EVIDENCE` is optional; the test also prints its evidence as
JSON. The calculation has a 180-second cancellation deadline and a two-search
budget including the saved initial band. Ordinary test runs skip it.

The recorded GFN2-xTB result is three minima and two verified saddles. The first
edge is an interconversion between reactant conformers with the same mapped
bond graph; the second has the Claisen bond changes. The unexpected conformer
is about 0.767 kcal/mol lower than the requested one. Electronic forward/reverse
barriers are about 1.967/2.735 kcal/mol for the conformer edge and
27.261/34.668 kcal/mol for the Claisen edge. These are semiempirical model
results, not experimental barriers or Gibbs free energies. The test checks
independent target-method Hessians at all three minima and both saddles,
requiring positive minimum spectra, exactly one significant imaginary saddle
mode, and two converged IRC branches per edge.

`claisen-discovery-reference.json` records the fixture hash, pinned engine,
settings, actual minimum/saddle energies, independent frequencies, endpoint
residuals, route and runtime from the native regression. These are attributable
computed values rather than bitwise golden assertions; the opt-in test exports
full geometries and spectra for a new run.

This case also exercises a practical precision limit: an earlier force-only
normalization target of 1e-10 stalled near a gradient norm of 2.5e-6 Eh/bohr
with the fixed-SCF worker. Identity checks first use the independently checked
Cartesian Hessian and gradient: after projecting global translations and
rotations, the Newton residual must have equal-atom RMSD below 1.56e-3 bohr and
all physical curvatures must be positive. Only an unresolved geometry needs
stronger optimization, with relative force targets and fresh residual checks
over at most eight batches and 100 accepted steps. Every same-graph reference is
checked, including
one whose initial Normal-converged energy differs by more than 0.1 kcal/mol;
soft strain alone must not create a new intermediate. Validated same-basin
representations can correct edge-free endpoints, while already linked minima
and their checked barrier data are retained. Unresolved curvature, extra flat
fragment modes, or precision-limited matches remain explicit gaps. The
regression does not claim that every molecular intermediate or conformational
path will be found within these budgets.
