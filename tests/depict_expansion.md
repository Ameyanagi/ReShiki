This library ports the pinned native `computeInitialCoords` stage and its
fragment expansion/merging operations. Collision correction, canonical
orientation, packing, conformer creation, one-coordinate translation and runtime
layout dispatch remain separate stages. None is substituted with a simpler layout.

`expansion::compute_initial` borrows a chemical `perception::State`, explicit
optional chiral ranks and an optional coordinate map. It returns a detached
`Initial` containing pre-stereo depict ranks, the complete post-initial chemical
state/ring cache and surviving fragments in native list order. The original
chemical state is unchanged. The caller retains the original coordinate-map
option, including the distinction between absent, empty and one-point maps, for
later finalization. The lower `Input` API exposes seed, direct merge and expansion
observations without rerunning chemical preparation.

The source order is depict ranks, symmetric rings including dative bonds, legacy
stereochemistry with clean=false/force=false, coordinate-map seed when size>1,
ordered fused/spiro ring systems, non-tetrahedral seeds, non-ring double-bond
seeds, nonembedded atoms, then repeated first-prespecified/largest/minimum-rank
fragment expansion. More than one mapped atom in a ring system disables its
ring-template path. Coordination ideal lengths remain explicit static-constructor
values, defaulting to 1.5; the current bond length is independently supplied.

Merges preserve the transformed incoming fragment, extended common-ID order,
fixed flags, normal transformation, clockwise/rotation metadata, pending-neighbor
order and native map-default insertion for a valid but absent stereo-control ID.
Source observations include those values before and after direct pair merges and
whole-pool expansion. The original native one-anchor failure can also insert the
invalid unsigned key 4294967295; the safe API returns a typed error instead.

## Bounds and failure atomicity

Every public operation returns detached data. Internal mutation avoids cloning
the growing fragment once per atom. One 50-million-unit work allowance is shared
through chemical ring/stereo preparation and ring/template/seed/attachment calls
during expansion. The private `compute_initial_with_work` entry accepts the
solver's remaining allowance; collision index construction, collision passes and
finalization accept that same counter. Wrappers debit completed work on both
success and failure, preserve caller allowance above a local cap, and never
replenish it between stages. The stereo adapter requires the freshly computed
symmetric cache, so its independent fallback ring searches are unreachable.
Ordinary public ring and stereo perception keep their original budgets.
A two-million-slot
allocation allowance counts new atom records, neighbor entries and attachment
entries before persistent insertion/copy; it is monotonic, so dropping a fragment
does not replenish it. Primitive constructors retain their own graph and temporary
storage caps. Ring-component traversal uses an explicit stack. Over-limit,
invalid-ID, duplicate-ID, missing-anchor and nonfinite failures publish no fragment.

Tests include a 20,000-atom chain, cumulative seed work, repeated high-degree
fragments exceeding aggregate storage before cloning, bad coordinate IDs,
nonfinite coordinates, repeated common/pending IDs, invalid master fragments and
failed partial expansion with unchanged source data. Existing public primitive
wrappers keep detached behavior and their normal numeric results. Six focused
unit tests additionally verify exact-total versus total-minus-one pipeline work,
preparation early returns, failure debits, excess allowance, and ring constructor
continuations with intervening work.

## Independent original-native capture

Source is RDKit 2026.03.6, commit
`0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985`:

- `Code/GraphMol/Depictor/RDDepictor.cpp`: `computeInitialCoords`, helper stage order.
  SHA-256 `f1ae9e705405d5254575c595c09c5d2ee240d4d2abf05209dd42dad5ae99b31f`.
- `Code/GraphMol/Depictor/EmbeddedFrag.cpp`: `expandEfrag`, `mergeWithCommon`,
  `mergeNoCommon`, transforms and reflection decisions.
  SHA-256 `a3c55426a09deb23443e53cb2b59b4056e9d312b8fdb1c749da5033bc5a1eb33`.
- `Code/GraphMol/Depictor/EmbeddedFrag.h`:
  SHA-256 `a45692bbffd2dff60aa608565dc98d366a2aae7cfb75eeb372f1cf35a8f3f0f4`.
- `Code/GraphMol/Depictor/DepictUtils.h`: `getAtomDepictRank`.
  SHA-256 `0e29dd5bcb355ef24c0d2b442ab4af0fc78b435cca1acdde795352bf4273ec45`.
- `Code/GraphMol/Aromaticity.cpp`: ring-neighbor map and ordered fused-ring DFS.
  SHA-256 `45bb00f7d472de1d91b39989112c5c18b0e3bd80fec8103ef59deac441fe76f3`.

These adaptations retain BSD-3-Clause attribution in `licenses/rdkit/NOTICE`.
The observer compiles a source-extracted `computeInitialCoords` with only capture
statements and a renamed function/signature. It calls original wheel helpers and
EmbeddedFrag methods. On Linux and Mac, each request also calls the wheel's original
`computeInitialCoords` separately; any observed final-fragment difference or
observer-only exception terminates capture rather than becoming a native error.
The header's private-to-public observation changes visibility only. No Rust code
participates in expected results. Headers must match the wheel's Boost 1.85 ABI.

The fixed corpus has 1,877 requests: 46 structures, atom permutations, fixed-map
variants, current-length variants, cached and unassigned stereo, CIP/chiral-rank ties and wrapping values,
100 consecutive unfiltered pinned NCI examples, long chains and disconnected
components. Linux replay has 1,698 finite successful initial results, 117 native
exceptions, 1,478 successful direct pair merges and 1,791 expansion transitions.
It compares 9,189 complete fragments and 379,062 finite f64 values, including
signed zero: zero unequal f64 bits and zero independently projected f32 differences.
Prepared chemical state, ranks, list order and all integer metadata also match.

