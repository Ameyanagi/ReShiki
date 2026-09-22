# Public 2D layout reference contract

`depict_pipeline_reference.py` calls the original public `Compute2DCoords` API.
It never calls the application engine or the Rust solver. The checked fixture
contains inputs only; expected output is produced live on the tested host.

```sh
.venv/bin/python tests/depict_pipeline_reference.py --live \
  --output artifacts/depict-pipeline-native.json.gz
# Diagnose a stable case family:
.venv/bin/python tests/depict_pipeline_reference.py --live --filter fixed/5/
# Deliberately observe native process-local seed caches:
.venv/bin/python tests/depict_pipeline_reference.py --audit-seed-cache \
  --output artifacts/depict-pipeline-seed-cache.json.gz
# Read a saved capture with Python's standard library only:
python3 tests/depict_pipeline_reference.py --fixture artifacts/depict-pipeline-native.json.gz
```

Inputs come from RDKit 2026.03.6, commit
`0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985`. Generation requires that source tree:

```sh
.venv/bin/python tests/depict_pipeline_reference.py --generate-inputs \
  --rdkit-source /path/to/pinned/rdkit \
  --output tests/fixtures/depict-pipeline-inputs.json.gz
```

Source files, upstream license, NCI dataset, native libraries, observer and
trusted state adapters have SHA-256 provenance. Native replay records the actual
host and Python version. A capture from one ABI does not establish another ABI's
parity. Reference tools remain development dependencies.

Native runs on macOS ARM64, Linux x64 and Windows x64 each completed all 2,854
inputs: 2,758 successes, 96 defined exceptions, no nonfinite outputs and 135,744
coordinate scalars including preserved pre-error conformers. Original chemical
state, atom data and noncomputed properties were unchanged in every case. These
counts describe the native reference, independently of Rust replay.

The Rust solver matches all 2,758 successes and 96 typed failures on these three
hosts, including 130,722 exact output coordinate values. Forward and reversed
coordination requests also match. The default-options path is checked against
native import defaults: canonical orientation enabled, ring templates disabled.
The separate atomicity test covers invalid lengths, missing ranks, out-of-range
anchors and exhausted work budgets. Application routing remains separate.

## Rust comparison interface

Each JSONL record has a stable `name` and these fields:

| Field                                    | Meaning                                                                                                                  |
| ---------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `state`                                  | Original molecule before public layout, using `perception::State`                                                        |
| `atom_data`                              | Original hybridization, CIP rank and chiral rank                                                                         |
| `options`                                | Canonical orientation, templates, optional bond length and coordinate map                                                |
| `before_conformers`, `before_properties` | Original IDs, 2D/3D flags, coordinates and noncomputed properties                                                        |
| `expected`                               | Native success/error, returned conformer ID, all resulting conformers, original-molecule state, atom data and properties |
| `pickle`                                 | Independent native replay transport; never an input to Rust                                                              |

Coordinates, including z, and explicit bond lengths use 16-character IEEE f64 hex
strings. `bond_length:null` omits the native keyword. `coordinates:null` omits
`coordMap`; `coordinates:[]` supplies an explicitly empty map. Nonempty maps use
`[atom_index,x_hex,y_hex]` entries. Other options are fixed to `clear_confs:true`,
`force_rdkit:true`, and zero samples/flips.

Compare each successful conformer's exact coordinate bits, ID and 2D/3D flag.
Compare all original chemical state, atom data and properties after the call.
`expected.state` describes the original molecule, **not** the internal clone
returned by the initial-layout stage. Publishing that clone could accidentally
change stereo/cache properties; this contract detects that mistake.

A native exception requires a typed Rust failure with no published coordinate,
chemical-state or property changes. Exception text is diagnostic, not a required
Rust error string. A native success with nonfinite output is explicitly flagged
`finite:false`; it must become an atomic Rust error, never accepted coordinates.
No tolerance, alignment, rescaling or post-layout cleanup is part of comparison.

## Coverage and boundaries

The 2,854 inputs include every 578 built-in template graph, 116 corresponding
layouts with templates disabled, 833 molecules spread across the bundled NCI
file, 42 NCI cleanup constraints, 492 curated fixed-map cases, 208 scale cases,
CX coordinates/labels/properties, tetrahedral/enhanced/double-bond stereo,
SP/TBP/OH coordination, dative bonds, aromatic and bridged/spiro/macrocyclic
rings, disconnected graphs, long chains, and many components. Empty molecules,
absent/empty/single/multiple/all fixed maps, atom permutations and missing
original ring caches are included. Built-in skeletons retain the native reference
preparation even if a current Rust stage does not accept them.

