# xTB worker protocol

xTB calculations run in a child process of the application executable,
`reshiki --xtb-worker`, started by the client in `crates/xtb-worker`. One child
holds one **session**: one molecule (atoms, charge, unpaired electrons, method,
solvent), evaluated at one geometry after another so that each SCF can start
from the previous wavefunction. Licensing and packaging are in
[xTB licensing](xtb-licensing.md).

Protocol version 1 covers GFN2-xTB energy, gradient, and a HOMO–LUMO gap
diagnostic. Larger results (Hessians, orbitals) use the binary chunks planned
below.

## Frames

Every message in either direction is one frame: a 16-byte header and a body.

| Bytes | Meaning                                                  |
| ----- | -------------------------------------------------------- |
| 0–7   | ASCII `RSHXTBWK`                                         |
| 8–9   | Little-endian protocol version, 1                        |
| 10–11 | Little-endian frame kind: 0 JSON message, 1 binary chunk |
| 12–15 | Little-endian unsigned body length in bytes              |

A JSON body is one UTF-8 JSON object, at most 4 MiB. Version 1 sends only kind
0; a kind-1 frame is a protocol error until the chunk format below is
implemented. Unknown JSON fields are errors, never ignored.

## Requests and responses

The client sends one request and waits for its response before sending the
next (no pipelining). Each request has an `id`, a positive integer that
increases with every request; the response repeats it.

```json
{ "id": 1, "op": "hello" }
{ "id": 1, "ok": { "protocol": 1, "engine": { "name": "axtb", "version": "0.2.0",
  "license": "LGPL-3.0-or-later", "source": "https://github.com/Ameyanagi/axtb",
  "rev": "06e29058…" }, "methods": ["gfn2"], "operations": ["energy", "gradient"],
  "max_atoms": 500 } }
```

