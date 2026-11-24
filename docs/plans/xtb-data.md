# Design: quantum chemistry data (`reshiki-qcdata`)

Status: design for Phase 2a of [the xTB plan](xtb.md). The crate implements the types, limits,
conversions and validation below; document integration is designed here and implemented later.

## Why a separate crate

Readers, writers, the xTB worker, the optimizer and the user interface all exchange the same
scientific data. One small crate fixes it once:

- `crates/qcdata`, package `reshiki-qcdata`, MIT OR Apache-2.0.
- Dependencies: `serde`, `serde_json` (tests), `sha2`, `thiserror`; all already in the lockfile.
  No dependency on axtb, on `reshiki-model` or on any LGPL crate, so every build and every
  package can use it.
- Plain structs and enums with public fields, conversions, validation and size limits. No
  calculation, no I/O, no document code.

The existing drawing types stay as they are. `Model3d` keeps `f32` display coordinates;
scientific data lives beside the drawing in `f64` and is never derived from the 2D drawing.

## Units

Every physical field carries its unit in its name, so JSON is self-describing:

| Quantity | Unit | Field suffix |
| --- | --- | --- |
| Positions | ångström | `_angstrom` |
| Energies | hartree (Eh) | `_hartree` |
| Gradient | Eh/bohr | `_hartree_per_bohr` |
| Hessian | Eh/bohr² | `_hartree_per_bohr2` |
| Gaussian exponents | bohr⁻² | `_per_bohr2` |
| Dipole | e·bohr | `_e_bohr` |
| Frequencies | cm⁻¹, imaginary as negative | `_cm1` |
| Masses | unified atomic mass unit (Da) | `_amu` |
| IR intensities | km/mol | `_km_mol` |
| Temperature, pressure | K, Pa | `_k`, `_pa` |

Constants (CODATA 2022): `ANGSTROM_PER_BOHR = 0.529177210544`,
`DEBYE_PER_E_BOHR = 2.541746473`. Orbital evaluation converts frame positions to bohr with
`ANGSTROM_PER_BOHR` before using exponents.

Every `f64` must be finite; validation rejects NaN and infinities (JSON cannot carry them anyway).

## Types

### `Frame`

One molecular geometry, immutable once stored.

| Field | Type | Meaning |
| --- | --- | --- |
| `numbers` | `Vec<u8>` | Atomic numbers, 1 to 118. |
| `positions_angstrom` | `Vec<[f64; 3]>` | Cartesian positions, same length. |
| `isotopes` | `Vec<u16>` | Empty, or a mass number per atom (0 = natural abundance). |
| `masses_amu` | `Vec<f64>` | Empty, or an explicit mass per atom (overrides isotopes for vibrational analysis). |
| `charge` | `i32` | Total charge. |
| `multiplicity` | `u32` | 2S + 1, at least 1. |
| `drawing_atoms` | `Vec<Option<u64>>` | Empty, or a drawing atom ID per atom (`None` for atoms the drawing does not show, such as folded hydrogens). IDs are distinct. |

Validation: lengths agree; atomic numbers in range; positions finite; masses positive; the
electron count Σ Z − charge is non-negative and has the parity multiplicity − 1 requires;
the atom count is at most `MAX_FRAME_ATOMS`.

`Frame::hash()` returns a `FrameHash`: SHA-256 over a versioned binary encoding of the atom
count, numbers, isotopes (absent = all zero), masses (absent and present are distinct),
positions (`f64` bits, little-endian, `-0.0` written as `0.0`), charge and multiplicity. The
drawing map is not hashed: it does not change the physics. A `FrameHash` serializes as 64
lowercase hex digits.

### `Trajectory`

`steps: Vec<Step>`, each `Step { frame: Frame, energy_hartree: Option<f64>, label: Option<String> }`.
Every step has the same atomic numbers, charge and multiplicity as the first (readers split a
file whose frames differ). At most `MAX_FRAMES` steps and `MAX_TRAJECTORY_ATOMS` atoms over all
steps.

### `BasisSet`, `Shell` and `Orbitals`: the canonical AO contract

This is the one convention every orbital producer converts into and every consumer (grid
evaluation, writers) reads. A reader that cannot map its source onto it reports the format as
unsupported instead of guessing.

`BasisSet { shells: Vec<Shell>, core_electrons: Vec<u32> }`

`Shell { atom: u32, l: u8, pure: bool, exponents_per_bohr2: Vec<f64>, coefficients: Vec<f64> }`

1. **Centres.** `atom` indexes the frame the result belongs to; shells of one atom may appear in
   any order, and the AO order is the shell order.
2. **Representation per shell.** `pure` selects real solid harmonics (2l + 1 functions) or
   Cartesian monomials ((l + 1)(l + 2)/2 functions). Mixed shells in one basis are allowed. For
   `l ≤ 1` the two coincide and `pure` must be `false`. `l ≤ MAX_L` (5, h functions).