The final 104 cases add 72 hydrogen, quadruple and partial-bond layouts, 16
native unsigned-string rank conversions, and 16 branch/ring rank controls.
Bond cases cover isolated pairs, chains, branches, rings, templates on/off,
canonical orientation and cleanup anchors. Rank controls exercise
`_chiralAtomRank` without `_CIPRank`, then conflicting ranks to verify CIP
precedence. Invalid properties that native layout never reads belong to the
separate raw import-property tests; this harness uses a typed chemical snapshot.
The original 2,750 input records are unchanged.
All eight paired rank controls also verify the intended observable difference:
reversing fallback ranks changes geometry, while reversing them under fixed CIP
ranks preserves it.

## Rejection categories

`expected.rejection_kind` is null on success. Native exceptions retain their
original type and message, with a stable category for comparison:

| Category                        | Original 2,750 cases | Native cause                                                                                                         | Required Rust rejection                                     |
| ------------------------------- | -------------------- | -------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------- |
| `zero_length_vector`            | 83                   | Coincident fixed coordinates fail normalization while preparing the coordinate seed                                  | `Initial(Seeds(Geometry(Numeric)))`                         |
| `missing_fragment_neighbor`     | 13                   | Cleanup anchors leave a single-anchor fragment merge without a neighbor; native `-1` becomes atom index `4294967295` | `Initial(Invalid("missing single-anchor merge neighbors"))` |
| `unclassified_native_exception` | 0                    | A new exception requiring investigation                                                                              | Fail the comparison until its cause is checked              |

These stages are supported by the original coordinate-map constructor and
attachment preparation (`EmbeddedFrag.cpp:74–101,313–352`), single-anchor merge
(`EmbeddedFrag.cpp:715–748,1142–1195`), and `Point2D::normalize` (`point.h:365–371`). The
Rust error census agrees on all 96 cases. Compare the typed variants above;
resource limits, malformed input, nonfinite input and unrelated invariant
failures must not count as matching native rejections. The classifier excludes
platform-specific file paths, source lines and Boost-version text.

Requests are limited to 1,024 atoms, 4,096 bonds, finite coordinates within 1e8,
and positive explicit lengths from 1e-12 to 1e6. Invalid indices, duplicate fixed
atoms, NaN/infinity, malformed transport and unsupported sampling are rejected
before native execution. These restrictions are harness boundaries, not evidence
that the original C++ API safely rejects every malformed input.

Transport limits: 8,192 records, 2 MiB per record, 64 MiB expanded input and
256 MiB output. Each worker handles at most 64 records with a 60-second timeout;
a complete live run has a ten-minute deadline. Native crashes or timeouts fail
the harness; they are never recorded as ordinary chemical rejections.

## Native process state and required Rust policy

The original wrapper changes global `BOND_LEN` for positive `bondLength` and
restores it only on success (`Wrap/rdDepictor.cpp:48–60`). After any exception,
the observer exits immediately and remaining requests resume in a fresh worker.
The checked `sequence/custom-length-failure` followed by
`sequence/default-after-failure` also verifies that the latter equals a separate
ordinary default-length control. The Rust solver must pass that same sequence:
a failed custom-length request cannot affect the next request.

SP/TBP/OH seed functions initialize `static idealPoints` from the first
`BOND_LEN` (`RDDepictor.cpp:63–68,101–108,139–146`). Deliberate public-API tests
compare fresh calls at lengths 0.5/default/2.75 with forward and reverse sequences
in one process. Mac, Linux and Windows captures showed `[true,false,false]` against fresh controls
in both orders for all three geometries: later calls inherit the first seed
length. Raw sequence coordinates are retained in the separate audit output.

Ordinary live replay therefore starts each SP/TBP/OH-bearing request in a fresh
worker. It does not prewarm or reset the caches. Other requests may share a worker
until the next coordination seed or native exception. Expected output means
fresh-process semantics; it is not an assertion that native output is stateless.
The Rust solver uses per-request ideal lengths matching the requested style,
verified against fresh native calls and independent of request order. It borrows
the original chemical state and returns a detached conformer, leaving original
properties and coordinates with the caller on failure.
