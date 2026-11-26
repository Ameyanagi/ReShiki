# xTB worker protocol

xTB calculations run in a child process of the application executable,
`reshiki --xtb-worker`, started by the client in `crates/xtb-worker`. One child
holds one **session**: one molecule (atoms, charge, unpaired electrons, method,
settings), evaluated at one geometry after another so that each SCF can start
from the previous wavefunction. Licensing and packaging are in
[xTB licensing](xtb-licensing.md); the scientific data types, the canonical
atomic-orbital contract and the method × task table are in
[the quantum chemistry data design](plans/xtb-data.md) (`reshiki-qcdata`).

Protocol version 2 covers GFN2-xTB, GFN-FF and g-xTB 2.0.1: energies,
gradients, charges, multipole moments, bond orders, orbitals with their basis,
analytic Hessians with vibrational analysis and thermochemistry. The client and
the worker are always the same executable, so there is no version
negotiation: a frame of any other version is a protocol error.

## Frames

Every message in either direction is one frame: a 16-byte header and a body.

| Bytes | Meaning                                                    |
| ----- | ---------------------------------------------------------- |
| 0–7   | ASCII `RSHXTBWK`                                           |
| 8–9   | Little-endian protocol version, 2                          |
| 10–11 | Little-endian frame kind: 0 JSON message, 1 binary chunk   |
| 12–15 | Little-endian unsigned body length in bytes, at most 4 MiB |