| `op`      | Fields                                                                                                        | `ok` result                                                                                                                                                                        |
| --------- | ------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `hello`   | none                                                                                                          | protocol, engine, methods, operations, max_atoms                                                                                                                                   |
| `open`    | `numbers`, `positions`, `units`, `charge`, `uhf`, `method`, `conventions`, `solvent`, `threads`, `heap_bytes` | `atoms`, `method`, `conventions`, `numerics` (axtb's numerics identity, e.g. `gfn2:1`)                                                                                             |
| `compute` | `positions` (optional: the last geometry), `units`, `gradient`                                                | `energy` (Eh), `gradient` (Eh/bohr, one `[x, y, z]` per atom, when asked), `method`, `conventions`, `numerics`, `homo_lumo_gap_ev`, `near_degenerate`, `scf_iterations`, `seconds` |
| `close`   | none                                                                                                          | `{}`; the worker then exits 0                                                                                                                                                      |

- `numbers`: atomic numbers, 1 to 500 atoms. `positions`: one `[x, y, z]` per
  atom, finite. `units`: `"angstrom"` or `"bohr"`, always explicit. Ångström
  is converted with the factor axtb's XYZ reader uses for the selected
  `conventions`, so a geometry read from an XYZ file gives the corresponding
  axtb CLI's numbers bit for bit. The same conversion applies to replacement
  geometries in `compute`.
- The `conventions` field accepts `"axtb"` (default) or `"xtb-6.8"` (xtb 6.8 compatible).
  These protocol names map to the axtb driver's `"axtb"` and `"xtb"`
  conventions, respectively. Both `Opened` and every `Computed` response
  include `method`, `conventions`, and `numerics` using protocol spellings.
- `charge`: integer, default 0. The total electron count is the sum of the
  atomic numbers minus the charge. `uhf`: unpaired electrons, default from
  the electron count. `open` returns `input` if the electron count is negative
  or if `uhf` exceeds it or differs in parity. Version 1 requires
  `"method": "gfn2"`. GFN2-xTB is the default method and the only one
  available in version 1. The planned name `"gfnff"` means GFN-FF, whose
  results will carry a null gap and a false near-degenerate flag. The planned
  name `"gxtb"` means g-xTB 2.0.1 only. g-xTB v1.1 will never be offered.
  `solvent`: `null` or
  `{ "model": "alpb" | "gbsa", "name": "<solvent>" }`. The name can be any
  solvent axtb accepts for the selected model. An unknown name returns `input`.
  `threads`: 1 to 64, default 1. `heap_bytes`: the worker's allocator budget.
- `open` is allowed once per process; `compute` only after a successful open.
  A failed `compute` (for example an SCF that did not converge) keeps the
  session open at the last successful geometry.
- Every `Computed` response reports `homo_lumo_gap_ev` as a finite,
  non-negative number in eV or `null` when there is no LUMO. Conversion from
  axtb's Eh value uses `27.211386245981` eV/Eh (CODATA 2022). It also includes
  `near_degenerate`, which is `true` if and only if a numeric gap is strictly
  below `0.1` eV. A gap of exactly `0.1` eV and a null gap both give `false`.
- Without xTB, `hello` gives `engine: null` and empty `methods` and
  `operations` lists. Every other operation returns `unavailable`.

Errors replace `ok`:

```json
{ "id": 3, "error": { "kind": "scf", "message": "SCF did not converge in 250 iterations" } }
```

Kinds: `input` (the request), `unsupported_element`, `topology`, `scf`,
`numerical` (from axtb's error kinds), `limit` (size or budget), `state` (wrong
order, such as `compute` before `open`), `unavailable` (this build has no xTB),
and `protocol`. After a `protocol` error (bad header, version, kind, length,
JSON, or a non-positive or non-increasing ID) the worker writes the error
frame with `id` 0 and exits 2. Error messages are limited to 1 KiB of UTF-8
text. If encoding would exceed the frame limit, the worker sends a short
fixed message with the same ID and error kind.

## Lifecycle, deadlines and cancellation

- The client starts the worker through the native process supervisor
  (`native/process`): its own process group or job, three pipes, CPU-time,
  resident-memory, address-space (Linux) and file-size limits. Defaults: 2 GiB
  heap budget, 3 GiB resident memory, one hour of CPU time for the whole
  session, 64 GiB address space on Linux, and 1 MiB file size. The client
  takes the `open` request's heap budget from its session limits. The worker
  always uses axtb's built-in eigensolver. The client removes `AXTB_LAPACK`
  and `AXTB_LAPACK_BLAS` from the worker's environment.
- A client session stays on the thread that started it. On Linux the worker
  receives a parent-death signal if that thread exits. A `Cancel` handle can
  be shared with other threads.
- Deadlines are per request: `hello` 10 s, `open` 30 s, `compute` 600 s by
  default (callers may shorten them). Closing uses the hello deadline.
  The client drains stdout and stderr while it waits, so a blocked pipe
  cannot stall either side.
- **Cancel** is kill and reap: the client terminates the process group and
  waits for it before returning. The session is gone; a new one starts with
  `open`. There is no cancel message, so there are no late replies.
- **Crash or EOF**: if the worker exits or closes stdout before a complete
  response, the client reports the exit status and the last 4 KiB of stderr.
  The session is gone. If the worker reads EOF on stdin between requests,
  it exits 0. Exit 75 is the heap budget (`reshiki_process_heap`). On Unix
  the client uses the final reaped status. On Windows the client uses an
  exit 75 observed before cleanup because job termination also uses 75.
- An incomplete response on an open pipe is subject to the request deadline.
  If the worker closes the pipe first, the client reports EOF.
- Before sending an `open` or replacement geometry, the client checks the
  atom limit from `hello`, the number and finiteness of the positions, and
  the `open` thread count. A local `Error::Input` leaves the worker running
  with its current geometry intact.
- The client checks response IDs and verifies that an `open` response matches
  the requested method and conventions and provides a non-empty numerics
  identity. It checks that each result matches all three fields from `open`
  and has finite energy, one finite gradient row per atom when requested,
  a gap that is null or finite and non-negative, and a `near_degenerate`
  flag that agrees with the gap threshold.

## Planned: binary chunks (not implemented)

Arrays too large for a JSON frame travel as raw little-endian `f64`:

1. The JSON response announces them:
   `"arrays": [{ "name": "hessian", "shape": [3N, 3N], "chunks": k }]`.
2. Exactly `k` kind-1 frames follow for each array, in order. Each body is a
   4-byte little-endian chunk index followed by whole `f64` values, at most
   4 MiB per frame.
3. The client checks the index sequence, the total count against `shape`, and
   a per-array ceiling before allocating (Hessian: 200 atoms, 600 × 600; orbital
   coefficients: bounded by the basis size the session reported).

## Limits (version 1)

| Item                 | Limit                          |
| -------------------- | ------------------------------ |
| Atoms per session    | 500                            |
| JSON frame body      | 4 MiB                          |
| Threads              | 1–64                           |
| Requests per session | no fixed count; CPU-time limit |

## Command line

`reshiki --cli calc --energy [--gradient] [--charge N] [--uhf N] [--conventions axtb|xtb-6.8] FILE.xyz`
reads the first structure of an XYZ file (Ångström, case-insensitive element
symbols) and runs one GFN2-xTB calculation through the worker. The JSON
result includes `energy` (Eh), `gradient` (Eh/bohr) when asked, `method`,
`conventions`, `numerics`, `engine`, `homo_lumo_gap_ev`, and `near_degenerate`.
`--conventions` defaults to `axtb`. The `xtb-6.8` option selects xtb 6.8 compatible
conventions and their Ångström-to-bohr conversion.

When `near_degenerate` is `true`, stderr carries the warning below.

```text
reshiki: Near-degenerate electronic structure: results can differ between computers. Try a higher electronic temperature or more unpaired electrons.
```

The warning leaves the calculation's exit status unchanged. In a build
without xTB the CLI prints `reshiki: xTB is not included in this build` on
stderr and exits 1.
