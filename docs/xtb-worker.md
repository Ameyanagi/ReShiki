# xTB worker protocol

xTB calculations run in a child process of the application executable,
`reshiki --xtb-worker`, started by the client in `crates/xtb-worker`. One child
holds one **session**: one molecule (atoms, charge, unpaired electrons, method,
solvent), evaluated at one geometry after another so that each SCF can start
from the previous wavefunction. Licensing and packaging are in
[xTB licensing](xtb-licensing.md).

Protocol version 1 covers energy and gradient. Larger results (Hessians,
orbitals) use the binary chunks planned below.

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

| `op`      | Fields                                                                                         | `ok` result                                                                                            |
| --------- | ---------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `hello`   | none                                                                                           | protocol, engine, methods, operations, max_atoms                                                       |
| `open`    | `numbers`, `positions`, `units`, `charge`, `uhf`, `method`, `solvent`, `threads`, `heap_bytes` | `atoms`, `method`, `numerics` (axtb's numerics identity, e.g. `gfn2:1`)                                |
| `compute` | `positions` (optional: the last geometry), `units`, `gradient`                                 | `energy` (Eh), `gradient` (Eh/bohr, one `[x, y, z]` per atom, when asked), `scf_iterations`, `seconds` |
| `close`   | none                                                                                           | `{}`; the worker then exits 0                                                                          |

- `numbers`: atomic numbers, 1 to 500 atoms. `positions`: one `[x, y, z]` per
  atom, finite. `units`: `"angstrom"` or `"bohr"`, always explicit. Ångström
  is converted with the factor axtb's XYZ reader uses for the method's
  conventions, so a geometry read from an XYZ file gives the axtb CLI's numbers
  bit for bit.
- `charge`: integer, default 0. `uhf`: unpaired electrons, default from the
  electron count. `method`: `"gfn2"` (the only one in version 1). `solvent`:
  `null` or `{ "model": "alpb" | "gbsa", "name": "water" }`. `threads`: 1 to
  64, default 1. `heap_bytes`: the worker's allocator budget.
- `open` is allowed once per process; `compute` only after a successful open.
  A failed `compute` (for example an SCF that did not converge) keeps the
  session open at the last successful geometry.
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
frame with `id` 0 and exits 2.

## Lifecycle, deadlines and cancellation

- The client starts the worker through the native process supervisor
  (`native/process`): its own process group or job, three pipes, CPU-time,
  resident-memory, address-space (Linux) and file-size limits. Defaults: 2 GiB
  heap budget, 3 GiB resident memory, one hour of CPU time for the whole
  session, 64 GiB address space on Linux, and 1 MiB file size. The client
  takes the `open` request's heap budget from its session limits.
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
  it exits 0. Exit 75 is the heap budget (`reshiki_process_heap`).
- An incomplete response on an open pipe is subject to the request deadline.
  If the worker closes the pipe first, the client reports EOF.
- The client checks every response: matching `id`, finite energy, one
  finite gradient row per atom.

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

`reshiki --cli calc --energy [--gradient] [--charge N] [--uhf N] FILE.xyz`
reads the first structure of an XYZ file (Ångström), runs one GFN2-xTB
calculation through the worker, and prints one JSON object: `energy`
(Eh), `gradient` (Eh/bohr) when asked, `method`, `numerics` and `engine`. In a
build without xTB it prints `reshiki: xTB is not included in this build`
on stderr and exits 1.