3. **SP shells** do not exist in the canonical form: a reader splits an SP shell into an s shell
   and a p shell with the same exponents, s first. The AO order is unchanged (s, x, y, z).
4. **Cartesian component order**: for `a` from `l` down to 0, for `b` from `l − a` down to 0,
   `c = l − a − b`, the component is x^a y^b z^c. So d is xx, xy, xz, yy, yz, zz.
5. **p order**: x, y, z.
6. **Pure order and signs**: m = −l … +l. The functions are the real solid harmonics without the
   Condon–Shortley phase, with positive leading coefficients:

   | l | m = −3 | −2 | −1 | 0 | +1 | +2 | +3 |
   | --- | --- | --- | --- | --- | --- | --- | --- |
   | 2 | | xy | yz | 2z² − x² − y² | xz | x² − y² | |
   | 3 | 3x²y − y³ | xyz | y(4z² − x² − y²) | z(2z² − 3x² − 3y²) | x(4z² − x² − y²) | z(x² − y²) | x³ − 3xy² |

   g and h follow the same real solid harmonics (Helgaker, Jørgensen and Olsen, *Molecular
   Electronic-Structure Theory*, §6.4).
7. **Contraction coefficients** multiply normalized primitives: shell `l`, exponent α,
   coefficient d gives the radial sum Σ d · n(α, l) · exp(−α r²) with
   n(α, l) = (2α/π)^{3/4} (4α)^{l/2} / √((2l − 1)!!), the normalization of the x^l primitive
   (the convention of Gaussian-format basis sets and Molden files).
8. **AO normalization**: every AO (each Cartesian component and each pure function) has unit
   self-overlap: AO = N · A(x, y, z) · radial sum, with A the monomial or harmonic above and
   N > 0 fixed by ⟨AO|AO⟩ = 1. MO coefficients refer to these unit AOs.
9. **Spin.** `Orbitals { basis, alpha: OrbitalSet, beta: Beta }` with
   `Beta::Restricted` (closed shell; alpha occupations are totals 0 … 2),
   `Beta::SameOrbitals { occupations }` (one set of orbitals with separate alpha and beta
   occupations 0 … 1, as GFN2-xTB open shells) or `Beta::Unrestricted(OrbitalSet)`.
10. **Orbital sets.** `OrbitalSet { energies_hartree, occupations, coefficients, first_coefficient_mo, symmetries }`:
    energies and occupations for every MO in ascending order of energy; `coefficients` is
    row-major, one row of `n_ao` values per MO, for the contiguous MOs starting at
    `first_coefficient_mo` (all MOs, or a window around the frontier orbitals for large
    systems); `symmetries` is empty or one label per MO.
11. **Electrons not in the basis.** `core_electrons` is empty or one count per atom: electrons
    an ECP removes, or the core electrons of a valence-only method such as xTB. Occupations
    sum to Σ Z − charge − Σ core.

#### Molden conversion