A JSON body is one UTF-8 JSON object. Unknown JSON fields are errors, never
ignored. Requests are always JSON; only a `compute` response is followed by
binary chunks ([Large arrays](#large-arrays)).

## Requests and responses

The client sends one request and waits for its whole response before sending
the next (no pipelining). Each request has an `id`, a positive integer that
increases with every request; the response repeats it.

```json
{ "id": 1, "op": "hello" }
{ "id": 1, "ok": { "protocol": 2, "engine": { "name": "axtb", "version": "0.2.0",
  "license": "LGPL-3.0-or-later", "source": "https://github.com/Ameyanagi/axtb",
  "rev": "9d0388c9…" }, "methods": ["gfn2", "gfnff", "gxtb"],
  "max_atoms": 500, "max_hessian_atoms": 200 } }
```

| `op`      | Fields                                                                                                                        | `ok` result                                                                                  |
| --------- | ----------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| `hello`   | none                                                                                                                          | `protocol`, `engine`, `methods`, `max_atoms`, `max_hessian_atoms`                            |
| `open`    | `numbers`, `positions`, `units`, `charge`, `uhf`, `method`, `conventions`, `solvent`, `scf`, `gfnff`, `threads`, `heap_bytes` | `atoms`, `method`, `uhf`, `conventions`, `numerics`, `engine_conventions`, `basis_functions` |
| `compute` | `positions` (optional: the last geometry), `units`, `gradient`, `hessian`, `bond_orders`, `orbitals`                          | [`Computed`](#computed), then the announced binary chunks                                    |
| `close`   | none                                                                                                                          | `{}`; the worker then exits 0                                                                |

### Methods and what they can do

| `method` | Method                 | Notes                                                                                                                                               |
| -------- | ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gfn2`   | GFN2-xTB (the default) | Everything below; ALPB and GBSA solvents directly.                                                                                                  |
| `gfnff`  | GFN-FF                 | Energy, gradient, Hessian. No solvent, no SCF settings, no orbitals, no bond orders, singlets only (`uhf` 0 or absent). EEQ charges.                |
| `gxtb`   | g-xTB 2.0.1 only       | Everything GFN2-xTB has. A solvent needs `"correction": "gfn2"`: E = E(g-xTB) + [E(GFN2-xTB + solvent) − E(GFN2-xTB)]. g-xTB v1.1 is never offered. |

The table is `reshiki_qcdata::XtbMethod::capabilities()`. Both sides enforce
it with `reshiki_qcdata::check_calculation`: the client before it sends a
request, the worker again on `open` (as an energy) and on every `compute` (as
an energy, gradient or Hessian, with the requested orbital rows). An output
the method cannot provide returns `unsupported` with qcdata's message, such as
`GFN-FF does not provide orbitals`. Invalid settings return `input`. Refused
computes leave the session open at its last successful geometry.

### `open`

- `numbers`: atomic numbers, 1 to 500 atoms. `positions`: one `[x, y, z]` per
  atom, finite. `units`: `"angstrom"` or `"bohr"`, always explicit. Ångström
  is converted with the factor axtb's XYZ reader uses for the selected method
  and `conventions`, so a geometry read from an XYZ file gives the axtb CLI's
  numbers bit for bit. The same conversion applies to every other position in
  the session (`gfnff.reference_positions`, replacement geometries).
- `conventions`: `"axtb"` (default) or `"xtb-6.8"` (xtb 6.8 compatible). The
  worker passes the axtb driver these conventions: GFN2-xTB `axtb` / `xtb`,
  GFN-FF `axtb` / `xtb`, g-xTB `axtb` / `gxtb` (the g-xTB 2.0.1 program's).
  `engine_conventions` in every response is the driver's canonical name.
- `charge` is an integer and defaults to 0. For SCF methods, the worker
  resolves an omitted `uhf` (unpaired electrons) from the method's valence
  electron count. It returns `input` if that count is negative or if an explicit
  `uhf` exceeds it or differs in parity. Admission uses the all-electron parity
  to validate the qcdata geometry. The `open` response reports the resolved
  `uhf`, and every `Computed` echoes it in `settings.uhf`.
  qcdata cannot yet store a result when the resolved valence `uhf + 1` differs
  from the frame multiplicity.
- `solvent`: `null` or `{ "model": "alpb" | "gbsa", "name": "<solvent>",
"state": "gsolv" | "bar1mol" | "reference", "correction": null | "gfn2" }`.
  `state` defaults to `gsolv`. The name is any solvent axtb accepts for the
  model; an unknown name returns `input`. `correction` is required for g-xTB
  and refused otherwise.
- `scf` (optional; SCF methods only): `{ "accuracy": x, "electronic_temperature_k": T,
"max_iterations": n, "retries": [{ "electronic_temperature_k": T, "max_iterations": n }] }`.
  Every field is optional; a missing one is the method's default (GFN2-xTB
  300 K, g-xTB 0 K; 250 iterations; accuracy 1). `retries` is an SCF retry
  ladder: after an SCF that does not converge, a fresh SCF at each rung in
  order until one converges, for example `[{ "electronic_temperature_k": 1000 }]`
  ("retry at 1000 K"). No rung may go below an explicit `electronic_temperature_k`. For the
  g-xTB solvent composite the ladder applies to each part.
- `gfnff` (optional; GFN-FF only): `{ "reference_positions": [[x, y, z], …],
"fragment_charges": [q, …] }`, both optional. The topology is set up from
  `reference_positions` (same `units`; default `positions`) and the integer
  fragment charges, one per fragment in the order of the fragments' first
  atoms, summing to `charge` (default: the whole charge on the first fragment).
  Every `Computed` of a GFN-FF session reports the reference and the fragment
  charges it used, so a later session (a restarted optimization, a Hessian)
  can pass them back and evaluate the same energy surface (review finding 12).
- `threads`: 1 to 64, default 1. `heap_bytes`: the worker's allocator budget.
- `basis_functions` in the result: the number of atomic orbitals of the
  method's basis for these atoms (`null` for GFN-FF). The client uses it to
  admit orbital rows before it asks for them.
- `open` is allowed once per process; `compute` only after a successful open.
  A failed `compute` (for example an SCF that did not converge, or a refused
  task) keeps the session open at the last successful geometry.

### `compute`

| Field         | Values                                                                                                                                                                                                          |
| ------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `positions`   | Optional replacement geometry, one finite row per atom, in `units`.                                                                                                                                             |
| `gradient`    | `true` adds the gradient (Eh/bohr).                                                                                                                                                                             |
| `hessian`     | `null` (default) or an object with optional `masses_amu` and `thermo`. At most 200 atoms. Adds the gradient, the analytic Hessian, dipole derivatives when available, vibrational analysis and thermochemistry. |
| `bond_orders` | `true` adds Mayer–Wiberg bond orders (SCF methods).                                                                                                                                                             |
| `orbitals`    | `"none"` (default), `"levels"` (energies and occupations), `"all"` (also every coefficient row and the basis) or `{ "rows": [i, …] }` (strictly increasing zero-based MO indices). SCF methods.                 |

`masses_amu` overrides the vibrational analysis masses with one positive finite
mass per atom and requires `thermo.symmetry_number` because automatic symmetry
ignores isotopes. `thermo` accepts optional `temperature_k`, `pressure_pa`,
`rotor_cutoff_cm1`, `imaginary_cutoff_cm1`, `frequency_scale` and `symmetry_number`.
Thermochemistry requires positive finite temperature, pressure and frequency
scale, a symmetry number of at least 1, and finite cutoffs. The engine uses
the magnitude of the imaginary cutoff.

A caller that saves a Hessian result computed with explicit masses must store
those masses on the frame it passes to `to_calc_result`.
When a retry rung converged, `to_calc_result` saves that rung's electronic
temperature and iteration limit instead of the ladder, so recomputing gives the
same numbers. For the g-xTB solvent composite it keeps the requested settings
and the component energies; the per-part settings stay in `Computed` (qcdata
has no field for them yet).

Each SCF starts from the previous geometry's converged wavefunction in the
same session (axtb's `restart: previous`), whatever geometries the optimizer
tried before, so a rejected trial geometry only changes the starting guess.
`scf.start` in the result says where the SCF started (`previous`, or the
method's own start `eeq`, `sad`, `eeq-bc`).

### `Computed`

```json
{ "id": 4, "ok": {
  "method": "gfn2", "conventions": "axtb", "engine_conventions": "axtb", "numerics": "gfn2:1",
  "settings": { "charge": 0, "uhf": 0, "solvent": null, "scf": {}, "threads": 1 },
  "energy": -5.0702222893, "energy_terms": { "repulsion": 0.03, … },
  "gradient": [[0.0, 0.0, 0.0146], …],
  "charges": { "scheme": "mulliken", "values": [-0.56, 0.28, 0.28] },
  "dipole_e_bohr": [0.0, 0.0, 0.85], "quadrupole_e_bohr2": [ … six values … ],
  "bond_orders": [{ "a": 0, "b": 1, "order": 0.92 }, { "a": 0, "b": 2, "order": 0.92 }],
  "orbitals": { "basis": { "shells": [ … ] }, "alpha": { "energies_hartree": [ … ],
    "occupations": [ … ], "rows": [0, 1, 2, 3, 4, 5], "coefficients": "" }, "beta": "restricted" },
  "homo_lumo_gap_ev": 14.18, "near_degenerate": false,
  "scf": { "iterations": 9, "start": "previous", "electronic_temperature_k": 300.0 },
  "hessian_method": "analytic",
  "vibrations": { "conventions": "axtb", "frequencies_cm1": [ … ], "ir_intensities_km_mol": [ … ],
    "reduced_masses_amu": [ … ], "force_constants_mdyn_per_angstrom": [ … ], "masses_amu": [ … ],
    "linear": false, "point_group": "c2v" },
  "thermo": { "temperature_k": 298.15, "pressure_pa": 101325.0, … },
  "arrays": [{ "name": "hessian", "shape": [9, 9], "chunks": 1 }, … ],
  "seconds": 0.02 } }
```

- `settings`: the session's settings as the worker applied them: `charge`,
  `uhf` (the resolved unpaired electrons; 0 for GFN-FF), `solvent`, `scf` (as
  given), `gfnff` (`reference_positions_bohr` and the resolved integer
  `fragment_charges`; GFN-FF only) and `threads`. With `method`, `conventions`,
  `engine_conventions`, `numerics`, `scf`, `hessian_method`, the vibrational
  conventions and the thermochemistry settings, a result records every setting
  that produced it.
- `energy` (Eh) and `energy_terms` (Eh, as axtb names them). `charges`:
  Mulliken (SCF methods) or EEQ (GFN-FF). `dipole_e_bohr` is about the
  coordinate origin. SCF methods also report `quadrupole_e_bohr2` about that
  origin, as the components of a traceless tensor ordered xx, xy, yy, xz, yz, zz.
- `bond_orders` (when asked): the pairs `a < b` whose Mayer–Wiberg order is at
  least `0.1` ("bonded pairs"), in row-major order of the N × N matrix.
- `orbitals` (when asked): a `reshiki_qcdata::Orbitals` in the canonical AO
  contract ([below](#orbitals-and-the-canonical-ao-contract)). Levels are
  always the whole spectrum. A closed shell is `"restricted"` (occupations
  0–2); a GFN2-xTB open shell is `{ "same_orbitals": { "occupations": […] } }`
  (one set of orbitals, alpha and beta occupations 0–1); a g-xTB open shell is
  `{ "unrestricted": { … } }`. `core_electrons` is the electrons of each atom
  outside the valence basis. `basis` and `rows` are present when rows were
  asked for; the coefficients arrive as binary chunks and are `""` in JSON.
- `homo_lumo_gap_ev`: finite and non-negative, or `null` without a LUMO
  (always `null` for GFN-FF). Converted from Eh with `27.211386245981`
  eV/Eh (CODATA 2022). `near_degenerate` is `true` if and only if a numeric
  gap is strictly below `0.1` eV. The user interface then shows: "Near-degenerate
  electronic structure: results can differ between computers. Try a higher
  electronic temperature or more unpaired electrons."
- `scf` (`null` for GFN-FF): iterations of the converged attempt, its start,
  the electronic temperature of the occupations, and with `retries` the rung
  that converged: `{ "rung": i, "of": n, "label": "…" }` (0 is the session's
  own settings). A solvent composite reports the highest rung used by its parts.
- `composite` is present for g-xTB in solvent. `components` maps axtb names
  to energies in Eh, including the solvent correction. `parts` maps
  `gxtb`, `gfn2_solvated` and `gfn2_gas` to their actual `method`,
  `conventions`, `electronic_temperature_k`, `max_iterations`, `iterations`
  and `retry` (`null` or a retry label). This includes settings selected by
  the driver's built-in ladders when `scf.retries` is omitted. With a Hessian,
  `hessian_parts` reports the same fields for each of the same three parts of
  the Hessian calculation, with `iterations` set to `null` when the driver
  reports no SCF count. Conversion to qcdata adds the component energies to
  `energy_terms` with a `composite.` prefix.
- `gfnff` (GFN-FF): `{ "fragments": [...] }`, the zero-based fragment of each
  atom; the reference geometry and fragment charges are in `settings.gfnff`.
  Conversion to qcdata requires the reference frame hash.
- `hessian_method`, `vibrations`, `thermo` (with `hessian`): frequencies in
  cm⁻¹ ascending with imaginary ones negative, IR intensities in km/mol when
  dipole derivatives are present, reduced masses and analysis masses in u,
  and force constants in mdyn/Å. `thermo` is a
  `reshiki_qcdata::Thermo` (corrections relative to `energy`, in Eh and Eh/K).
  In `xtb-6.8` vibrational conventions the analysis projects rigid-body motion,
  then discards up to six modes (five for linear molecules), starting with the
  first frequencies below 0.01 cm⁻¹ in magnitude, so a near-zero imaginary mode
  of a near-rigid system can be dropped.
- `arrays`: the binary arrays that follow, in this order when present:
  `hessian` [3N, 3N] (Eh/bohr², row-major, atom-major axes, symmetrized),
  `dipole_derivatives` [3N, 3] (e, ∂μ/∂R), `normal_modes` [M, 3N] (one unit
  Cartesian displacement per frequency, not mass-weighted),
  `coefficients_alpha` and `coefficients_beta` [rows, basis functions] (the
  canonical AO order; beta only for unrestricted orbitals).

## Large arrays

Arrays travel after the JSON response as kind-1 frames, raw little-endian
`f64`, so a 200-atom Hessian needs no decimal text.

1. The JSON response announces them: `{ "name", "shape": [rows, columns], "chunks": k }`,
   with rows and columns at least 1.
2. Exactly `k` chunk frames follow for each array, in the announced order.
   Each body is a 4-byte little-endian chunk index (0, 1, … per array)
   followed by whole `f64` values: exactly 524,287 values ((4 MiB − 4) / 8) in
   every chunk but the last, and the remaining 1 to 524,287 values in the
   last, so `k = ⌈rows × columns / 524,287⌉`.
3. Before it allocates, the client checks every announced name, order and
   shape against what it asked for and what the JSON part reports: `hessian`
   is exactly [3N, 3N] with N ≤ 200, `dipole_derivatives` [3N, 3],
   `normal_modes` [number of frequencies, 3N], each coefficient array
   [number of rows, `basis_functions`] within `reshiki_qcdata::MAX_COEFFICIENTS`
   per spin. While it reads, it checks the chunk indices, the chunk sizes and
   that every value is finite. Anything else, or any byte after the last
   chunk, is a protocol error; the session is gone.

A whole response, chunks included, must arrive within the request deadline.

## Errors

Errors replace `ok`:

```json
{ "id": 3, "error": { "kind": "scf", "message": "SCF did not converge in 250 iterations" } }
```

Kinds: `input` (the request), `unsupported` (the method cannot do it, from the
capability table), `unsupported_element`, `topology`, `scf`, `numerical` (from
axtb's error kinds), `limit` (size or budget), `state` (wrong order, such as
`compute` before `open`), `unavailable` (xTB cannot be used), and `protocol`.
After a `protocol` error (bad header, version, kind, length, JSON, or a
non-positive or non-increasing ID) the worker writes the error frame with `id`
0 and exits 2. Error messages are limited to 1 KiB of UTF-8 text. If encoding
would exceed the frame limit, the worker sends a short fixed message with the
same ID and error kind.

Without xTB, `hello` gives `engine: null` and an empty `methods` list. Every
other operation returns `unavailable`.

## Orbitals and the canonical AO contract

axtb stores orbitals in its own AO order: atom by atom, shell by shell,
spherical functions m = −l … l, p as y, z, x; each shell's coefficients
include the normalization of the x^l primitive, and the contracted function
need not have unit norm. Its spherical functions are unit real solid harmonics
with the standard signs (d₋₂ ∝ xy, d₋₁ ∝ yz, d₊₁ ∝ xz, d₊₂ ∝ x² − y²,
f₋₃ ∝ y(3x² − y²), f₊₃ ∝ x(x² − 3y²)). The worker converts them to the
canonical contract of `reshiki-qcdata` before they leave the process:

| axtb                                                               | Canonical                                                                           |
| ------------------------------------------------------------------ | ----------------------------------------------------------------------------------- |
| s                                                                  | s, `pure: false`                                                                    |
| p in y, z, x order                                                 | Cartesian p in x, y, z order, `pure: false`                                         |
| d, f spherical, m = −l … l                                         | `pure: true`, m = −l … l, same functions (no reordering, no sign change)            |
| contraction coefficient c (includes n(α, l))                       | d = c / n(α, l) / ‖contraction‖, so the canonical contraction has unit self-overlap |
| MO coefficient C of an AO whose contraction has norm ‖contraction‖ | C × ‖contraction‖ (the coefficient of the unit AO)                                  |

Shells with l > 3 are refused as `unsupported` (no axtb method in this build
produces them, and no Molden reference exists to check them). The conversion
is proved at the AO level by tests that evaluate the converted orbitals at
grid points and compare them with axtb's Cartesian Molden output for the same
calculation, and that convert the canonical orbitals to Molden's spherical
order with `reshiki_qcdata::mo_to_molden` and compare them with axtb's
spherical Molden output.

## Lifecycle, deadlines and cancellation

- The client starts the worker through the native process supervisor
  (`native/process`): its own process group or job, three pipes, CPU-time,
  resident-memory, address-space (Linux) and file-size limits. Defaults: 2 GiB
  heap budget, 3 GiB resident memory, one hour of CPU time for the whole
  session, 64 GiB address space on Linux, and 1 MiB file size. The client
  takes the `open` request's heap budget from its session limits. The client
  removes `AXTB_LAPACK` and `AXTB_LAPACK_BLAS` from the worker's environment
  so it uses axtb's built-in eigensolver. On Unix, an engine-enabled worker
  started with either variable set re-executes itself with both removed and
  keeps its arguments and standard streams. If re-execution fails, `open`
  returns `unavailable` and names both variables in the error message.
- A client session stays on the thread that started it. On Linux the worker
  receives a parent-death signal if that thread exits. A `Cancel` handle can
  be shared with other threads.
- Deadlines are per request: `hello` 10 s, `open` 30 s, `compute` 600 s by
  default (callers may shorten them). Closing uses the hello deadline.
  The client drains stdout and stderr while it waits, so a blocked pipe
  cannot stall either side.
- `Session::start(executable, limits, cancel)`, `open`, `compute` and
  `session.close(cancel)` check the caller's `&Cancel` before sending each
  request and while waiting for its reply. Closing also checks while waiting
  for the worker to exit. `start`, `open` and `compute` check cancellation
  before validating local input. An already cancelled token makes `start`
  return `Error::Cancelled` before spawning. If a worker is running, the
  client terminates its process group or job and reaps it before returning
  `Error::Cancelled`. A subsequent calculation needs a new session.
- A reply to an earlier request (a wrong `id`), a second response, or a
  chunk after the announced ones is a protocol error.
- **Crash or EOF**: if the worker exits or closes stdout before a complete
  response (chunks included), the client reports the exit status and the last
  4 KiB of stderr. The session is gone. If the worker reads EOF on stdin
  between requests, it exits 0. Exit 75 is the heap budget
  (`reshiki_process_heap`). On Unix the client uses the final reaped status. On
  Windows the client uses an exit 75 observed before cleanup because job
  termination also uses 75.
- Before sending a request, the client checks the atom limit from `hello`, the
  number and finiteness of the positions, the `open` thread count, and the
  method × task table with qcdata's `check_calculation`. A local
  `Error::Input` or `Error::Unsupported` leaves the worker running with its
  current geometry intact.
- The client checks response IDs; that `open` matches the requested method and
  conventions and has a non-empty numerics identity; that each result matches
  them, echoes the session's settings, has a finite energy, one finite
  gradient row per atom when requested, a gap that is null or finite and
  non-negative, a consistent `near_degenerate` flag, the requested parts and no
  others, orbitals that pass `reshiki_qcdata::Orbitals::validate`, and the
  arrays above.

## Using the worker from the optimizer

`reshiki_optimize::Potential` has a `hessian` hook. An xTB potential answers it
with one `compute` that asks for a Hessian at the optimizer's geometry, and
returns `None` (unavailable) above 200 atoms:

```rust
fn hessian(&mut self, positions: &[f64], out: &mut [f64]) -> Option<Result<(), EvalError>> {
    if positions.len() / 3 > protocol::MAX_HESSIAN_ATOMS {
        return None;
    }
    let request = Compute {
        positions: Some(positions.as_chunks::<3>().0.to_vec()),
        units: Units::Bohr,
        hessian: Some(HessianRequest::default()),
        ..Compute::default()
    };
    Some(self.session.compute(&request, self.cancel).map(|c| out.copy_from_slice(&c.hessian)).map_err(eval_error))
}
```

## Limits (version 2)

| Item                        | Limit                                       |
| --------------------------- | ------------------------------------------- |
| Atoms per session           | 500                                         |
| Atoms for a Hessian         | 200 (Hessian 600 × 600, 2.9 MB of chunks)   |
| JSON frame body, chunk body | 4 MiB each                                  |
| Orbital coefficients        | `reshiki_qcdata::MAX_COEFFICIENTS` per spin |
| Threads                     | 1–64                                        |
| Requests per session        | no fixed count; CPU-time limit              |

## Command line

`calc` is a CLI-only command.

```text
reshiki --cli calc [--task energy|gradient|hessian|orbitals] [--method gfn2|gfnff|gxtb]
    [--charge N] [--uhf N] [--conventions axtb|xtb-6.8]
    [--alpb NAME | --gbsa NAME] [--solvent-correction gfn2]
    [--etemp K] [--accuracy X] [--max-iter N] [--scf-retry K[:N]]... [--threads N]
    [--thermo-temperature K] [--gradient] [--wbo] FILE.xyz
```

reads the first structure of an XYZ file (1 to 500 atoms, finite Ångström
coordinates, case-insensitive element symbols) and runs one calculation
through the worker. `--task` defaults to `energy`. The CLI accepts
`--energy` for compatibility and always includes the energy. `--task hessian`
adds the analytic Hessian, vibrational analysis and thermochemistry (up to
200 atoms). `--task orbitals` asks for every orbital row and the basis.
`--gradient` adds the gradient to any task and `--wbo` adds the bond orders.
The CLI prints one JSON line with the worker's `Computed` plus `engine`.
It writes each binary array as `{ "shape": [rows, columns], "values": [...] }`
under its name and preserves the array descriptors and empty compact orbital
coefficient fields.

The [method](#methods-and-what-they-can-do) and [session settings](#open)
above also apply to the CLI. Use `--alpb` or `--gbsa` to select the solvent
model and `--solvent-correction gfn2` with a solvated g-xTB calculation.
Each `--scf-retry K[:N]` adds a rung with electronic temperature `K` in kelvin
and an optional iteration limit `N`. No rung may go below an explicitly given `--etemp`.
With `--task hessian`, `--thermo-temperature K` sets the thermochemistry
temperature (default 298.15 K). It must be greater than 0 K and at most 10000 K.

When `near_degenerate` is `true`, stderr carries the warning below.

```text
reshiki: Near-degenerate electronic structure: results can differ between computers. Try a higher electronic temperature or more unpaired electrons.
```

The warning leaves the calculation's exit status unchanged. A refused method ×
task combination exits 1 with qcdata's message. In a build without xTB the CLI
prints `reshiki: xTB is not included in this build` on stderr and exits 1.

`ts` uses single-threaded GFN2-xTB worker sessions for gas-phase
transition-state searches. It independently requests a Hessian analysis at
the final geometry and reports both the search and worker spectra.
[Transition-state command](agent-cli.md#ts)
