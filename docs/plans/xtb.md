# Plan: quantum chemistry files and xTB calculations

Status: in progress on the `xtb/next` integration branch. Nothing here is released.

ReShiki gains 3D quantum chemistry files (read and write) and semiempirical calculations with
GFN2-xTB, GFN-FF and g-xTB through [axtb](https://github.com/atomsforhumanity/axtb), a pure
Rust xTB implementation. The data model is fixed in [xtb-data.md](xtb-data.md).

## Decisions

| Topic            | Decision                                                                                                                                                                                                                       |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Engine           | A fork of axtb (LGPL-3.0-or-later) pinned by revision: `Ameyanagi/axtb`, branch `reshiki`.                                                                                                                                     |
| Linking          | The `reshiki` executable links the fork only under the Cargo feature `xtb`. The feature is off by default and exists only on the root application crate; library crates never enable it.                                       |
| Process          | Calculations run in a child process of the same executable (`reshiki --xtb-worker`), like `--geometry-worker`. There is no separate helper executable on the desktop.                                                          |
| Desktop builds   | macOS and Linux desktop releases include xTB. Windows desktop releases do not until code signing for them works.                                                                                                               |
| Packages         | Python, npm and crates stay MIT OR Apache-2.0 and calculate nothing unless a package opts in. The Python opt-in package is `reshiki-xtb` (TestPyPI first). Web/WebAssembly calculations come later.                            |
| Methods          | GFN2-xTB (default), GFN-FF and g-xTB. Each method supports a different set of tasks; the table is data in `reshiki-qcdata` ([capabilities](xtb-data.md#method--task-capabilities)).                                            |
| Limits           | Energy, gradient and optimization up to 500 atoms; Hessians up to 200 atoms; trajectories up to 1000 frames; orbital surfaces simplified to about 20,000 triangles for export.                                                 |
| Orbital surfaces | Both an interactive viewer and vector shapes in the drawing (SVG and PDF export).                                                                                                                                              |
| Optimizer        | Our own (MIT OR Apache-2.0), reusable with MMFF.                                                                                                                                                                               |
| Files            | Read Molden, Gaussian fchk, Gaussian `.log` and ORCA `.out`; read and write XYZ (multi-frame), SDF/V3000 3D, and Gaussian, ORCA and GAMESS inputs. Files come before calculations, but every phase runs now in parallel lanes. |

### Licensing

- ReShiki's own code stays MIT OR Apache-2.0. LGPL code (axtb, tblite, xtb) is never copied into
  it. ReShiki code calls the axtb fork's API only under the `xtb` feature.
- Builds with `xtb` statically link LGPL code. The complete ReShiki source and lockfile of every
  release are public, so recipients can rebuild the executable against a modified axtb; the
  release notes link the exact fork revision and the ReShiki tag. Distribution notices list the
  fork, its revision, both GNU licence texts and the fork's change notice. The owner approves
  this analysis and the release wording before the first release that contains xTB.
- The g-xTB parameters are distributed under the permission recorded in the axtb NOTICE. The
  scope of that permission for our fork is checked before the fork or any artifact containing it
  is published.
- Orbital evaluation, the optimizer, parsers and writers are written fresh from format
  specifications, papers and textbooks.
- Test fixtures come only from permissively licensed sources (cclib `data/` and
  `test/io/data/*.molden`, BSD-3-Clause; materialsproject/pymatgen-test-files `io/gaussian`, MIT),
  each recorded with source URL, commit and licence.

## Phases

The order follows the review of the first draft: shared data types first, one real GFN2-xTB
request before packaging depends on it, and user interface after file support.

### Phase 0: engine check

- Fork and pin axtb; build and run its own test structures on macOS (Apple silicon), Ubuntu arm64
  and Arch Linux x64; Windows x64 is checked but not shipped.
- Compare against reference programs per method (xtb 6.8.0 for GFN2-xTB and GFN-FF, tblite in its
  matching GFN2 conventions, the g-xTB release for g-xTB), with pinned binary hashes, recorded
  units, SCF settings, spin and solvent states, case counts and explicit expected failures. A
  missing required reference fails the check. External-reference agreement is kept apart from
  same-engine agreement across platforms (energy 1e-8 Eh, gradient 1e-7 Eh/bohr, frequencies
  0.1 cm⁻¹).
- Benchmark the dependency graph actually shipped, with LAPACK and loader overrides cleared.

### Phase 1: licensing and packaging

- Notices for builds with and without `xtb`; a dependency audit that fails when an LGPL crate
  enters the default dependency graph of any ReShiki crate or a default artifact.
- Release builds for macOS and Linux turn `xtb` on; the release check runs a real GFN2-xTB water
  energy through `reshiki --xtb-worker`.
- The About window and `reshiki --version` report whether xTB is included and link its source.
- Python opt-in package `reshiki-xtb` on TestPyPI first.

### Phase 2: 3D files (every build, no LGPL code)

- 2a, data types: the `reshiki-qcdata` crate and [xtb-data.md](xtb-data.md), then the document integration. It
  alone owns the scientific types, the native format floor and the atom mapping; other lanes
  build on it.
- 2b, writers: XYZ and multi-frame XYZ, SDF/V3000 3D, Gaussian `.gjf`/`.com`, ORCA `.inp`, GAMESS
  `.inp`, a keyword template engine and the export dialog. Writers use the selected scientific
  frame and its explicit charge and multiplicity, never the 2D drawing.
- 2c, readers: multi-frame XYZ, SDF 3D, Molden (geometry, orbitals, frequencies), Gaussian fchk,
  Gaussian `.log` and ORCA `.out` (final geometry, energy, frequencies, orbital energies; tested
  versions listed), and geometry from Gaussian, ORCA and GAMESS inputs. Log readers parse job and
  step records and bind each result to its own geometry and completion state. Readers convert
  orbitals into the one canonical AO contract or report the format as unsupported.

### Phase 3: energy and optimization (`xtb` builds)

- Worker protocol: versioned frames with request IDs, size and dimension checks before
  allocation, per-request, idle and whole-job deadlines, cancellation by ending the child
  process. Energy and gradient first.
- Optimizer crate: bounded Cartesian L-BFGS with diagonal preconditioning and a documented
  stopping rule first; approximate normal coordinates only if benchmarks justify them.
- Calculate panel, results palette, stale-result display on the document storage Phase 2a owns, CLI
  `reshiki calc`, MCP tools and the Python API, all through the existing admission, cancellation
  and revision-checked commit of the agent API.
- Validation against `xtb --opt` on the test set: atom-mapped aligned RMSD and energy
  differences, with basin changes diagnosed.

### Phase 4: frequencies

- Mode list and animation on the 3D model, broadened IR spectrum, thermochemistry, an
  imaginary-mode warning with push-and-reoptimize, and export of a mode as multi-frame XYZ.
- Validation keeps three checks apart: analytic against finite-difference Hessians, vibrational
  analysis of the same Hessian, and end-to-end agreement with `xtb --hess`.

### Phase 5: orbitals

- Level diagram, grid evaluation from the canonical AO contract, marching cubes, mesh
  simplification, vector rendering in the 3D model with explicit hidden-surface handling, SVG and
  PDF export, isovalue control.
- Validation: signed grid values at known points against PySCF's Molden loader and `cubegen`
  (development-only, through uv) for identical coefficients, with mixed-shell fixtures and a
  separate fchk check.

Later, only if asked: web/WebAssembly calculations and an npm package, GFN-FF for live
relaxation, Q-Chem and Turbomole writers.

## Process

- Every lane opens pull requests against `xtb/next` (or the lane branch it stacks on). The owner
  chooses what merges.
- Design and user interface are written first by the lane lead; bounded backend work is
  delegated; every diff gets an independent adversarial review before its pull request.
- Checks run locally on macOS, Ubuntu arm64 and Arch Linux x64 before pushing; CI covers
  Windows x64.
- Visible changes carry before/after screenshots of the real app; numerical work carries tables.

## Risks

- axtb correctness on each platform, a single upstream author and API churn (mitigated by the
  pinned fork).
- LGPL compliance details: notices in every artifact, source availability, rebuildability.
- Native file compatibility for documents that carry results ([xtb-data.md](xtb-data.md#documents)).
- Vector isosurfaces: file size and render time of large meshes, and correct occlusion.
- Log parsers: format drift across Gaussian and ORCA versions.
- Performance of Hessians near 200 atoms and responsive cancellation.
