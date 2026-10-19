# Runtime safety audit — 2026-09-20

The first sections record the worker-based application at the time of the audit. Their validation counts and process descriptions are historical; see [architecture](architecture.md) for the current Rust engine and isolated native InChI helper. The sections from [naming worker](#naming-worker) on describe current limits.

The application and library forbid Rust `unsafe` code. Non-test builds deny
`clippy::unwrap_used`, `expect_used`, `panic`, `unreachable`, `todo`,
`unimplemented`, and `indexing_slicing`. Both crate roots enforce the same rules;
`cargo clippy --all-targets -- -D warnings` checks the production builds as well
as tests. Test assertions intentionally fail when a regression occurs.

Runtime changes include:

- Checked atom/bond/template lookups and slice iteration in editing, placement,
  selection handles, previews, rendering and the chemistry bridge.
- Fallible worker startup, protocol decoding and output serialization; request
  counter exhaustion returns an error and allows worker restart.
- Checked clipboard ID allocation. Invalid fragments, missing references or
  exhausted IDs leave the destination untouched.
- Validated edits roll back before entering history when their document is
  invalid. Template attachment rejects invalid geometry and source graphs.
- Safe signed-charge formatting and saturating charge changes, including
  `i32::MIN` and `i32::MAX` inputs.
- UTF-8-safe color/text/number parsing; bounded sequences and checked numeric
  increments. Missing or malformed bundled templates produce a library notice;
  invalid bundled style JSON falls back to explicit JACS settings.
- SVG string construction no longer unwraps formatting results. Image exports
  validate their document, retain the existing raster size cap, and return
  allocation/encoding errors.

Regression coverage exercises dangling bonds, stale anchors and handles,
invalid geometry, exhausted IDs, extreme charges, Unicode and invalid text
ranges, malformed chemistry requests, and recovery on the following valid
request. It also checks atomic rollback and the bundled JACS/catalog data.

Validation: 125 Rust tests and 36 Python tests passed; formatting and strict
Clippy passed. The current application was exercised through native computer
control for label editing, dragging, Undo/Redo and save/open. Logs are under
`artifacts/atom-labels-qa-20260920/` (local, ignored by Git).

These checks cover ReShiki's runtime source and the exercised inputs. They do
not prove that operating-system services or third-party libraries can never
fail. The chemistry worker remains a separate process, and bridge errors are
reported without replacing the current drawing.

## Native clipboard update

The macOS helper uses checked optionals and explicit errors, without forced unwraps. Requests/responses are bounded, child execution times out after ten seconds, and the Rust process concurrently drains stdout/stderr. The helper validates all representations before clearing the clipboard. Cut deletes only after a successful write; captured document revisions prevent delayed Cut/Paste from changing newer work.

Binary drawing parsing bounds input to 16 MB, nesting to 64 levels, objects to 100,000 and properties to one million. Malformed lengths, duplicate nonzero identifiers, unsupported objects/properties and detected query predicates produce recoverable errors. Raster clipboard bounds are finite and checked before fixed-point encoding. See [clipboard verification and limits](clipboard.md).

This update passes **135 Rust tests**, **42 Python tests**, formatting and strict Clippy. Test totals above record earlier milestones.

## Naming worker

Every name parse and every name generation starts a fresh child of the same executable in `--naming-worker` mode. `src/naming/client.rs` supervises it with `WORKER_LIMITS`, and `reshiki-process` (`native/process/`) applies those limits before the worker runs. That crate holds all of the platform FFI.

| Limit           | Value                                                                                                                | Enforced by                                                              |
| --------------- | -------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| Wall-clock time | 15 s; the client polls every 25 ms                                                                                   | `DEADLINE` and `POLL` in `src/naming/client.rs`                          |
| Memory          | 384 MiB resident, sampled at every poll on macOS and Linux; 384 MiB of job memory on Windows                         | `WORKER_LIMITS.memory_bytes`; `native/process/src/unix.rs`, `windows.rs` |
| Address space   | 8 GiB, on Linux only (`RLIMIT_AS`)                                                                                   | `WORKER_LIMITS.address_bytes`; `unix.rs`                                 |
| CPU time        | 20 s (`RLIMIT_CPU`; job user time on Windows)                                                                        | `WORKER_LIMITS.cpu_seconds`; `unix.rs`, `windows.rs`                     |
| File size       | 8 MiB on macOS and Linux (`RLIMIT_FSIZE`)                                                                            | `WORKER_LIMITS.file_bytes`; `unix.rs`                                    |
| Worker heap     | 96 MiB of live Rust allocations                                                                                      | `HEAP_BYTES` in `src/naming/worker.rs`                                   |
| Request         | 32,768 bytes; a name has at most 2,048 bytes and no control characters                                               | `WIRE_LIMIT` and `MAX_NAME_BYTES` in `worker.rs`                         |
| Response        | 65,536 bytes for a parse; 32,768 bytes for a generated name                                                          | `OUTPUT_LIMIT` in `worker.rs`; `src/naming/reverse.rs`                   |
| Molecule size   | 512 atoms and 32,768 SMILES bytes for a naming identity; 1–64 heavy atoms and 200,000 work units for name generation | `crates/model/src/molecule_names.rs`, `src/naming/rules.rs`              |
| Concurrency     | One parse and one generation at a time                                                                               | `SLOTS` in `src/naming/local.rs` and `reverse.rs`                        |

The worker starts in a new temporary folder with core dumps off on macOS and Linux. On macOS and Linux it leads its own process group, and termination kills the whole group. Linux also kills the worker when the app exits (`PR_SET_PDEATHSIG`). On macOS a small observer process kills it when the app or the worker exits, and after 20 seconds at most. On Windows a job object allows one process and kills it when the job closes. The worker must close its pipes within one second of exiting, so a launcher that leaves a daemon behind fails. Cancelling or dropping a request kills the worker. Any exceeded limit returns an error, and nothing is inserted.

`RESHIKI_NAMING_HELPER` can name another executable by absolute path. Test binaries refuse to start naming without it.

### Alias catalog

`src/naming/catalog.rs` loads the bundled `catalog/data.json` once, through `OnceLock`, and fails closed:

- The data is at most 65,536 bytes (`MAX_CATALOG_BYTES`), with 1–40 entries (`MAX_ENTRIES`) and 1–5 names per entry (`MAX_ALIASES`).
- A name is at most 128 bytes (`MAX_ALIAS_BYTES`) and has no control characters.
- Each entry is one connected identity. Names, compared without ASCII case, and identities never repeat.

An invalid catalog makes `aliases_for_smiles` fail. `src/naming/local.rs` and `reverse.rs` propagate that error, so every local name parse and name generation fails. Catalog edits must therefore pass `src/naming/catalog/tests.rs`.

## EMF worker

Windows only. EMF playback runs Windows GDI+ on picture data from files, so it never runs in the editor or its Office server. The app relaunches itself in one of three modes from `native/windows/src/metafile_worker.rs`:

- `--emf-worker` plays an imported EMF file into its PNG preview. `src/metafile.rs` starts it.
- `--emf-file-worker` and `--emf-office-worker` record an EMF figure file or Office object for a drawing that contains a retained EMF picture. `crates/io/src/native_windows.rs` starts them through `reshiki_windows::isolated_metafile`; drawings without such a picture record in process.

| Limit                   | Value                                                                                                        | Enforced by                                                        |
| ----------------------- | ------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------ |
| Committed memory        | 512 MiB for the worker process, including allocations GDI+ makes outside Rust                                | The job object of `limited_job` in `metafile_worker.rs`            |
| Playback time           | 30 s (`PLAYBACK_LIMIT`). The worker ends itself, and the caller stops waiting and kills it.                  | `deadline` and `record` in `metafile_worker.rs`; `src/metafile.rs` |
| Import input            | 16 MiB, after `reshiki-metafile` checks the EMF framing, the record count (100,000) and the physical size    | `MAX_BYTES` and `MAX_RECORDS` in `crates/metafile/src/lib.rs`      |
| Import preview          | 16 MiB; `Picture::from_emf` validates the EMF again, and the EMF and its preview must fit in 16 MiB together | `src/metafile.rs`, `crates/model/src/pictures.rs`                  |
| Export input and output | 128 MiB                                                                                                      | `MAX_SNAPSHOT` in `metafile_worker.rs`                             |

The worker reads only stdin and writes only stdout. Its stderr is discarded, and no console window opens. A failure or timeout returns an error and leaves the drawing unchanged.

## Bounded editor jobs

These jobs run off the UI thread. NMR prediction, generic expansion and Auto-map run on blocking tasks inside the app process; 3D geometry runs in its worker process. Each job has its own bounds, and a job that fails or runs out of budget leaves the drawing unchanged.

### NMR prediction

Enforced in `crates/chemistry/src/nmr.rs`, `crates/io/src/nmr.rs` and `src/app/nmr/worker.rs`:

- The encoder and the index lookup stop at a 3-second work deadline (`Environment::deadline`).
- A 6-second outer timeout (`PREDICTION_TIMEOUT`) must exceed that deadline, so an engine error normally arrives first.
- One calculation runs per process. A second request is refused at once with "An NMR calculation is already running; try again shortly". The permit stays held until the blocking work ends, even after the timeout.
- A request covers one molecule of at most 128 atoms. The bundled index is at most 32 MiB.

### Generic expansion

Enforced in `crates/model/src/generic/limits.rs`, `Limits` in `crates/model/src/generic.rs`, `crates/model/src/generic/budget.rs` and `src/app/generic.rs`:

- Each request has one work allowance of 50,000,000 units (`Limits::default`). Every stage pays from it before it copies or scans, and it never resets.
- An expansion has at most 500 candidate products and 100,000 atoms in total, counted before duplicates are removed.
- The source has at most 100,000 atoms and 300,000 bonds.
- A drawing has at most 10,000 definitions, 118 choices per site, repeat counts from 1 to 1000, and 100,000 repeat members and crossing bonds in all. Labels have at most 64 characters (`MAX_LABEL_CHARS`).
- One expansion runs at a time.

### Auto-map

Enforced in `crates/model/src/reaction_mapping/search.rs` (`Budget::default`), started from `src/app/reaction_mapping.rs`:

- The search has 20,000,000 work units and 250,000 search states.
- Running out of work before the search has a full correspondence is an error: "Auto-map search budget reached before a full correspondence was found; add manual anchors".
- A search that reaches either limit after that keeps its best feasible mapping. The status reads "Search budget reached · Review the feasible proposal or add manual anchors", and the review panel shows "Budget reached · optimality and uniqueness unknown" above **Apply maps**.

### 3D geometry

Enforced in `crates/process-heap/src/policy.rs` and `crates/io/src/geometry/client.rs`. Each calculation resolves an allocator budget of 64–512 MiB and a deadline of 60–120 s from the machine's memory headroom and throughput, and runs in the `--geometry-worker` process. The [calculation resource policy](chemistry-resource-policy.md) gives the formulas, reservations and structural limits.

## Agent API (P1, experimental)

The experimental agent API (`reshiki --mcp` and `reshiki --cli`; see the
[agent API reference](agent-api.md)) is in ReShiki 0.12.0 and later. MCP asks
servers to validate tool inputs, enforce access controls, rate-limit calls and
sanitize outputs
([MCP tools, Security Considerations](https://modelcontextprotocol.io/specification/2026-07-28/server/tools)).
This section records how P1 does each, what remains possible, and the tests
that pin it. Gate numbers refer to the P1 acceptance gates.

### Budgets

The operation budgets are `Budgets::default()` in
`crates/agent/src/ops/budget.rs`. The MCP connection limits are
`Limits::from_budgets` in `crates/mcp/src/framing.rs`. The `info` tool and
`reshiki --cli info` report the operation budgets.

The [generated budget table](agent-reference.md#budgets) lists the default
operation limits and their units. Session weight counts objects plus one for
each document and history frame. Picture memory counts unique picture data
across documents and history.

| Limit                                                                       | When exceeded                                            |
| --------------------------------------------------------------------------- | -------------------------------------------------------- |
| Input text, CDX base64, documents, objects, object IDs, pictures and output | `budget`                                                 |
| Session weight                                                              | `budget`, checked before any change                      |
| History depth                                                               | The oldest step is dropped                               |
| Inspect                                                                     | `truncated: true`; counts stay complete                  |
| Idempotency                                                                 | The oldest receipt is dropped; a longer key is invalid   |
| Idle documents                                                              | `unknown_document`                                       |
| Concurrency and queue                                                       | `busy`                                                   |
| Deadline                                                                    | `timeout` at the next cooperative checkpoint             |
| Preview render                                                              | `invalid_arguments` for a side, `budget` for the pixels  |
| PNG export                                                                  | Resolution steps down from the style's dpi until it fits |

MCP framing and process limits also apply:

| Limit                      | Value                                                | When exceeded                                                    |
| -------------------------- | ---------------------------------------------------- | ---------------------------------------------------------------- |
| Request line               | 24 MiB (25,165,824 bytes) before the newline         | JSON-RPC -32600 `Request exceeds N bytes`; serving continues     |
| Request ID                 | 256 bytes for a string ID                            | -32600 without an ID                                             |
| Outstanding requests       | 12 (concurrency + queue + 2), holding at most 48 MiB | The reader stops reading until a request completes               |
| MCP result                 | 48 MiB serialized                                    | `budget`: request a smaller render or fewer objects              |
| Error messages             | 500 characters                                       | Cut, so errors never echo large input                            |
| `agent-access.json`        | 64 KiB                                               | Exit 2 at startup                                                |
| Heap (`--mcp` and `--cli`) | See the [generated MCP help](agent-reference.md#mcp) | stderr `RESHIKI_HEAP_LIMIT <budget> <used> <requested>`, exit 75 |

An invalid `RESHIKI_AGENT_HEAP_MB` is a usage error (exit 2) before anything
is read. The heap ceiling counts the server or command's own allocations; the
InChI and geometry workers it starts keep their own limits.

### Containment

- **Transport.** `reshiki --mcp` serves only the standard input and output
  of the client that started it. It opens no listener and makes no network
  requests. Stdout carries MCP messages only. Stderr carries a bounded log
  that never includes drawing, file or request content; a panic logs its
  location, never its payload.
- **Sessions.** Documents live in the server's memory, behind opaque handles
  checked against their owner. The server cannot reach drawings open in the
  app.
- **Files.** Only `file_open` and `file_save` touch files, and only inside
  folders granted at startup with `--allow-read` and `--allow-write` or in
  `agent-access.json`. Read and write grants are separate. Each folder is
  opened once, from its canonical path, as a directory handle (cap-std).
  Every request must be a plain absolute path inside a granted folder, with
  an allowed extension, and is resolved relative to that handle: `..` and
  symlinks that leave the folder are refused. Writes go
  to a temporary file beside the target, then a no-clobber hard link
  publishes a new file or a rename replaces an existing one when `overwrite`
  is true. Folders are never created.
- **Refused folders.** File system roots, the home folder (as Windows or the
  environment reports it) and its ancestors, folders that overlap ReShiki's
  data folder, earlier installations' data or the executable's folder, Unix
  `/proc`, `/sys` and `/dev`, and the Windows system folder cannot be
  granted, even before they exist. A refused folder or an unreadable
  `agent-access.json` exits 2 before stdin is read.
- **Client roots.** The server never requests MCP roots, which the
  2026-07-28 revision deprecates as informational guidance rather than access
  control. A client's project folder, such as Claude Code's
  `CLAUDE_PROJECT_DIR`, grants nothing.
- **Fences.** Outside its `access` module, `reshiki-agent` cannot call
  filesystem APIs, and `reshiki-mcp` uses none (`clippy.toml` in each crate,
  checked by `tests/test_agent_fs_fence.py`).
- **Command line.** `reshiki --cli` reads and writes the paths on its command
  line with the user's own permissions, without grants or an extension
  allowlist. It never replaces an existing output without `--force`.
- **Windows.** Both modes keep the client's pipes out of the worker processes
  they start, so a worker that outlives the server cannot hold the client's
  output open; startup fails if a pipe stays inheritable.

### Residual risks

- **Hard links.** A file inside a granted folder can be a hard link to a file
  outside it. Reads follow it. Refusing every multiply linked file would
  also refuse ordinary files, so this is accepted. Replacing such a file
  writes a new file and leaves the outside file's bytes unchanged.
- **Same-user processes.** Another process running as the same user can swap
  folders, including while the grants are being opened at startup. The
  post-open checks narrow that window; they cannot close it.
- **Roots captured at startup.** Grants are opened once. A granted folder
  that is later moved keeps resolving inside the folder that was opened, and
  new grants need a restart of the server.
- **Windows identity.** On Unix the opened folder's device and inode must
  match its canonical path. cap-std offers no such check on Windows, so only
  the canonical path is checked again there.
- **Cloud placeholders.** Reading a OneDrive, iCloud Drive or similar
  placeholder may make the sync client download it.
- **macOS privacy permissions.** macOS attributes the server's file access to
  the client app that started it. Folders in Documents, Desktop, Downloads or
  iCloud Drive work only when that client has permission, and macOS may ask
  on the client's behalf.
- **FAT and exFAT.** Without hard links, `file_save` cannot create a new file
  without risking an overwrite and fails with `no_clobber_unsupported`;
  `overwrite: true` still replaces.
- **Spelling.** Paths match a granted folder exactly on Unix and
  ASCII-case-insensitively on Windows. Other spellings, such as another
  Unicode normalization, fail closed as `path_not_granted`.
- **Interrupted writes.** An interrupted publication can leave a
  `.reshiki-<pid>-<n>.tmp` file beside a complete destination.
- **Cooperative deadline.** Blocking chemistry is not pre-empted. Deadlines
  and cancellation take effect at the next checkpoint, a call keeps its
  permit until its work ends, and the input budgets, the existing worker
  timeouts and the heap ceiling bound that work.

### Malformed input and cancellation

transport-2's framing contract (`crates/mcp/src/framing.rs`) answers every
malformed line and keeps serving:

| Input                                                      | Reply                                                    |
| ---------------------------------------------------------- | -------------------------------------------------------- |
| Not UTF-8 or not JSON                                      | -32700 `Parse error`, without an ID                      |
| JSON that is not an object, or an invalid ID               | -32600 `Invalid Request`, without an ID                  |
| `jsonrpc` other than "2.0", or no string `method`          | -32600, with the ID when it is valid                     |
| A line over the limit                                      | -32600 `Request exceeds N bytes`, with the ID when found |
| A duplicate in-flight ID                                   | -32600 `Duplicate request id` with that ID               |
| A client response, or an unknown or malformed notification | Nothing                                                  |
| An unknown method                                          | -32601                                                   |
| Malformed parameters, an unknown tool or a missing `_meta` | -32602                                                   |
| An unsupported protocol revision                           | -32022 with `data.supported`                             |
| A tool task that panicked                                  | -32603; serving continues                                |
| Invalid tool arguments and every other tool failure        | An `isError` result with an operation error code         |

The ID is omitted, never null, when it cannot be read; 2025-11-25 and
2026-07-28 allow that (`id?: RequestId`).

Cancellation is advisory, and completion may race it
([MCP cancellation](https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/cancellation)):

- `notifications/cancelled` for an outstanding request reaches the host once.
  The writer drops that request's response, whenever it arrives.
- A call still queued never runs. A running call stops at its next
  checkpoint and keeps its permit until its blocking work ends.
- Every edit is one atomic commit. A cancel that arrives after the commit
  keeps the edit and suppresses the response. Repeating an `apply` with the
  same `idempotency_key` returns the stored receipt instead of editing again.
- `import`, `compose`, `file_open` and `document_new` are not idempotent. A
  cancelled create can leave a document that `document_list` shows and that
  counts toward the document budget until it is closed or expires.
- A cancel for an unknown or already answered request is ignored. While 12
  requests are outstanding the reader stops, so a cancel is read once a slot
  frees.

The server exits when stdin ends and its replies are delivered. A client that
stops reading stdout keeps it alive until the client reads, closes the pipe
or terminates the process.

### Tests

| Gate | Area                             | Tests                                                                                                                                                                                              |
| ---- | -------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| G3   | Grants and escapes               | `crates/agent/src/access/escape_tests.rs` (`unix.rs`, `windows.rs`), `crates/agent/src/access/tests.rs`, `tests/agent_api_grants.rs`, `tests/test_agent_fs_fence.py`                               |
| G3   | Command-line files               | `tests/agent_api_cli_files.rs`                                                                                                                                                                     |
| G4   | Budgets, queue, deadline, cancel | `crates/agent/src/ops/exec/tests.rs`, `crates/agent/tests/headless.rs`, `crates/mcp/tests/transcripts.rs`, `tests/agent_api_runtime.rs`                                                            |
| G4   | Backpressure and shutdown        | `tests/mcp_shutdown.rs`, `tests/mcp_stdio.rs`                                                                                                                                                      |
| G4   | Heap ceiling                     | `tests/agent_api_runtime.rs` (`the_heap_ceiling_is_validated_and_applied`)                                                                                                                         |
| G4   | Windows pipes                    | `tests/agent_api_windows_pipes.rs`, also run in the release profile on x64 and ARM64                                                                                                               |
| G5   | Malformed input                  | `tests/agent_api_malformed.rs` with `tests/fixtures/agent-api/malformed/`, `crates/mcp/src/framing/tests.rs`, `crates/mcp/tests/transcripts.rs` (`errors.jsonl`), `crates/mcp/tests/panic_hook.rs` |
