# Native 3D geometry distribution

ReShiki links its geometry backend into the application executable. The same
executable handles the bounded `--geometry-worker` protocol, so a calculation
runs in a disposable process while the editor retains ownership of its graph.
Portable archives contain one executable and the required attribution records.

The geometry core uses RDKit **2026.03.6**, revision
`0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985`, and Boost **1.92.0** headers. The
minimal source closure is checked into `native/geometry/vendor/`. Its manifest
pins both compressed archives and every source file with SHA-256. Cargo uses
CMake to verify and extract those local archives into its build output, then
links one static geometry library. This build needs no Python, installed RDKit,
installed Boost, source download, or generated runtime data directory. Cargo
dependencies must already be available for a fully offline Cargo invocation.

## Build requirements

Use the repository's Rust toolchain, CMake 3.18 or newer, a C++20 compiler, and
the platform's native build tools. The release and geometry CI jobs use native
runners for every target:

| Platform      | Rust target                 | Native tools                                 |
| ------------- | --------------------------- | -------------------------------------------- |
| macOS ARM64   | `aarch64-apple-darwin`      | Apple Clang, Xcode command-line tools, CMake |
| macOS Intel   | `x86_64-apple-darwin`       | Apple Clang, Xcode command-line tools, CMake |
| Windows x64   | `x86_64-pc-windows-msvc`    | Visual Studio MSVC x64, Windows SDK, CMake   |
| Windows ARM64 | `aarch64-pc-windows-msvc`   | Visual Studio MSVC ARM64, Windows SDK, CMake |
| Linux x64     | `x86_64-unknown-linux-gnu`  | GCC 11 or newer, Make, CMake, binutils       |
| Linux ARM64   | `aarch64-unknown-linux-gnu` | GCC 11 or newer, Make, CMake, binutils       |

macOS releases target macOS 14 or newer. Native Windows ARM64 releases require
Windows 11 on ARM. Select the matching Visual Studio compiler environment on
Windows; CI uses `.github/actions/setup-msvc`. Windows release CI sets
`RUSTFLAGS=-C target-feature=+crt-static`, and the geometry build matches this
with MSVC's static `/MT` runtime. Set the same flag for a manual Windows release
build. The archive verifier rejects a dependency on redistributable MSVCP or
VCRUNTIME DLLs.

Audit the local source without compiling:

```sh
uv run --no-project --isolated --python 3.12 python scripts/geometry_source.py
```

To run the geometry crate and native numerical checks on the current native
target, enable `RESHIKI_GEOMETRY_NATIVE_TESTS=1` and run:

```sh
cargo test --locked -p reshiki-geometry
```

The native test mode requires the Cargo host and target to match, then runs
CTest. The dedicated six-platform CI job explicitly selects the matching Rust
host and uses this setting. A standalone native build and
provenance report are also available:

```sh
uv run --no-project --isolated --python 3.12 python scripts/build_geometry_backend.py --output build/geometry-proof
```

## Release acceptance

`scripts/build_release.py` records the RDKit revision, Boost version, source
manifest hash, archive hashes, and static linkage in `build.json`. Verification
extracts the archive, checks the executable's architecture, preserves the
existing chemistry checks, and requires exactly one native executable.

The geometry check inspects native imports using `otool -L` on macOS,
`dumpbin /DEPENDENTS` on Windows, or `readelf -d` on Linux. It rejects RDKit,
Boost, and Python shared libraries, plus redistributable Windows C++ runtimes.
Normal operating-system libraries remain part of the platform requirements.

It then copies only the executable to a fresh directory whose paths contain
spaces and runs two framed ethanol generation requests: explicit MMFF94 and
explicit UFF. The process receives an empty `PATH`, no `PYTHONPATH` or Python
home, no dynamic-library search or preload overrides, unavailable checkout and
interpreter locations, and empty user-data locations. Both responses must have
the current protocol and RDKit version, all nine ethanol atom coordinates,
correct added-hydrogen parents, finite energies, plausible bond lengths, and
converged optimization. Creating a runtime payload fails verification.

This is an isolated launch and source/import audit. It does not itself block
network access. The backend has no runtime download or interpreter path.

The release workflow performs this acceptance on macOS ARM64/Intel, Windows
x64/ARM64, and Linux x64/ARM64. A local pass verifies only the platform on which
it ran; the other platform results require the corresponding CI jobs.

## Worker framing

Each request and response contains a 16-byte header followed by one JSON body:

| Bytes | Meaning                                              |
| ----- | ---------------------------------------------------- |
| 0–7   | ASCII `RSHGEOM1`                                     |
| 8–9   | Little-endian unsigned protocol version, currently 1 |
| 10–11 | Reserved zero bytes                                  |
| 12–15 | Little-endian unsigned JSON byte length              |

The request JSON contains `heap_bytes` and `operation`; `operation` is the
native geometry request with its own `Generate`, `Relax`, or `Evaluate` field.
The response contains the pinned RDKit `version` and a Rust-style
`result` object containing `Ok` geometry or an `Err` message. Complete frames,
including the header, are limited to 4 MiB. A frame cannot contain trailing
bytes. The normal calculation deadline is 60 seconds, with a maximum of
120 seconds.

`heap_bytes` budgets Rust allocator activity during the calculation. RDKit's
C++ native allocations are not counted, so this is not a total-process memory
cap. Separate limits on original atoms (512), bonds (2,048), total coordinates
(4,096), conformers (32), iterations (10,000), frame size, and the child-process
deadline bound the workload and keep it isolated from the editor.

## Independent development oracle

After building the application, the locked development RDKit environment can
compare native energies and analytic gradients with RDKit's public Python API:

```sh
RESHIKI_GEOMETRY_REFERENCE_APP=/absolute/path/to/reshiki \
RESHIKI_REQUIRE_GEOMETRY_REFERENCE=1 \
uv run --locked python -m unittest tests.test_geometry_reference
```

The oracle requires RDKit 2026.03.6 and checks MMFF94, MMFF94s, and UFF on
organic, aromatic, charged, tetrahedral, and alkene fixtures, plus fixed atoms
and UFF-only boron parameters. It compares values at identical Cartesian
coordinates: energy tolerance is `1e-6 + 1e-8*abs(E)` kcal/mol, and each gradient
component tolerance is `1e-5 + 1e-7*abs(g)` kcal/(mol Å). These checks validate
local optimization and reference parity; they do not establish a global energy
minimum. Python belongs to this development oracle. The application calculates
through its native worker.

Release CI requires these comparisons on the five platforms with a pinned
RDKit wheel. Intel macOS retains native numerical and distribution checks
because the pinned Python wheel is unavailable there.

The source archives retain upstream terms. RDKit's BSD 3-Clause and Boost's
Boost Software License attribution are included in the consolidated release
notices; original bridge, validation, and packaging code follows the project's
`MIT OR Apache-2.0` license.
