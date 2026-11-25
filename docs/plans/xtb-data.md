# Design: quantum chemistry data (`reshiki-qcdata`)

Status: design for Phase 2a of [the xTB plan](xtb.md), revised after an adversarial review and
the owner's file-size policy. The crate implements the types, limits, compact arrays,
conversions and validation below. Phase 2a also owns the native schema, version gate and atom
mapping described under [Documents](#documents); that integration lands in a follow-up pull
request of the same phase, and Phase 3 consumes it.

## Why a separate crate

Readers, writers, the xTB worker, the optimizer and the user interface exchange the same
scientific data. One small crate fixes it once:

- `crates/qcdata`, package `reshiki-qcdata`, MIT OR Apache-2.0.
- Dependencies: `serde`, `serde_json`, `sha2`, `thiserror`, `base64` and `miniz_oxide`; all
  already in the lockfile. No dependency on axtb, on `reshiki-model` or on any LGPL crate, so
  every build and every package can use it.
- Plain structs and enums with public fields, conversions, validation and size limits. No
  calculation, no I/O, no document code.

The drawing types stay as they are. `Model3d` keeps `f32` display coordinates; scientific data
lives beside the drawing in `f64` and is never derived from the 2D drawing or from `Model3d`.
Readers build scientific frames from the file's own records, before hydrogen folding, crystal
deduplication or the `f32` conversion of the drawing import.

## Units and conventions

Every physical field carries its unit in its name, so JSON is self-describing:

| Quantity           | Unit                        | Field suffix         |
| ------------------ | --------------------------- | -------------------- |
| Positions          | ångström                    | `_angstrom`          |
| Energies           | hartree (Eh)                | `_hartree`           |
| Gradient           | Eh/bohr                     | `_hartree_per_bohr`  |
| Hessian            | Eh/bohr²                    | `_hartree_per_bohr2` |
| Gaussian exponents | bohr⁻²                      | `_per_bohr2`         |
| Dipole             | e·bohr                      | `_e_bohr`            |
| Frequencies        | cm⁻¹, imaginary as negative | `_cm1`               |
| Masses             | unified atomic mass unit    | `_amu`               |
| IR intensities     | km/mol                      | `_km_mol`            |
| Temperature        | K                           | `_k`                 |
| Pressure           | Pa                          | `_pa`                |

- Constants (CODATA 2022): `ANGSTROM_PER_BOHR = 0.529177210544`,
  `DEBYE_PER_E_BOHR = 2.541746473`. Orbital evaluation converts frame positions to bohr before
  using exponents.
- Per-atom Cartesian arrays are atom-major: x, y, z of atom 0, then atom 1, and so on. The
  Hessian's rows and columns follow the same order.
- Gradients are derivatives of the energy. Readers of programs that print forces negate them.
- Every `f64` is finite; validation rejects NaN and infinities.

## Types

### `Frame`

One molecular geometry, immutable once stored.

| Field                | Type               | Meaning                                                                                                                                                                                                              |
| -------------------- | ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `numbers`            | `Vec<u8>`          | Atomic numbers, 1 to 118.                                                                                                                                                                                            |
| `positions_angstrom` | `Vec<[f64; 3]>`    | Cartesian positions, same length.                                                                                                                                                                                    |
| `isotopes`           | `Vec<u16>`         | Empty, or a mass number per atom (0 = natural abundance).                                                                                                                                                            |
| `masses_amu`         | `Vec<f64>`         | Empty, or an explicit mass per atom (overrides isotopes for vibrational analysis).                                                                                                                                   |
| `charge`             | `i32`              | Total charge.                                                                                                                                                                                                        |
| `multiplicity`       | `u32`              | 2S + 1, at least 1.                                                                                                                                                                                                  |
| `drawing_atoms`      | `Vec<Option<u64>>` | Empty, or the drawing atom each atom came from when the frame was made (`None` for atoms the drawing does not show). IDs are distinct. A record of identity at creation; the live link is the document's attachment. |

Validation: lengths agree; atomic numbers in range; positions finite; masses positive; the
electron count N = Σ Z − charge is non-negative, has the parity multiplicity − 1 requires, and
multiplicity − 1 ≤ N; the atom count is at most `MAX_FRAME_ATOMS`.

`Frame::hash()` returns a `FrameHash`: SHA-256 over this encoding, all integers little-endian:
the ASCII bytes `reshiki-qcdata frame 1` and a zero byte; atom count as `u64`; each atomic number
as `u8`; each isotope as `u16` (absent = all zero); a tag byte `0` (no masses) or `1` followed by
each mass as `f64` bits; each position atom-major x, y, z as `f64` bits, with `-0.0` written as
`0.0`; charge as `i32`; multiplicity as `u32`. The drawing map is not hashed. A `FrameHash`
serializes as 64 lowercase hex digits. A golden hash for water is pinned in the tests.

### `Trajectory`

A sequence of geometries of one molecule: an optimization, a scan, a reaction path, a
conformer set, a multi-frame file.
`Trajectory { numbers, isotopes, masses_amu, charge, multiplicity, positions_angstrom, energies_hartree, labels, minimizer_energies_hartree, cluster_sizes }`.
The atoms are stored once; `positions_angstrom` is flat (steps × atoms × 3), saved as `f32`, and
heavy except in a conformer result, where the positions are the result
([File size](#file-size)); it is empty when the steps were not saved. `energies_hartree`,
`labels`, `minimizer_energies_hartree` and `cluster_sizes` are empty or one per step and decide
the step count when the positions are absent. Only optimization, conformer and
transition-state results carry a trajectory; the last step of an optimization and the first
conformer equal the result frame at `f32` precision. For a conformer set, `minimizer_energies_hartree` (the energy from the
minimizer that made the starts, such as MMFF94s) and `cluster_sizes` (how many starts fell into
each conformer's cluster) are empty or one per step; by convention other producers leave them
empty.
`Trajectory::frame(i)` builds the frame of step `i`.

Result identity never depends on a trajectory: the input and final frames of an optimization
are stored exactly as frames of their own. A frame built from a saved step carries `f32`
precision (about 1e-7 relative, 6e-5 Å at 1000 Å) and gets its own hash; "Use this geometry"
makes such a new frame.

Readers split a file whose frames have different atoms. The frame limit applies before
splitting (all frames of one file) and to every producer, including the optimizer while it
emits steps: at most `MAX_FRAMES` steps and `MAX_TRAJECTORY_ATOMS` atoms over all steps.

### `BasisSet`, `Shell` and `Orbitals`: the canonical AO contract

This is the one convention every orbital producer converts into and every consumer (grid
evaluation, writers) reads. A reader that cannot map its source onto it reports the format as
unsupported instead of guessing.

`BasisSet { shells: Vec<Shell> }`

`Shell { atom: u32, l: u8, pure: bool, exponents_per_bohr2: Vec<f64>, coefficients: Vec<f64> }`

1. **Centres.** `atom` indexes the frame the result belongs to. The AO order is the shell order.
2. **Representation per shell.** `pure` selects real solid harmonics (2l + 1 functions) or
   Cartesian monomials ((l + 1)(l + 2)/2 functions). Mixed representations in one basis are
   allowed. For `l ≤ 1` the two coincide and `pure` must be `false`. `l ≤ MAX_L` (5).
3. **Contractions.** At least one primitive, at most `MAX_PRIMITIVES`; exponents positive;
   coefficients finite; the contraction's self-overlap (below) positive and finite.
4. **SP shells** do not exist in the canonical form: a reader splits an SP shell into an s shell
   with the s coefficients and a p shell with the p coefficients, same exponents, s first. The
   AO order is unchanged (s, x, y, z).
5. **Cartesian component order**: for `a` from `l` down to 0, for `b` from `l − a` down to 0,
   `c = l − a − b`, the component is x^a y^b z^c. So d is xx, xy, xz, yy, yz, zz, and p is
   x, y, z.
6. **Pure order and phases**: m = −l … +l. The angular part of function (l, m) is
   r^l P_l^|m|(cos θ) cos(mφ) for m ≥ 0 and r^l P_l^|m|(cos θ) sin(|m|φ) for m < 0, where
   P_l^m is the associated Legendre function **without** the Condon–Shortley factor (−1)^m.
   For d and f this gives, up to positive factors:

   | l   | m = −3    | −2  | −1               | 0                  | +1               | +2         | +3        |
   | --- | --------- | --- | ---------------- | ------------------ | ---------------- | ---------- | --------- |
   | 2   |           | xy  | yz               | 2z² − x² − y²      | xz               | x² − y²    |           |
   | 3   | 3x²y − y³ | xyz | y(4z² − x² − y²) | z(2z² − 3x² − 3y²) | x(4z² − x² − y²) | z(x² − y²) | x³ − 3xy² |

7. **Primitive normalization.** Contraction coefficients multiply normalized primitives: shell
   `l`, exponent α, coefficient d gives the radial sum R(r) = Σ d · n(α, l) · exp(−α r²) with
   n(α, l) = (2α/π)^{3/4} (4α)^{l/2} / √((2l − 1)!!), the normalization of the x^l primitive
   (the convention of Gaussian-format basis sets and Molden files). The contraction's
   self-overlap is S = Σᵢ Σⱼ dᵢ dⱼ n(αᵢ, l) n(αⱼ, l) (2l − 1)!! π^{3/2} / (2^l (αᵢ + αⱼ)^{l + 3/2}),
   which is 1 for a single primitive with d = 1.
8. **AO normalization**: every AO (each Cartesian component and each pure function) has unit
   self-overlap: AO = N · A(x, y, z) · R(r), with A the monomial or harmonic above and N > 0
   fixed by ⟨AO|AO⟩ = 1, whatever the scale of the contraction coefficients. MO coefficients
   refer to these unit AOs.
9. **Spin.** `Orbitals { basis: Option<BasisSet>, core_electrons: Vec<u32>, alpha: OrbitalSet, beta: Beta }`
   with `Beta::Restricted` (closed shell; alpha occupations are totals 0 … 2),
   `Beta::SameOrbitals { occupations }` (one set of orbitals with separate alpha and beta
   occupations 0 … 1, as GFN2-xTB open shells) or `Beta::Unrestricted(OrbitalSet)`.
10. **Orbital sets.**
    `OrbitalSet { energies_hartree, occupations, symmetries, rows, coefficients }`: energies in
    ascending order and occupations (empty when the source has none), one per MO of the source's
    spectrum; `symmetries` empty or one label per MO; `rows` the strictly increasing MO indices
    that have coefficients, and `coefficients` one row of `n_ao` values per entry of `rows`.
    A full set has every row; a large system or a light result keeps only some rows or none.
    `basis` is present exactly when some set has rows.
11. **Electrons not in the basis.** `core_electrons` is empty or one count per atom, each at
    most Z: electrons an ECP removes, or the core electrons of a valence-only method such as
    xTB. It stays with the orbitals when the basis is dropped. Readers take it from producer
    metadata or an explicit choice; they never infer it from occupations alone (a Molden file
    cannot tell neutral Na with a 10-electron core from Na¹⁰⁺).

Occupation ranges are checked with a tolerance of 1e-6. The total occupation is not checked
by the crate: imports may list partial spectra or rounded occupations, and xTB uses fractional
occupations at finite electronic temperature. Readers check their own totals against producer
metadata.

#### Molden conversion

The Molden format (<https://www.theochem.ru.nl/molden/molden_format.html>) differs from the
canonical form in component order and, for Cartesian shells, possibly normalization:

| Shell      | Molden order                                                                             |
| ---------- | ---------------------------------------------------------------------------------------- |
| p          | x, y, z                                                                                  |
| 6D         | xx, yy, zz, xy, xz, yz                                                                   |
| 10F        | xxx, yyy, zzz, xyy, xxy, xxz, xzz, yzz, yyz, xyz                                         |
| 15G        | xxxx, yyyy, zzzz, xxxy, xxxz, yyyx, yyyz, zzzx, zzzy, xxyy, xxzz, yyzz, xxyz, yyxz, zzxy |
| 5D, 7F, 9G | m = 0, +1, −1, +2, −2, …                                                                 |

Molden and PySCF exports already follow rule 7 for contraction coefficients. Molden chooses
pure or Cartesian once per angular momentum for the whole file and has no h shells, so
writing a basis that mixes representations within one `l`, or has `l = 5`, is an error.
`[STO]` bases are unsupported.

The specification does not say how Cartesian components are normalized, and producers differ.
The converters take `CartesianNorm`:

- `Unit`: each Molden component is a unit-norm AO, as in the canonical form.
- `Axial`: every component shares one normalization constant, the one that gives the x^l
  component unit norm. Component x^a y^b z^c then has squared norm
  r(a, b, c) = (2a − 1)!! (2b − 1)!! (2c − 1)!! / (2l − 1)!!, so importing multiplies the MO
  coefficient by √r and exporting divides by it. A d_xy coefficient of 1 becomes 1/√3.

The cclib Molden 5.7 carbon fixture verifies Cartesian d/f/g `Unit` normalization by MO
orthonormality. By import policy, the Molden writer and unidentified producers use the
specification's pure d/f/g convention, verified on PySCF 2.15.0 exports. Producer-specific
sign changes of pure functions belong to the reader's adapter. The printed G09 and G16
Mo4OCl4-sp overlaps and the G09 fchk signed grid reference verify that the Gaussian pure d
order and signs match Molden's.

The ORCA adapter divides bare radial coefficients by `primitive_norm(alpha, l)` and flips
the pure f coefficients at m = ±3. The implied AO overlap of the complete water TZVP and
formaldehyde TDDFT MO sets verifies the radial conversion on contracted s/p shells. Every
d/f shell in these fixtures has one primitive. The overlap phase checks assume ORCA's p
components are x, y, z and establish the pure d/f phases used for the signed grid references.
The adapter requires the exact `[Title]` prefix `Molden file created by orca_2mkl for BaseName=`,
with the bare program name. ORCA files without this header keep only orbital energies.
Pure g and Cartesian d/f/g remain unverified and cause the reader to keep only the orbital
spectrum.

Functions: `canonical_cartesian_order(l)`, `molden_cartesian_order(l)`, `molden_pure_order(l)`,
`axial_norm_ratio(a, b, c)`, `primitive_norm(alpha, l)`, `contraction_overlap(shell)`,
`shell_from_molden` / `shell_to_molden` (one shell's block of one MO) and `mo_from_molden` /
`mo_to_molden` (one MO over a basis). Tests use hand-built shells: permutations component by
component, normalization ratios and the contraction overlap against numerical quadrature, a
one-hot d_xy, an SP shell with different s and p coefficients, a two-primitive contraction,
and round trips for every l and both conventions. The PySCF cc-pVQZ water fixtures validate
pure d/f/g order and signs and Cartesian d/f/g order and `Unit` normalization against signed
grid values. The two mixed water fixtures validate per-l flags by independent per-shell
evaluation. PySCF's loader uses one Cartesian flag for the whole file.

### `Vibrations`

| Field                   | Meaning                                                                                                                                                                                                                                                                                                                                  |
| ----------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `frequencies_cm1`       | Harmonic frequencies in ascending order; imaginary ones negative. Rigid-body modes removed.                                                                                                                                                                                                                                              |
| `ir_intensities_km_mol` | Empty or one per mode.                                                                                                                                                                                                                                                                                                                   |
| `modes`                 | Empty, or one mode per frequency, flat (modes × 3N, atom-major): Cartesian displacements, **not mass-weighted**, with unit Euclidean norm as produced. Consumers renormalize before use; validation checks the [−1, 1] component range and that every mode keeps a non-zero component after 16-bit saving. The sign is arbitrary. Heavy. |
| `reduced_masses_amu`    | Empty or one per mode. Heavy.                                                                                                                                                                                                                                                                                                            |
| `masses_amu`            | Empty, or the masses the analysis used (needed to return to mass-weighted coordinates). Heavy.                                                                                                                                                                                                                                           |

`imaginary_count()` counts negative frequencies, however small; the warning threshold for
near-zero negative modes belongs to the user interface.

### `Thermo`

`temperature_k`, `pressure_pa`, `zero_point_energy_hartree`, `enthalpy_correction_hartree`
(H(T) − E, including the ZPE), `entropy_hartree_per_k`, `gibbs_correction_hartree`
((H − E) − T·S), optional `heat_capacity_hartree_per_k`, and the analysis settings
`frequency_scale`, `rotor_cutoff_cm1`, `imaginary_cutoff_cm1`, `symmetry_number`. E is the
result's `energy_hartree` as the program reports it (for xTB at finite electronic temperature,
including the electronic entropy term); the total free energy is E plus the Gibbs correction.

### `CalcResult`

One calculation or one imported job step, bound to the frame it describes.

| Field                              | Meaning                                                                                                                                                                                                                                      |
| ---------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `frame`                            | `FrameHash` of the geometry the values belong to (for an optimization, the final geometry).                                                                                                                                                  |
| `input_frame`                      | `FrameHash` the calculation started from, when it differs from `frame`.                                                                                                                                                                      |
| `task`                             | `Energy`, `Gradient`, `Optimize`, `Hessian`, `Conformers` or `TransitionState`.                                                                                                                                                              |
| `endpoints`                        | For a transition state: `Endpoints { reactant, product }`, the `FrameHash`es of the optimized endpoints.                                                                                                                                     |
| `settings`                         | `Settings::Xtb(XtbSettings)` or `Settings::Imported { keywords }`.                                                                                                                                                                           |
| `provenance`                       | `Provenance`.                                                                                                                                                                                                                                |
| `status`                           | `Converged`, `NotConverged { reason }` or `Failed { reason }`.                                                                                                                                                                               |
| `energy_hartree`, `energy_terms`   | Total energy and named components (`BTreeMap<String, f64>`, Eh), for example the parts of a solvent composite.                                                                                                                               |
| `gradient_hartree_per_bohr`        | Empty or one vector per atom. Heavy.                                                                                                                                                                                                         |
| `hessian_hartree_per_bohr2`        | Empty or (3N)² values, row-major, atom-major axes, not mass-weighted. Heavy.                                                                                                                                                                 |
| `charges`                          | `Vec<AtomicCharges { scheme, values }>`; scheme `Mulliken`, `Eeq`, `Cm5`, `Hirshfeld`, `Loewdin` or `Other(String)`.                                                                                                                         |
| `dipole_e_bohr`                    | Optional, about the origin of the frame.                                                                                                                                                                                                     |
| `bond_orders`                      | `Vec<BondOrder { a, b, order }>` (Wiberg–Mayer), sparse, `a < b`, no repeated pair.                                                                                                                                                          |
| `orbitals`, `vibrations`, `thermo` | Optional.                                                                                                                                                                                                                                    |
| `trajectory`                       | Optional. Optimization: the path, first step `input_frame`, last step `frame`. Conformers: the kept conformers in rank order, first step `frame` (the best). Transition state: the path images from reactant to product with their energies. |
| `keep_full`                        | The user's "Keep full results in this file" choice; `false` by default.                                                                                                                                                                      |
| `omitted_bytes`                    | Set on a saved copy that left out heavy arrays which cannot be recomputed: their encoded size, for the palette.                                                                                                                              |

`CalcResult::validate(&frame)` checks the hash, every array length against the frame and the
basis, orbital consistency and limits.

### `XtbSettings` and `Provenance`

`XtbSettings { method, conventions, solvent, electronic_temperature_k, accuracy, max_iterations, scf_retries, gfnff }`.
Conventions default to `axtb`, omitted overrides use the defaults pinned by the engine revision, and `scf_retries` lists temperature or iteration overrides tried after a failed SCF.

- `method`: `XtbMethod::{Gfn2Xtb, GfnFf, GXtb}`. `GXtb` is g-xTB 2.0.1.
- `solvent`: `Solvent { model, name, state, correction }`, with `model: SolventModel::{Alpb, Gbsa}` and
  `state: SolvationState::{Gsolv, Bar1mol, Reference}` (default `Gsolv`). For g-xTB the
  correction must be `Some(SolventCorrection::Gfn2)`: the composite
  E = E(g-xTB) + [E(GFN2-xTB + solvent) − E(GFN2-xTB)], named explicitly so a solvent never
  silently means it; a given `electronic_temperature_k` applies to all three parts. GFN2-xTB
  takes no correction.
- GFN-FF takes no solvent or SCF settings and accepts only singlets.
  `gfnff` is required for GFN-FF and absent otherwise:
  `GfnFfTopology { reference: FrameHash, fragment_charges: Vec<i32> }`, the frame the topology
  was set up from and one charge per molecular fragment in the order of the fragments' first
  atoms (empty: the whole charge on the first fragment). The fragments follow from the
  reference frame's topology, so the reference and charges pin the energy surface for a
  restarted optimization or a later Hessian. A stored frame that settings reference is never
  removed while the result exists.

`Provenance { program, program_version, method, basis, source }` with
`Source::Calculated { engine_revision }` or
`Source::Imported { format, file_name, file_sha256, job }` (`job` is the job or step index in a
multi-job log; the file name and hash say which file to reopen when heavy arrays were not
saved). `Settings::Xtb` requires `Source::Calculated` provenance with a non-empty
`engine_revision` so the result is recomputable.

A result belongs to a frame exactly when `result.frame == frame.hash()`. Two results are
comparable when their `settings` and engine revision are equal.

## File size

Native `.rsk` files are pretty-printed, uncompressed JSON, one number per line: the normal modes
of a 200-atom molecule would take 8.2 MB that way. The owner's policy is binding for every lane.

| Data                                                                                                                                                                                                                                                                                                                                                                                         | Saved                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Settings, provenance (method, engine version), status, energies and terms, charges, dipole, bond orders of bonded pairs, orbital energies and occupations, core electrons, frequencies and IR intensities, thermochemistry, trajectory energies, labels, minimizer energies and cluster sizes, transition-state endpoints; the positions of kept conformers (at most `MAX_SAVED_CONFORMERS`) | Always ("light result"). Target ≤ 50 KB at 50 atoms, ≤ 200 KB at 200 atoms. The input and final frames are stored as frames of the document.                                                                                                                                                                                                                                                                                          |
| Normal modes, reduced and analysis masses, orbital basis and coefficients, Hessian, gradient, trajectory positions ("heavy")                                                                                                                                                                                                                                                                 | Not by default: properties at a saved geometry are recomputed on demand from the frame and settings. Rerunning a conformer search or a path search needs that search's own settings (minimizer, seeds, clustering, images), which the Phase 3 and Phase 6 work adds to `Settings` before relying on it. Kept when `keep_full` is set, or for imported results within the budget below. Rows that placed surfaces use are always kept. |
| Grids, meshes, animation frames                                                                                                                                                                                                                                                                                                                                                              | Never.                                                                                                                                                                                                                                                                                                                                                                                                                                |

- **Compact arrays.** Heavy arrays never serialize as JSON number lists. Each is one JSON string:
  a tag, a colon and base64 (standard alphabet, padded) of raw deflate (`miniz_oxide`) of
  little-endian values, byte-shuffled (all first bytes, then all second bytes, and so on) so
  the exponent bytes compress together. An empty array serializes as an empty string. Two
  encodings, chosen by the field:
  - `f32:` for Hessians, gradients, orbital coefficients, masses, trajectory positions. Values
    keep about 7 significant digits.
  - `q16:` for normal modes, whose components lie in [−1, 1]: `round(x · 32767)` as `i16`
    (absolute precision 3e-5). The worst case for a 200-atom molecule, 594 × 600 values with no
    compression, is 950,400 base64 bytes; f32 would be about 1.9 MB, and deflate gains only
    10–15 % on such data.
  - Decoding rejects an unknown tag, an odd byte count, more than `MAX_COMPACT_VALUES` values,
    and a decoded size above `MAX_COMPACT_EXPANSION` (32) times the encoded string, so a small
    file cannot inflate into a large allocation. The encoder stores data uncompressed when
    deflate would exceed that ratio, so every saved array reloads. `q16` decoding rejects −32768. Encoding a `q16` value outside [−1, 1] is an
    error.
- **Saved form.** `CalcResult::for_file(heavy_budget, pinned)` returns what a document writes:
  the result itself when `keep_full` is set; the light result when it is recomputable
  (`Settings::Xtb`); for an imported result, the result itself when `heavy_bytes()` ≤
  `heavy_budget`, else the light result with `omitted_bytes` increased by the bytes this save
  drops; the full result is kept instead when the light form, counter included, would not be
  smaller. Pinned rows are kept even when the budget is exceeded. `pinned` lists
  `(Spin, row)` orbitals that placed surfaces use: the light result keeps their rows and the
  basis. The document passes `MAX_IMPORTED_HEAVY_BYTES` (2 MB) minus what the molecule's
  other imported results already keep. The light result keeps bond orders of at least
  `LIGHT_BOND_ORDER_MIN` (0.3); the document may filter by drawn bonds instead.
- **Size estimates.** `heavy_bytes()` is what keeping the heavy data adds to the file: the
  pretty JSON length of the full result minus that of its light form ("Keep full results in
  this file (+1.4 MB)"); `saved_bytes(heavy_budget, pinned)` is the length of the pretty JSON
  of the saved form ("Saved in file: 84 KB").
- **Recomputing** gives a new result. Recomputed orbitals may differ in sign or within a
  degenerate set, which is why placed surfaces keep their own rows.

## Method × task capabilities

`XtbMethod::capabilities()` returns this table as data, from axtb's documented behaviour; the
worker and the user interface both read it, and `check_calculation` enforces it.

| Method   | Energy | Gradient | Optimize | Hessian        | Conformers | Transition state | Orbitals | Bond orders | ALPB solvent              | Open shells   |
| -------- | ------ | -------- | -------- | -------------- | ---------- | ---------------- | -------- | ----------- | ------------------------- | ------------- |
| GFN2-xTB | yes    | yes      | yes      | yes (analytic) | yes        | yes              | yes      | yes         | native                    | yes           |
| GFN-FF   | yes    | yes      | yes      | yes (analytic) | yes        | no               | no       | no          | no                        | no (singlets) |
| g-xTB    | yes    | yes      | yes      | yes (analytic) | yes        | no               | yes      | yes         | GFN2 correction composite | yes           |

Transition-state searches use GFN2-xTB gradients and Hessians, as Phase 6 plans; the other
methods can be added when that pipeline is validated with them.

g-xTB's orbitals, charges and dipole in solvent are those of its g-xTB part.

`check_calculation(task, &settings, &frame, orbitals: OrbitalRequest)` validates the frame, the size limit of
the task, the solvent rule, the SCF settings rule, open-shell support, orbital support and the
GFN-FF topology. Energy and optimization are admitted independently of orbitals: `orbitals` is
`OrbitalRequest::None`, `Levels` (energies and occupations only) or `Rows { n_ao, rows }`
(coefficients of `rows` MOs over `n_ao` AOs), and `Rows` is refused when `n_ao` is zero, above
`MAX_BASIS_FUNCTIONS`, or `n_ao × rows` exceeds `MAX_COEFFICIENTS`; the worker computes the AO
count before it allocates. Every boundary that admits a
calculation (worker request, CLI, MCP, Python, the Calculate panel) calls it; the worker calls
it again on its side.

## Limits

| Constant                   | Value     | Applies to                                                                                                         |
| -------------------------- | --------- | ------------------------------------------------------------------------------------------------------------------ |
| `MAX_CALC_ATOMS`           | 500       | Energy, gradient and optimization.                                                                                 |
| `MAX_HESSIAN_ATOMS`        | 200       | Hessians and vibrational analysis; conformer re-ranking with xTB.                                                  |
| `MAX_TS_ATOMS`             | 120       | Transition-state searches.                                                                                         |
| `MAX_SAVED_CONFORMERS`     | 32        | Conformers a conformer result keeps.                                                                               |
| `MAX_FRAMES`               | 1000      | Steps in a trajectory, and frames read from one file.                                                              |
| `MAX_FRAME_ATOMS`          | 10,000    | Any stored frame (imported files larger than this keep their drawing and get no scientific frame, with a warning). |
| `MAX_TRAJECTORY_ATOMS`     | 1,000,000 | Atoms summed over a trajectory's steps.                                                                            |
| `MAX_BASIS_FUNCTIONS`      | 10,000    | AOs in a basis.                                                                                                    |
| `MAX_COEFFICIENTS`         | 4,194,304 | Stored MO coefficients per spin channel (16 MiB as f32).                                                           |
| `MAX_PRIMITIVES`           | 64        | Primitives per shell.                                                                                              |
| `MAX_L`                    | 5         | Angular momentum.                                                                                                  |
| `MAX_COMPACT_VALUES`       | 4,194,304 | Values in one compact array.                                                                                       |
| `MAX_COMPACT_EXPANSION`    | 32        | Decoded bytes per encoded byte of a compact array.                                                                 |
| `MAX_IMPORTED_HEAVY_BYTES` | 2,000,000 | Encoded heavy arrays saved per molecule for imported results.                                                      |
| `LIGHT_BOND_ORDER_MIN`     | 0.3       | Smallest bond order a light result keeps.                                                                          |

`validate` enforces them after parsing; compact arrays enforce theirs while decoding.

## Documents

Implemented in `crates/model/src/science.rs` (Phase 2a, part 2). The user interface, the worker
and the agent API call the functions named here.

- **Storage apart from the drawing.** `Document::science: Science` (absent from the file when
  empty) holds `frames: Vec<Arc<Frame>>`, `attachments: Vec<Attachment>` and
  `results: Vec<Stored>`, where `Stored { attachment, keep_full, result: Arc<CalcResult> }` names
  the one attachment a result belongs to (the attachment of its input frame, or of its own frame
  when it has none). Frames and results are immutable; "Keep full results" is the `Stored` flag,
  so toggling it never copies the arrays. A frame is stored once per hash.
- **Attachments, not frame hashes, link to the drawing.** Two copies of one geometry hash alike,
  so the hash cannot say which molecule a frame belongs to. An
  `Attachment { id, frame: FrameHash, atoms: Vec<Option<u64>>, baseline: String }` maps each frame
  atom to its drawing atom (`None` for hydrogens the drawing does not show) and keeps the
  chemistry fingerprint of the mapped atoms when the frame was made: the same digest that marks
  3D models out of date (element, charge, isotope, radicals, hydrogens, bonds and perceived
  stereo, in frame order, so the atom count counts too), plus each atom's singlet choice, which
  sets the spin, and its enhanced-stereo group, which the digest leaves out. Attachment IDs are unique among the
  document's attachments; they are not drawing object IDs, so selection, hit testing and ID
  remapping of drawing objects never meet them; a new one takes the smallest free ID. Two
  molecules with identical geometries keep their own results. `Model3d` is unchanged: applying a geometry writes the model's
  `f32` display coordinates, and its own fingerprint keeps deciding when the model is out of date.
- **Starting a calculation.** `science::launch(doc, atom, charge, multiplicity)` builds the input
  frame from the molecule's 3D model (model atoms in model order, then the model's added
  hydrogens) and returns a `Launch { frame, atoms, baseline }` without changing the document.
  `drawn_state(doc, atom)` gives the charge and multiplicity the drawing implies. When the model
  still shows an attached frame exactly (an applied optimization), the launch reuses that `f64`
  frame, so chained calculations keep full precision. A missing, out-of-date or periodic model, or one that also
  holds other molecules, is an error.
- **Staleness.** `Attachment::freshness(doc)` is `Current` when the mapped atoms still exist, form
  one whole molecule with no other atoms, and match the baseline; otherwise `Stale`. Every
  transaction runs `science::prune`: deleted atoms leave their map entries in the same edit, so a
  reused ID never links again, and attachments without drawing atoms go, with the results and
  frames nothing else needs. Undo restores the snapshot, and with it the fresh state.
- **Storing a result.** `science::attach(doc, &launch, new_frames, result)` stores the launch
  frame, the result's new frames, the result and the launch frame's attachment as one edit, after
  validating everything; the result must start from the launch frame and describe the same
  atoms in the same order (a transition state excepted). `results_for(doc, atom)` lists the
  results of every attachment still linked to the atom's current molecule. It returns
  `Attached { freshness, attachment }`: the launch's baseline is the revision the reply is checked
  against, so a reply for a molecule edited since launch is stored `Stale` (greyed, with
  Recalculate) and becomes current if Undo returns to the launch state; a reply for a deleted
  molecule is refused (`Gone`). The caller commits the edit to history; the agent API commits it
  through its revision-checked store commit.
- **Applying a geometry.** `science::apply_geometry(doc, attachment, &frame)` moves the molecule's
  3D model to a frame stored for that attachment, such as an optimization's final frame ("Apply
  xTB geometry", one undo step for the caller); the 2D drawing does not move. It refuses a stale
  attachment, a model whose atoms or hydrogen order no longer match the frame (each added hydrogen
  must sit nearest its own parent), and coordinates outside the model's range, before changing
  anything.
  Before applying, `science::stereo_changes(doc, &atoms, &frame)` lists drawn stereocentres the
  frame inverts or flattens and drawn E/Z double bonds it flips, comparing the drawing's perceived
  configurations with the configurations perceived from the frame's coordinates on the same graph.
- **Native version.** `CALCULATIONS` (42) is the floor for any document whose `science` is
  non-empty; `Document::file_version` derives it, and `Document::current()`, which files and
  recovery drafts go through, also turns every result into its saved form. Internal round trips
  that serialize a document without `current()` (engine jobs such as cleanup) keep the full data. Older
  ReShiki reads the version before parsing and asks the user to update, so it cannot open such a
  document and save it without the data. Documents without science keep their version; the floor
  drops when the last attachment goes.
- **Admission.** `Science` deserializes through one check used by every reading route (files,
  recovery, inserted drawings): records are admitted one at a time and decoding stops as soon as
  they pass `MAX_SCIENCE_BYTES` (256 MiB, every stored result counted as a file stores it) or
  `MAX_SCIENCE_RECORDS` (10,000); compact-array limits while decoding, `validate` for
  every frame and every result against its frame, every referenced frame present (input frame,
  endpoints, GFN-FF topology reference), attachments matching their frame's atom count with
  distinct atoms, no repeated frame, each result matching its attachment, unknown fields refused.
  `Document::validate` applies the same budget, so an insertion that would pass it is refused.
- **Undo history.** Snapshots clone the `Arc`s, never the payloads. `History` counts each distinct
  payload's `heap_bytes()` once while any snapshot holds it (by address) and releases it with the
  last holder; each snapshot counts its own lists and attachments. `History::bytes()` includes
  the shared payloads, so the editor's per-tab budget sees them. `History::science_bytes(live)`
  covers the live document and its history together, and every commit calls
  `History::fit_science(live)`, which drops the oldest steps, below the minimum undo depth if
  needed, until they fit `MAX_SCIENCE_BYTES`.
- **Saved size.** Each result is written in its saved form (`CalcResult::for_file`): imported
  results keep heavy arrays while their molecule's imported data stays within
  `MAX_IMPORTED_HEAVY_BYTES`, in arrival order. `Science::saved_sizes()` gives every result's
  "Saved in file: 84 KB" in one pass and `Science::file_bytes()` the document total (both
  re-encode large imported arrays, so the interface computes them off the UI thread); Save offers "Make lighter"
  above `LIGHTER_BYTES` (5 MB). `science::make_lighter(doc)` turns every recomputable (xTB) result
  that keeps full data into its light form; imported results, which cannot be recomputed, are
  left alone. `science::set_keep_full(doc, index, keep)` is the per-result toggle.
- **Generated data.** Grids, isosurface meshes and simplified meshes are never saved and never
  enter history: they are an editor-side cache keyed by result, spin, MO row, isovalue and grid
  settings. A surface placed in the drawing saves its request (result, spin, MO row, isovalue,
  colours), pins that MO's row and the basis in the saved result, and saves a simplified
  projected outline (a compact array, at most 300 KB) so it prints and exports in builds
  without xTB. Its 3D surface is rebuilt from the pinned row. (Phase 5.)
- **Clipboard, figures and templates.** Copies carry no calculation data: `editing::selection`
  and the whole-drawing copy drop `science`, the editable drawing embedded in PNG, SVG and PDF
  figures drops it, and templates store none. Inserting a native file that has results remaps the
  attachments' atom IDs with the drawing and keeps the results. Any other ID remapping that loses
  an atom (a join moving a fragment) unlinks it as a delete would.

## Not in this crate

Readers and writers, the worker protocol, orbital grid evaluation, the optimizer and document
integration are other pull requests. They depend on this crate; this crate depends on none of
them.
