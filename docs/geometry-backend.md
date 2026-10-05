# Rust 3D geometry distribution

ReShiki links its Rust geometry backend into the application executable. That
same executable handles the bounded `--geometry-worker` protocol in a disposable
child process while the editor retains ownership of its graph. Portable archives
contain one executable and attribution records. No Python interpreter, chemistry
helper executable, RDKit shared library, or runtime parameter directory is needed.

The backend uses **COSMolKit 0.3.0** for ETKDGv3 conformer generation and
MMFF94, MMFF94s, and UFF energy, gradient, and minimization calculations. Its
complete Git revision is pinned in [the geometry manifest](../native/geometry/Cargo.toml)
and `Cargo.lock`. The opaque force-field adapter keeps calculation contributions
separate from their coordinate context; ETKDG uses the same ownership boundary
for its three- and four-dimensional fields.

COSMolKit's parameter-source reference is RDKit **2026.03.1**, revision
`351f8f378f8ad6bbd517980c38896e66bf907af8`. Its Rust build script reads embedded
parameter, atomic, and torsion data and generates Rust constants. Some data files
retain upstream `.cpp` names; they are not compiled as C++ or installed as runtime
files. ReShiki's independent development comparison uses RDKit **2026.03.6**,
revision `0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985`. A newer comparison reference
does not change the recorded origin of the parameter sources.

## Build requirements

Use the repository's Rust toolchain and normal native application build tools.
The geometry crate adds no CMake, C++ compiler, Boost, or Python requirement.
The release and geometry CI jobs select matching native Rust hosts:

| Platform      | Rust target                 |
| ------------- | --------------------------- |
| macOS ARM64   | `aarch64-apple-darwin`      |
| macOS Intel   | `x86_64-apple-darwin`       |
| Windows x64   | `x86_64-pc-windows-msvc`    |
| Windows ARM64 | `aarch64-pc-windows-msvc`   |
| Linux x64     | `x86_64-unknown-linux-gnu`  |
| Linux ARM64   | `aarch64-unknown-linux-gnu` |

macOS releases target macOS 14 or newer. Native Windows ARM64 releases require
Windows 11 on ARM. Select the matching Visual Studio environment on Windows;
CI uses `.github/actions/setup-msvc`. Windows release CI sets
`RUSTFLAGS=-C target-feature=+crt-static`; use the same flag for a manual release.
The archive verifier rejects redistributable MSVCP or VCRUNTIME dependencies.

Run the geometry tests with ordinary Cargo:

```sh
cargo test --locked -p reshiki-geometry
```

Optional developer scripts audit provenance and produce a separate build proof.
They do not become application dependencies:

```sh
uv run --no-project --isolated --python 3.12 python scripts/geometry_source.py
uv run --no-project --isolated --python 3.12 python scripts/build_geometry_backend.py --output build/geometry-proof --tests
```

`geometry_source.py --cosmolkit-source /path/to/pinned/checkout` additionally
checks the exact embedded data bytes without downloading sources.
`build_geometry_backend.py --offline` requires all locked Cargo dependencies
to be present in Cargo's cache. Its output is development evidence, not a
second executable distributed with ReShiki.

## Release acceptance

`scripts/build_release.py` records the Rust backend revision, original parameter
reference, independent comparison reference, seven parameter-file SHA-256 hashes,
and Cargo manifest/lock hashes in `build.json`. License packaging audits those
bytes in Cargo's already-resolved source. Verification extracts the archive,
checks executable architecture and existing chemistry checks, and requires exactly
one native executable.

The geometry check inspects native imports using `otool -L`, `dumpbin /DEPENDENTS`,
or `readelf -d`. It rejects RDKit, Boost, and Python shared libraries and Windows
C++ runtime redistributables. Normal operating-system libraries remain platform
requirements.

It copies only the executable to a fresh directory whose paths contain spaces and
runs framed ethanol generation requests with explicit MMFF94 and explicit UFF.
The child receives an empty `PATH`, no Python environment, no dynamic-library
search/preload overrides, unavailable checkout/interpreter locations, and empty
user-data locations. Responses must contain the expected protocol and comparison
version, all nine ethanol atom coordinates, correct added-hydrogen parents, finite
energies, plausible bond lengths, and converged optimization. Creating a runtime
payload fails verification. Release admission requires an actual executable;
missing application paths cannot turn this check into a skip.

This isolated launch and import audit does not itself block network access.
The backend has no runtime download or interpreter path. The release workflow
performs acceptance on all six targets; a local pass establishes only its host.

## Worker framing and limits

Each request and response has a 16-byte header followed by one JSON body:

| Bytes | Meaning                                     |
| ----- | ------------------------------------------- |
| 0–7   | ASCII `RSHGEOM1`                            |
| 8–9   | Little-endian protocol version, currently 1 |
| 10–11 | Reserved zero bytes                         |
| 12–15 | Little-endian unsigned JSON byte length     |

The request contains `heap_bytes` and an `operation` with `Generate`, `Relax`,
or `Evaluate`. The response retains RDKit **2026.03.6** in its `version` field
as the independent comparison reference and contains `result.Ok` geometry or
`result.Err` text. That field is not the Rust backend version. Complete frames
are limited to 4 MiB and cannot contain trailing bytes. The normal calculation
deadline is 60 seconds, with a maximum of 120 seconds.

`heap_bytes` budgets Rust allocator activity during calculation, including the
Rust geometry core. It is not a total-process memory cap: stack and operating
system allocations are separate. Limits on original atoms (512), bonds (2,048),
all-atom coordinates (4,096), conformers (32), iterations (10,000), frame size,
and process deadline bound each detached calculation. Live relaxation also
pauses after 50 batches for unchanged physical constraints or eight batches
without meaningful energy improvement; the preview remains editable.

## Independent development oracle

After building the app, the locked development RDKit environment compares
energies and analytic gradients with RDKit's public Python API:

```sh
RESHIKI_GEOMETRY_REFERENCE_APP=/absolute/path/to/reshiki \
RESHIKI_REQUIRE_GEOMETRY_REFERENCE=1 \
uv run --locked python -m unittest tests.test_geometry_reference
```

The oracle uses RDKit 2026.03.6 and compares MMFF94, MMFF94s, and UFF on organic,
aromatic, charged, tetrahedral, and alkene fixtures, plus exact fixed atoms and
UFF-only boron parameters. Values are compared at identical Cartesian coordinates:
energy tolerance is `1e-6 + 1e-8*abs(E)` kcal/mol; each gradient component tolerance
is `1e-5 + 1e-7*abs(g)` kcal/(mol Å). These checks establish the tested numerical
agreements and local minimization, not a global energy minimum or universal
equivalence between conformer generators. Python is used only for development
comparisons. Release CI requires them on the five targets with a pinned RDKit
wheel; Intel macOS retains Rust numerical and actual-executable acceptance.

COSMolKit's MIT, RDKit's BSD 3-Clause, and original parameter copyright notices
remain in the consolidated release notices. Original ReShiki adapter, validation,
and packaging code follows `MIT OR Apache-2.0`; see [the geometry notice](../licenses/geometry/NOTICE).