There are **62 separately counted raw-stage restrictions**, not accepted parity:
61 original outputs contain key 4294967295 and one zero-length trans-ring output
contains nonfinite coordinates. Original full `compute2DCoords` rejects every one
with both canonical-orientation options. The fixture pins those complete outcomes.
The safe initial library rejects earlier and does not publish invalid fragments.

The same 1,877 requests were captured directly on macOS ARM64; both native initial
calls agree and the exception/restriction counts match. Linux, Mac and Windows
tests select their matching fixture and require exact f64 bits. Mac and Windows
Rust validation belongs to integration. Windows ARM64 replay remains a separate
requirement; an x64 native capture alone does not establish ARM64 parity.

The new Mac merge contractions were checked against actual pinned Depictor
instructions: cis/trans dot at `289c0/289c4`, third-point dots at
`28ea0/28ea4/28eac/28eb0`, and density norm at `27554/27558` (vector fmla).
They use the private arithmetic policy from `depict_numeric.md`; Linux remains
unfused. No epsilon, alignment or zero cleanup is permitted.

```sh
.venv/bin/python tests/build_depict_expansion_oracle.py \
  --rdkit-source /path/to/pinned/rdkit --boost-include /path/to/boost185
.venv/bin/python tests/depict_expansion_reference.py \
  --oracle artifacts/depict-expansion-oracle --rdkit-source /path/to/pinned/rdkit \
  --replay --fixture tests/fixtures/depict-expansion-linux-native.json.gz \
  --output artifacts/depict-expansion-replayed.json.gz
CARGO_BUILD_JOBS=4 cargo +1.95.0 test --locked --test depict_expansion -- --nocapture
```

The builder chooses Apple clang++ on Mac and records original source hashes.
Capture provenance additionally records observer/builder, executable, linked wheel
libraries and NCI dataset hashes. Optional `DEPICT_EXPANSION_ORACLE` plus
`DEPICT_EXPANSION_SOURCE` runs the test against a live observer and always enforces
exact native values.

## Windows x64 source-adapter observation

The pinned Windows DLL exports the EmbeddedFrag operations, including expansion
and both merges, but does not export `computeInitialCoords` or its DepictorLocal
helpers. `build_depict_expansion_windows_oracle.py` generates an observer-only
adapter from these exact RDDepictor.cpp byte ranges:

| Source section                               | Byte range      | Verbatim SHA-256                                                   |
| -------------------------------------------- | --------------- | ------------------------------------------------------------------ |
| DepictorLocal helpers through `_shiftCoords` | `[1068,12454)`  | `2ac60d86ff6a22f437ce4f222212fa8b716db102fcbba19c9b2a3c00a54eddaa` |
| `computeInitialCoords`                       | `[12726,15699)` | `c24f2050866c89924472011f083daec4a9781216b1cae9c6052046216174f1ba` |
| `copyCoordinate`                             | `[15699,16729)` | `518e6b4134c503c841fd2c0e5d7c4d83e9c48d2c4e0a64d7283694736aa1c571` |
| Complete parameter-struct wrapper            | `[18810,21400)` | `4dbb3015ebe0825b76256e8f4e3329ee8d18b006ce68db9d815842edcc901f93` |

Only function names/calls needed to distinguish the adapter are renamed. The
builder verifies the original file hash, asserts replacement counts, and records
verbatim and compiled-section hashes. It also verifies that the binary imports
both original `compute2DCoords` and shared `BOND_LEN` from the wheel DLL. The
adapter has a distinct wrapper symbol; it cannot replace the imported public
function. Export tables, import table, generated header, full header tree,
Boost 1.85 header, compiler command/log, adapter and wheel library hashes are
recorded. The capture used x64 MSVC 19.43.34809.0 with `/O2 /MD` on physical
Windows x64. No Rust, native library edits or system installation are involved.

Every one of the original 1,877 inputs calls both the original public
`compute2DCoords` and the extracted complete wrapper, for canonOrient=false and
true with forceRDKit=true and the same shared BOND_LEN. Any outcome or conformer
ID/3D-flag/coordinate-bit mismatch terminates the observer. The checks passed
3,368 successful conformers, 386 error outcomes and 93,546 coordinate scalar
bits. The fixture records both original public outcomes for every input,
including the 117 initial-stage exceptions. Fourteen additional inputs have
finite initial results but fail in subsequent original layout stages; they are
retained as full-layout error witnesses. All 62 raw initial restrictions still
fail at both original full-layout boundaries.

**Windows raw initial-stage values are source-adapter observations validated at
the independent original public boundary.** Agreement between the uninstrumented
and instrumented initial adapters is an additional observation check, not the
independent validation. All fragment merge and expansion calls still execute the
original exported native methods. The input JSONL hash pins the unchanged Linux
corpus; no cases are filtered or regenerated from Windows-specific chemistry.

```bat
python tests\build_depict_expansion_windows_oracle.py ^
  --rdkit-source F:\isolated\rdkit --boost-include F:\isolated\boost185
```

The generator scopes crash-dialog suppression to its process and restores it.
Its build manifest also accompanies live native replay, so the same full-boundary
checks and strict Rust comparison apply when `DEPICT_EXPANSION_ORACLE` is set.

Windows fixture gzip SHA-256:
`09f282ffff0ad085cfd0960525d1f7bae2122e7f4c3fa44f1c119ff4e3f91c50`.
Uncompressed JSONL SHA-256:
`bf2fdad8bab0865498a34365c764172ebf55ba537f6a5c01419c8242b1efba23`.
