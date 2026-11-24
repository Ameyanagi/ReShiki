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

A display path of one molecule: an optimization, a scan, a multi-frame file.
`Trajectory { numbers, isotopes, masses_amu, charge, multiplicity, positions_angstrom, energies_hartree, labels }`.
The atoms are stored once; `positions_angstrom` is flat (steps × atoms × 3) and is a heavy
array saved as `f32` ([File size](#file-size)), or empty when the steps were not saved;
`energies_hartree` and `labels` are empty or one per step, and decide the step count when the
positions are absent. `Trajectory::frame(i)` builds the frame of step `i`.

Result identity never depends on a trajectory: the input and final frames of an optimization
are stored exactly as frames of their own. A frame built from a saved step carries `f32`
precision (about 1e-5 Å) and gets its own hash; "Use this geometry" makes such a new frame.

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

Contraction coefficients already follow rule 7. Molden chooses pure or Cartesian once per
angular momentum for the whole file and has no h shells, so writing a basis that mixes
representations within one `l`, or has `l = 5`, is an error. `[STO]` bases are unsupported.

The specification does not say how Cartesian components are normalized, and producers differ.
The converters take `CartesianNorm`:

- `Unit`: each Molden component is a unit-norm AO, as in the canonical form.
- `Axial`: every component shares one normalization constant, the one that gives the x^l
  component unit norm. Component x^a y^b z^c then has squared norm
  r(a, b, c) = (2a − 1)!! (2b − 1)!! (2c − 1)!! / (2l − 1)!!, so importing multiplies the MO
  coefficient by √r and exporting divides by it. A d_xy coefficient of 1 becomes 1/√3.

Each reader names the convention of its identified producer and proves it on a fixture.
Producer-specific sign changes of pure functions belong to the reader's adapter.

Functions: `canonical_cartesian_order(l)`, `molden_cartesian_order(l)`, `molden_pure_order(l)`,
`axial_norm_ratio(a, b, c)`, `primitive_norm(alpha, l)`, `contraction_overlap(shell)`,
`shell_from_molden` / `shell_to_molden` (one shell's block of one MO) and `mo_from_molden` /
`mo_to_molden` (one MO over a basis). Tests use hand-built shells: permutations component by
component, normalization ratios and the contraction overlap against numerical quadrature, a
one-hot d_xy, an SP shell with different s and p coefficients, a two-primitive contraction,
and round trips for every l and both conventions. Phase 5 validates signed grid values with
independent per-shell evaluations, since PySCF's loader uses one Cartesian flag for the whole
file.

### `Vibrations`

| Field                   | Meaning                                                                                                                                                                                                                      |
| ----------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `frequencies_cm1`       | Harmonic frequencies in ascending order; imaginary ones negative. Rigid-body modes removed.                                                                                                                                  |
| `ir_intensities_km_mol` | Empty or one per mode.                                                                                                                                                                                                       |
| `modes`                 | Empty, or one mode per frequency, flat (modes × 3N, atom-major): Cartesian displacements, **not mass-weighted**, unit Euclidean norm over all 3N components (within 1e-3 after 16-bit saving). The sign is arbitrary. Heavy. |
| `reduced_masses_amu`    | Empty or one per mode. Heavy.                                                                                                                                                                                                |
| `masses_amu`            | Empty, or the masses the analysis used (needed to return to mass-weighted coordinates). Heavy.                                                                                                                               |

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

| Field                              | Meaning                                                                                                              |
| ---------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `frame`                            | `FrameHash` of the geometry the values belong to (for an optimization, the final geometry).                          |
| `input_frame`                      | `FrameHash` the calculation started from, when it differs from `frame`.                                              |
| `task`                             | `Energy`, `Gradient`, `Optimize` or `Hessian`.                                                                       |
| `settings`                         | `Settings::Xtb(XtbSettings)` or `Settings::Imported { keywords }`.                                                   |
| `provenance`                       | `Provenance`.                                                                                                        |
| `status`                           | `Converged`, `NotConverged { reason }` or `Failed { reason }`.                                                       |
| `energy_hartree`, `energy_terms`   | Total energy and named components (`BTreeMap<String, f64>`, Eh), for example the parts of a solvent composite.       |
| `gradient_hartree_per_bohr`        | Empty or one vector per atom. Heavy.                                                                                 |
| `hessian_hartree_per_bohr2`        | Empty or (3N)² values, row-major, atom-major axes, not mass-weighted. Heavy.                                         |
| `charges`                          | `Vec<AtomicCharges { scheme, values }>`; scheme `Mulliken`, `Eeq`, `Cm5`, `Hirshfeld`, `Loewdin` or `Other(String)`. |
| `dipole_e_bohr`                    | Optional, about the origin of the frame.                                                                             |
| `bond_orders`                      | `Vec<BondOrder { a, b, order }>` (Wiberg–Mayer), sparse, `a < b`, no repeated pair.                                  |
| `orbitals`, `vibrations`, `thermo` | Optional.                                                                                                            |
| `trajectory`                       | Optional path; for an optimization its first step is `input_frame` and its last is `frame`.                          |
| `keep_full`                        | The user's "Keep full results in this file" choice; `false` by default.                                              |
| `omitted_bytes`                    | Set on a saved copy that left out heavy arrays which cannot be recomputed: their encoded size, for the palette.      |

`CalcResult::validate(&frame)` checks the hash, every array length against the frame and the
basis, orbital consistency and limits.

### `XtbSettings` and `Provenance`

`XtbSettings { method, solvent, electronic_temperature_k, accuracy, max_iterations, gfnff }`;
`None` means the engine's default for the method, and the engine revision in the provenance
pins what that default is.

- `method`: `XtbMethod::{Gfn2Xtb, GfnFf, GXtb}`. `GXtb` is g-xTB 2.0.1.
- `solvent`: `Solvent { model: SolventModel::Alpb, name, state, correction }`, with
  `state: SolvationState::{Gsolv, Bar1mol, Reference}` (default `Gsolv`). For g-xTB the
  correction must be `Some(SolventCorrection::Gfn2)`: the composite
  E = E(g-xTB) + [E(GFN2-xTB + ALPB) − E(GFN2-xTB)], named explicitly so a solvent never
  silently means it; a given `electronic_temperature_k` applies to all three parts. GFN2-xTB
  takes no correction.
- GFN-FF takes no solvent, no electronic temperature, accuracy or iteration count (it has no
  SCF), and only singlets. `gfnff` is required for GFN-FF and absent otherwise:
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
saved).

A result belongs to a frame exactly when `result.frame == frame.hash()`. Two results are
comparable when their `settings` and engine revision are equal.

## File size

Native `.rsk` files are pretty-printed, uncompressed JSON, one number per line: the normal modes
of a 200-atom molecule would take 8.2 MB that way. The owner's policy is binding for every lane.

| Data                                                                                                                                                                                                                                                       | Saved                                                                                                                                                                                                 |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Settings, provenance (method, engine version), status, energies and terms, charges, dipole, bond orders of bonded pairs, orbital energies and occupations, core electrons, frequencies and IR intensities, thermochemistry, trajectory energies and labels | Always ("light result"). Target ≤ 50 KB at 50 atoms, ≤ 200 KB at 200 atoms. The input and final frames are stored as frames of the document.                                                          |
| Normal modes, reduced and analysis masses, orbital basis and coefficients, Hessian, gradient, trajectory positions ("heavy")                                                                                                                               | Not by default: recomputed on demand from the saved frame and settings. Kept when `keep_full` is set, or for imported results within the budget below. Rows that placed surfaces use are always kept. |
| Grids, meshes, animation frames                                                                                                                                                                                                                            | Never.                                                                                                                                                                                                |

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
    file cannot inflate into a large allocation. Encoding a `q16` value outside [−1, 1] is an
    error.
- **Saved form.** `CalcResult::for_file(heavy_budget, pinned)` returns what a document writes:
  the result itself when `keep_full` is set; the light result when it is recomputable
  (`Settings::Xtb`); for an imported result, the result itself when `heavy_bytes()` ≤
  `heavy_budget`, else the light result with `omitted_bytes` set. `pinned` lists
  `(Spin, row)` orbitals that placed surfaces use: the light result keeps their rows and the
  basis. The document passes `MAX_IMPORTED_HEAVY_BYTES` (2 MB) minus what the molecule's
  other imported results already keep. The light result keeps bond orders of at least
  `LIGHT_BOND_ORDER_MIN` (0.3); the document may filter by drawn bonds instead.
- **Size estimates.** `heavy_bytes()` is the encoded size of the heavy arrays ("Keep full
  results in this file (+1.4 MB)"); `saved_bytes(heavy_budget, pinned)` is the length of the
  pretty JSON of the saved form ("Saved in file: 84 KB").
- **Recomputing** gives a new result. Recomputed orbitals may differ in sign or within a
  degenerate set, which is why placed surfaces keep their own rows.

## Method × task capabilities

`XtbMethod::capabilities()` returns this table as data, from axtb's documented behaviour; the
worker and the user interface both read it, and `check_calculation` enforces it.

| Method   | Energy | Gradient | Optimize | Hessian        | Orbitals | Bond orders | ALPB solvent              | Open shells   |
| -------- | ------ | -------- | -------- | -------------- | -------- | ----------- | ------------------------- | ------------- |
| GFN2-xTB | yes    | yes      | yes      | yes (analytic) | yes      | yes         | native                    | yes           |
| GFN-FF   | yes    | yes      | yes      | yes (analytic) | no       | no          | no                        | no (singlets) |
| g-xTB    | yes    | yes      | yes      | yes (analytic) | yes      | yes         | GFN2 correction composite | yes           |

g-xTB's orbitals, charges and dipole in solvent are those of its g-xTB part.

`check_calculation(task, &settings, &frame, orbitals: OrbitalRequest)` validates the frame, the size limit of
the task, the solvent rule, the SCF settings rule, open-shell support, orbital support and the
GFN-FF topology. Energy and optimization are admitted independently of orbitals: `orbitals` is
`OrbitalRequest::None`, `Levels` (energies and occupations only) or `Rows(n)` (coefficients of n
MOs), and `Rows` is refused when the number of AOs times n would exceed `MAX_COEFFICIENTS`;
the worker computes the AO count before it allocates. Every boundary that admits a
calculation (worker request, CLI, MCP, Python, the Calculate panel) calls it; the worker calls
it again on its side.

## Limits

| Constant                   | Value     | Applies to                                                                                                         |
| -------------------------- | --------- | ------------------------------------------------------------------------------------------------------------------ |
| `MAX_CALC_ATOMS`           | 500       | Energy, gradient and optimization.                                                                                 |
| `MAX_HESSIAN_ATOMS`        | 200       | Hessians and vibrational analysis.                                                                                 |
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

Owned by Phase 2a; implemented in `reshiki-model` in a follow-up pull request.

- **Storage apart from the drawing.** `Document` gains `science: Science` (skipped when empty)
  with `frames: Vec<Arc<Frame>>`, `results: Vec<Arc<CalcResult>>` and
  `attachments: Vec<Attachment>`. Frames and results are immutable; an edit or an optimization
  adds a new frame.
- **Attachments, not frame hashes, link to the drawing.** Two copies of one geometry hash
  alike, so the hash cannot say which molecule a frame belongs to. An
  `Attachment { id, frame: FrameHash, atoms: Vec<Option<u64>>, baseline: String }` has its own
  document object ID (from the shared ID counter), its own live map from frame atoms to drawing
  atoms, and the drawing fingerprint of the mapped atoms when it was made (element, charge,
  isotope, bonds, and the molecule's full membership, so an added atom also counts). Results
  are shown through attachments; `Model3d` gains an optional attachment ID to display a
  scientific frame, and its own `f32` atoms stay the display copy.
- **Staleness.** An attachment is current when every mapped atom still exists, the molecule has
  no other atoms, and the fingerprint matches its baseline. Deleting a mapped atom clears that
  entry in the same edit, so a reused ID never re-links. A stale attachment's results stay
  readable and greyed ("The molecule changed after this calculation"). A worker reply is
  committed only through the agent API's revision-checked commit, bound to the attachment,
  input frame hash, task and settings it was launched with; a reply after Undo or an edit is
  discarded.
- **Native version.** A new constant (the next free document version when this lands) is the
  floor for any document whose `science` is non-empty. One function stamps the version for
  every route that serializes a document with science: `file_json` (through `current()`),
  clipboard payloads (`payload::encode`), recovery drafts and templates. Older ReShiki reads the
  version before parsing and asks the user to update, so it cannot open such a document and
  save it without the data. Drawing-only representations (the clipboard's drawing flavours,
  images) strip `science` and every attachment reference instead. Documents without science
  keep their current version; the floor drops only when the last science field and reference
  are gone. Tests cover older-reader rejection through each route, legacy reads, save and
  reopen in a build without xTB, copy and paste, recovery, and removing the final result.
- **Admission.** Attach, native open, recovery, clipboard paste, worker replies and import share
  one admission check: bounded reads (native open reads at most `MAX_NATIVE_BYTES`, to be set
  with the integration), compact-array decoding limits, `validate` for every frame and result,
  and a document science budget.
- **Undo history.** History snapshots clone the `Arc`s, never the payloads. The history counts
  each distinct payload's `heap_bytes()` once while any snapshot or the live document holds it
  (tracked by pointer), and releases it only when the last holder goes; snapshot-owned
  containers are counted per snapshot. A hard science budget (256 MiB of `heap_bytes()`) covers
  the live document and the retained history together and is enforced at attach time, before
  the minimum undo depth applies; the agent API's quotas count the same bytes.
- **Saved size.** Each result is written as `for_file(budget, pinned)`
  ([File size](#file-size)). Save warns once when a document's results exceed 5 MB and offers
  "Make lighter", which clears `keep_full` everywhere.
- **Generated data.** Grids, isosurface meshes and simplified meshes are never saved and never
  enter history: they are an editor-side cache keyed by result, spin, MO row, isovalue and grid
  settings. A surface placed in the drawing saves its request (result, spin, MO row, isovalue,
  colours), pins that MO's row and the basis in the saved result, and saves a simplified
  projected outline (a compact array, at most 300 KB) so it prints and exports in builds
  without xTB. Its 3D surface is rebuilt from the pinned row.
- **Clipboard.** Copies never carry heavy arrays; the first integration carries no science
  through the clipboard at all.

## Not in this crate

Readers and writers, the worker protocol, orbital grid evaluation, the optimizer and document
integration are other pull requests. They depend on this crate; this crate depends on none of
them.