The Molden format (<https://www.theochem.ru.nl/molden/molden_format.html>) differs from the
canonical form in component order and, for Cartesian shells, possibly normalization:

| Shell | Molden order |
| --- | --- |
| p | x, y, z |
| 6D | xx, yy, zz, xy, xz, yz |
| 10F | xxx, yyy, zzz, xyy, xxy, xxz, xzz, yzz, yyz, xyz |
| 15G | xxxx, yyyy, zzzz, xxxy, xxxz, yyyx, yyyz, zzzx, zzzy, xxyy, xxzz, yyzz, xxyz, yyxz, zzxy |
| 5D, 7F, 9G | m = 0, +1, −1, +2, −2, … |

Molden has no h shells. Contraction coefficients already follow rule 7.

The specification does not say how Cartesian components are normalized, and producers differ:
some write coefficients of unit components, others of components that share the x^l
normalization ("axial"). The converters therefore take `CartesianNorm::{Unit, Axial}`. With
the axial convention, a component x^a y^b z^c has squared norm
r(a, b, c) = (2a − 1)!! (2b − 1)!! (2c − 1)!! / (2l − 1)!!, so the canonical coefficient is the
Molden coefficient times √r. Each reader picks the convention of its identified producer and
proves it on a fixture; producer-specific sign changes of pure functions are the reader's
adapter, not part of this conversion.

Functions: `canonical_cartesian_order(l)`, `molden_cartesian_order(l)`, `molden_pure_order(l)`,
`axial_norm_ratio(a, b, c)`, `primitive_norm(alpha, l)`, `shell_from_molden` /
`shell_to_molden` (one shell's block of one MO) and `mo_from_molden` / `mo_to_molden` (a whole
MO over a basis). Tests use hand-built shells: permutations checked component by component,
normalization ratios checked against numerical quadrature of the one-dimensional Gaussian
integrals, and round trips for every l and both conventions.

### `Vibrations`

| Field | Meaning |
| --- | --- |
| `frequencies_cm1` | Harmonic frequencies in ascending order; imaginary ones negative. Rigid-body modes removed. |
| `modes` | Empty, or one mode per frequency: Cartesian displacements per atom (`Vec<[f64; 3]>`), **not mass-weighted**, scaled to unit Euclidean norm over all 3N components. The overall sign of a mode is arbitrary. |
| `reduced_masses_amu` | Empty or one per mode. |
| `ir_intensities_km_mol` | Empty or one per mode. |
| `masses_amu` | Empty, or the masses the analysis used (needed to return to mass-weighted coordinates). |

`imaginary_count()` counts negative frequencies, however small; the warning threshold for
near-zero negative modes belongs to the user interface.

### `Thermo`

Temperature, pressure, zero-point energy, enthalpy correction H(T) − E (including the ZPE),
entropy (Eh/K), Gibbs correction H − TS, optional heat capacity, and the analysis settings:
`frequency_scale`, `rotor_cutoff_cm1`, `imaginary_cutoff_cm1`, `symmetry_number`. The total free
energy is the result's energy plus the Gibbs correction.

### `CalcResult`

One calculation or one imported job step, bound to the frame it describes.

| Field | Meaning |
| --- | --- |
| `frame` | `FrameHash` of the geometry the values belong to (for an optimization, the final geometry). |
| `task` | `Energy`, `Gradient`, `Optimize` or `Hessian`. |
| `settings` | `Settings::Xtb(XtbSettings)` or `Settings::Imported { keywords }`. |
| `provenance` | `Provenance`. |
| `status` | `Converged`, `NotConverged { reason }` or `Failed { reason }`. |
| `energy_hartree`, `energy_terms` | Total energy and named components (`BTreeMap<String, f64>`, Eh), for example the parts of a solvent composite. |
| `gradient_hartree_per_bohr` | Empty or one vector per atom. |
| `hessian_hartree_per_bohr2` | Empty or (3N)² values, row-major, Cartesian, not mass-weighted. |
| `charges` | `Vec<AtomicCharges { scheme, values }>`; scheme `Mulliken`, `Eeq`, `Cm5`, `Hirshfeld`, `Loewdin` or `Other(String)`. |
| `dipole_e_bohr` | Optional, about the origin of the frame. |
| `bond_orders` | `Vec<BondOrder { a, b, order }>` (Wiberg–Mayer), sparse, `a < b`. |
| `orbitals`, `vibrations`, `thermo` | Optional. |
| `trajectory` | Optional path (an optimization's steps; its last step is `frame`). |

`CalcResult::validate(&frame)` checks the hash, every array length against the frame and the
basis, orbital and occupation consistency, and limits.

### `XtbSettings` and `Provenance`

`XtbSettings { method, solvent, electronic_temperature_k, accuracy, max_iterations, gfnff }`:

- `method`: `XtbMethod::{Gfn2Xtb, GfnFf, GXtb}`. `GXtb` is g-xTB 2.0.1.
- `solvent`: `Solvent { model: SolventModel::Alpb, name, correction }`. For g-xTB the
  correction must be `Some(SolventCorrection::Gfn2)`: the composite
  E = E(g-xTB) + [E(GFN2-xTB + ALPB) − E(GFN2-xTB)], named explicitly so a solvent never
  silently means it. GFN2-xTB takes no correction; GFN-FF takes no solvent.
- `gfnff`: required for GFN-FF, absent otherwise: `GfnFfTopology { reference: FrameHash, fragment_charges: Vec<i32> }`,
  the frame the topology was set up from and the fragment charges, so a restarted
  optimization or a later Hessian uses the same energy surface.

`Provenance { program, program_version, method, basis, source }` with
`Source::Calculated { engine_revision }` or `Source::Imported { format, file_name, job }`
(`job` is the job or step index in a multi-job log).

A result is current for a frame exactly when `result.frame == frame.hash()`; whether its
settings are still the ones the user wants is a comparison of `settings` values.

## Method × task capabilities

`XtbMethod::capabilities()` returns this table as data, from axtb's documented behaviour; the
worker and the user interface both read it, and `check_calculation` enforces it.

| Method | Energy | Gradient | Optimize | Hessian | Orbitals | Bond orders | ALPB solvent | Open shells |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| GFN2-xTB | yes | yes | yes | yes (analytic) | yes | yes | native | yes |
| GFN-FF | yes | yes | yes | yes (analytic) | no | no | no | no (singlets) |
| g-xTB | yes | yes | yes | yes (analytic) | yes | yes | GFN2 correction composite | yes |

Further rules from the same source: GFN-FF sets its topology from the first or a reference
geometry and needs fragment charges that sum to the total charge (default: all on the first
fragment). g-xTB's orbitals, charges and dipole in solvent are those of its g-xTB part.

`check_calculation(task, &settings, &frame, want_orbitals)` validates the frame, the size limit
of the task, the solvent rule, open-shell support, orbital support and GFN-FF fragment charges.
Every boundary that admits a calculation (worker request, CLI, MCP, Python, the Calculate
panel) calls it; the worker calls it again on its side.

## Limits

| Constant | Value | Applies to |
| --- | --- | --- |
| `MAX_CALC_ATOMS` | 500 | Energy, gradient and optimization. |
| `MAX_HESSIAN_ATOMS` | 200 | Hessians and vibrational analysis. |
| `MAX_FRAMES` | 1000 | Steps in a trajectory. |
| `MAX_FRAME_ATOMS` | 10,000 | Any stored frame (imported files larger than this keep their drawing and get no scientific frame, with a warning). |
| `MAX_TRAJECTORY_ATOMS` | 1,000,000 | Atoms summed over a trajectory's steps. |
| `MAX_BASIS_FUNCTIONS` | 10,000 | AOs in a basis. |
| `MAX_COEFFICIENTS` | 4,194,304 | Stored MO coefficients per spin channel (32 MiB). |
| `MAX_PRIMITIVES` | 64 | Primitives per shell. |
| `MAX_L` | 5 | Angular momentum. |

Validation enforces them after parsing. Bounding allocation during parsing is the job of each
boundary: native files and worker messages are size-capped in bytes before they are parsed.

## Documents

Design only; a later lane implements it in `reshiki-model`.

- **Storage apart from the drawing.** `Document` gains `science: Science` (skipped when empty)
  with `frames: Vec<Arc<Frame>>`, `trajectories: Vec<Arc<Trajectory>>` and
  `results: Vec<Arc<CalcResult>>`. Frames are immutable: an edit or an optimization adds a new
  frame. `Model3d` gains an optional `frame: Option<FrameHash>` to display a scientific frame;
  its own `f32` atoms stay the display copy.
- **Atom mapping.** `Frame::drawing_atoms` maps scientific atoms to drawing atoms. Each stored
  frame also records the drawing fingerprint of its mapped atoms (element, charge, isotope,
  bonds), the same digest `Model3d` uses. A drawing edit never changes a frame; when the
  fingerprint no longer matches, the frame and its results are shown as belonging to an earlier
  structure rather than silently re-mapped. Deleting mapped atoms leaves the frame with
  `None` entries; deleting the frame removes its results.
- **Staleness.** A result is valid for exactly the frame whose hash it carries; a late worker
  reply is committed only if its frame is still stored, through the agent API's
  revision-checked commit, so a reply after Undo is discarded.
- **Native version.** A new constant (the next free document version when this lands, 42 if
  nothing else claims it) is the floor for any document whose `science` is non-empty, and
  `Document::file_version()` checks `science` first, so saving can never write science under an
  older version. Older ReShiki reads the version before parsing and asks the user to update, so
  it cannot open such a file and later save it without the data. Documents without science keep
  their current version. Reading validates every frame and result (`validate`) after the byte
  cap.
- **Undo history.** History frames clone the `Arc`s, never the payloads. `frame_bytes` counts
  each payload's `heap_bytes()` once per history: a payload is charged to the first frame that
  holds it (pointer identity), as pictures already are not counted per frame. A per-document
  science budget (256 MiB of `heap_bytes()`) applies at attach time, in the editor and in the
  agent API quotas.
- **Saved size.** Orbital coefficients above 1,048,576 per channel are saved as a frontier
  window (HOMO − 10 … LUMO + 10, plus every MO a placed surface uses); energies and
  occupations are always saved whole.
- **Generated data.** Grids, isosurface meshes and simplified meshes are never saved and never
  enter history: they are an editor-side cache keyed by result, MO index, isovalue and grid
  settings. A surface placed in the drawing saves only that request (result, MO, isovalue,
  colours) and is rebuilt on open.
- **Copy and paste, recovery.** The first integration does not carry science through the
  clipboard (pasted molecules arrive without results); recovery drafts are full native
  documents and keep it.
- **Tests the integration lane owes**: older-reader rejection of a result-bearing file, legacy
  reads, save/reopen without a worker, copy/paste, recovery, removal of the final result
  (version falls back), Undo followed by a late reply, and history byte accounting with shared
  payloads.

## Not in this crate

Readers and writers, the worker protocol, orbital grid evaluation, the optimizer and document
integration are other lanes. They depend on this crate; this crate depends on none of them.
