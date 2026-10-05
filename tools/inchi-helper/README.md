# Rust InChI worker

The helper uses `cosmolkit-inchi = 0.3.0` (InChI 1.07.5) from
[Ameyanagi/COSMolKit](https://github.com/Ameyanagi/COSMolKit), pinned to commit
`3a437849dcd28319b1a3cdf02d7897a7ee200cb2`. The fork corrects the heavy-element
hydrogen table, preserves perchlorate match ordering and ports RDKit's explicit
unknown-stereo handling. It also reserves string terminators for long inputs. See its
[compatibility notes](https://github.com/Ameyanagi/COSMolKit/blob/3a437849dcd28319b1a3cdf02d7897a7ee200cb2/crates/cosmolkit-inchi/FORK.md).
The application supplies ReShiki's existing Rust property-cache, Kekulé,
sanitation, hydrogen-removal and stereochemistry routines through its toolkit
traits. No InChI C source, FFI binding, native archive, RDKit installation or
Python interpreter is required at runtime.

Release packages contain only the application executable. The app relaunches `current_exe()` with `--inchi-worker` before initializing graphics or native UI. Its clipboard and printing modes on macOS use the same executable as well. Each operation still has its own process, pipes, deadline and heap budget.

A development-only executable shares `src/chemistry/inchi/worker.rs` and the same allocator for independent transport tests:

```sh
python3 scripts/build_inchi_helper.py
# Optimized test executable with dependency/source/target metadata:
python3 scripts/build_inchi_helper.py --production --target aarch64-apple-darwin
```

The test build script needs Python 3.11 or newer and Cargo; it also compiles a dependency-free Rust fault-injection stub unless `--production` is selected. Its startup check runs only when the target matches the compiler host. None of these test executables is shipped. `RESHIKI_INCHI_HELPER` accepts only an absolute, existing developer override; normal runtime never searches PATH, downloads a helper or invokes a compiler.

One process handles one immutable request. Bounded input/output pipes, a
120-second maximum deadline and `kill_on_drop` preserve timeout and cancellation
behavior. Heap exhaustion exits the child with a machine-readable diagnostic;
a later request starts a new process with a fresh budget. See
[ALLOCATION-AUDIT.md](ALLOCATION-AUDIT.md) for the exact scope.

## Protocol 3

Both directions use a 16-byte little-endian header: eight bytes `RSHINCHI`,
protocol `u16 = 3`, reserved `u16 = 0`, and JSON body length `u32`. Frames are
limited to 8 MiB, and InChI text to 2 MiB. Unknown fields, trailing bytes,
malformed UTF-8, invalid dimensions, indices and budgets are rejected.

Every molecular boundary checks graph and annotation dimensions, cached ring
indices and edges, stereo references and finite coordinates without recomputing
chemistry. The application runs the legacy generation preflight once to retain
its early-empty results and preparation errors. Transport and helper validation
do not repeat that preflight or construct discarded native input records.

Requests contain `heap_bytes` and an `operation`: `Generate` carries a detached
molecular state and optional coordinates; `Read` carries an InChI string and
sanitization/hydrogen-removal options. Responses contain the kernel `version`
and `result`, with either a typed generation/import result or an explicit
error. Generation retains status, identifier, message, log, auxiliary data and
adapter diagnostics. Import retains status, message, log, diagnostics, the
reconstructed state and unspecified-bond identities.

The dependency exposes molecules, not the old C arrays or warning bitmasks.
Protocol 2 is deliberately rejected; those unavailable records are never
fabricated. The old detached input/output adapters and their independent
fixtures remain useful development reference checks.

## Verification

```sh
RESHIKI_REQUIRE_INCHI_HELPER=1 python3 -m unittest tests.test_inchi_helper_protocol tests.test_inchi_helper_builder tests.test_inchi_distribution
cargo test --test inchi_rust
RESHIKI_REQUIRE_INCHI_HELPER=1 cargo test --features rdkit-reference --test inchi_generator --test inchi_reader --test native_import --test native_response
cargo clippy --workspace --all-targets --features rdkit-reference --locked -- -D warnings
```

`inchi_rust` compares 1,848 independent imported molecular states and 692
generation regressions captured from the official InChI 1.07.5 C kernel. The optional
live generation oracle compares 11,699 identifiers plus warnings, errors,
auxiliary data and diagnostics against the pinned original RDKit implementation.
The protocol tests exercise malformed frames, heap exhaustion, response bounds,
timeouts, early cancellation and blocked pipes. C sources are used only by
optional development tools that capture independent reference results; they
are not part of the Cargo/runtime helper build.
