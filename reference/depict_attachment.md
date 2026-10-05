The current ABI arithmetic policy and strict platform checks are documented in
[depict_numeric.md](depict_numeric.md). The historical cross-native differences
below describe platform build differences; same-platform Mac comparisons now
require exact bits, just as Linux comparisons do. Live replay is always strict.

This library checkpoint ports `EmbeddedFrag(unsigned atom, molecule)`,
`setupNewNeighs`, `updateNewNeighs`, `addNonRingAtom` and their direct helpers from
RDKit 2026.03.6, commit `0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985`. It reuses the
ordered `Fragment`/`EmbeddedAtom` representation from ring construction. There is
no runtime dispatch or new dependency.

`depict::attachment::Input::new` borrows a validated `Graph` and a matching array
of `AtomData { hybridization, cip_rank, chiral_rank }`. `single_atom`,
`setup_neighbors`, `update_neighbors` and `add_non_ring_atom` return new fragments.
`ranked_atoms` exposes the native ascending/descending order. Graph, properties
and fragment inputs remain unchanged on both success and failure.

Bond insertion order supplies adjacency. Neighbor setup initially appends H
neighbors after other elements, then applies the native rank/degree policy.
High-degree atoms swap the third- and second-last ranked positions and rotate
around the last already embedded neighbor in source adjacency order. Attachment
points are collected in ascending embedded-map order and then ranked. Rank
properties use `_CIPRank` first, then `_chiralAtomRank` (the lowercase-c literal
behind `common_properties::_ChiralAtomRank`), then element/degree/index fallback.
Unsigned 32-bit subtraction/addition/multiplication and the final signed 32-bit
pair conversion are preserved, including wrapping and ties by atom ID.

An update with no remaining neighbors retains the native stale attachment entry;
a full setup clears and rebuilds the list. The no-angle attachment helper leaves
`EmbeddedAtom.aid` at its native default zero, so the map key is authoritative.
The angle helper sets the new aid normally. These are internal depiction states,
not atom identity changes in the chemical graph.

Attachment preserves the source angle branches, rotation direction, anchor
updates, collision-count mirror choice and normal normalization. The no-angle
branch reads hybridization and degree, and reverses normal/clockwise sense when
adding the alternate cis/trans substituent. Rotation about a point composes two
complete 3×3 matrix products, including zero-initialized accumulation. Replacing
this with a simplified translation changes signed zeros and rounding. Density,
fixed flags, completion state and box extents of existing atoms/fragments are
preserved; newly added atoms get the source defaults.

Limits are 100,000 atoms, 300,000 bonds, 600,000 stored pending-neighbor entries
and 50 million charged traversal/ranking units per operation. `with_work_limit`
can lower the latter. External indices, property-array sizes, duplicate pending
neighbors and attachment points, rotation flags and finite geometry are checked.
Adding an already embedded atom, a nonbonded/pending-missing neighbor or a missing
angle anchor is an `Invalid` error. Nonfinite input is distinct from numeric
failure. Zero/tiny normalization failures become `Geometry(Numeric)`; a too-short
incoming normal is `Invalid`. No partly mutated fragment escapes an error.

The independent C++ helper calls the pinned native APIs directly and records the
initial fragment and each setup/update/add step. Native single-atom construction
is compared directly. Ring and cis/trans constructors provide explicit fragment
inputs for this attachment test; their outputs are not recomputed by Rust to
manufacture expected attachment values. Every successful operation is repeated,
all state fields and neighbor/attachment order are compared, and complete Rust
input snapshots are checked for mutation.

The corpus has 970 cases, 10,183 successful operation steps, 10,691 compared
fragment snapshots, 47,274 atom states and 326,408 scalar values. Cases cover
chains, branches, rings with substituents, fused/spiro/bridged rings, aromatic and
explicit-H graphs, tetrahedral and non-tetrahedral tags, cis/trans control and
alternate-substituent branches, all nine hybridizations, degrees 1–8, rank
precedence/ties/wrapping, varied and negative bond lengths, atom renumbering and
nondefault retained metadata. A 45,001-atom graph exercises fallback-rank unsigned
overflow. Three direct native exception cases cover zero normal and zero/tiny
bond lengths. Native may partly mutate its object before throwing; Rust checks
the corresponding error and preserves the caller input. Further malformed-index,
nonfinite, duplicate and storage/work-limit cases are checked safely in Rust.

Linux x86_64 Rust 1.95 matches every recorded scalar bit, including signed zeros,
and every independent drawing-space f32 projection. Both fixture comparison and
live same-host C++ replay pass. The four tests take about 6.3 seconds with fixtures
and 6.9 seconds with live replay on the isolated validation host. Existing geometry
and ring tests also pass after adding the shared rotation helper.

The Linux-versus-macOS ARM64 fixture comparison is a separate numerical audit. All discrete metadata and
ordering match. There are 30,874 different f64 scalars and 845 different projected
f32 scalars. Maximum absolute f64 difference is `1.7763568394002505e-15`; maximum
relative difference is `2.0` at nominal zero. Relative difference is
`abs(a-b)/max(abs(a),abs(b))`. Projection independently computes `x*28` or `-y*28`
and converts to f32; its maximum difference is `4.973799150320701e-14`.

The largest difference is the y coordinate of atom 5 after the last addition in
`molecule/9/0/0/3.25`: source-order Rust gives `-3.25000000000000089`, while the
macOS native library gives `-3.25000000000000266`. This path repeatedly rotates,
normalizes and extends the tetrahedral chain `C[C@H](F)[C@@H](Cl)Br`. Native ARM64
`addAtomToAtomWithNoAng` contracts both the incoming normal length-squared check
and final normalization length into `fmul`/`fmadd` sequences; the latter is
followed by `fsqrt`. Rust now follows the verified ABI policy at these sites. Native
point transforms and platform libm also differ between platforms. No alignment, zero cleanup or
epsilon converts these differences into an exact-parity claim.

Current tests require same-platform native bit equality and retain cross-native
raw audits. The former frozen mismatch-count guard has been removed.

For observation of the private native attachment list, the builder copies
`EmbeddedFrag.h` into an isolated generated include directory and changes exactly
the single ` private:` line to ` public:`. No member declarations, order, layout,
inline body or linked-library code changes. Controlled nondefault test inputs are
set through this exposed header. Original header SHA-256 is
`a45692bbffd2dff60aa608565dc98d366a2aae7cfb75eeb372f1cf35a8f3f0f4`; the observation
header SHA-256 is `bf49eb06e2296abcf79f215ebb147930344171b565386835fc6dc3ec701dc7a2`.
The generated header is used only for the attachment helper, not other helpers.

Generate in an isolated checkout using the pinned wheel, source and Boost headers:

```sh
CC=/usr/bin/clang CXX=/usr/bin/clang++ .venv/bin/python \
  tests/build_depict_geometry_oracle.py --component attachment \
  --rdkit-source /path/to/pinned/rdkit --boost-include /path/to/boost185

.venv/bin/python tests/build_depict_geometry_oracle.py --component attachment \
  --rdkit-source /path/to/pinned/rdkit --boost-include /path/to/boost185 --replay \
  --output tests/fixtures/depict-attachment-linux-native.json.gz
```

Windows x64 generation and fixture hashes are in the numeric audit. Use
`tests/build_depict_windows_oracle.py --component attachment`.

For optional direct native replay, set `RESHIKI_DEPICT_ATTACHMENT_ORACLE` to the
helper executable, `RESHIKI_RDKIT_SOURCE` to the pinned source tree and the wheel
library directory in the platform loader path. Live replay always requires bit equality. Run
`cargo +1.95.0 test --locked --test depict_attachment -- --nocapture`.
Each fixture records nine original source hashes, the observation-header hash,
native library hashes, helper executable hash and platform. Compressed/JSONL
fixture SHA-256 values follow:

- macos: `ed25049a46259ae391c401b71960d387d0f65f731a5fa30c5e478d9bba842b31`,
  `c217704389b66b0f2a29772b8d7b67685e10e39332b6c951873f1344c09c2a08`.
- linux: `374efbeb513247442f863d8d818eefb0a9e9c85b735b57da00c8d4d490a95abd`,
  `5678772a935f8e2f18f7a65c91b8b072e244e6c98e9efa4659dd9ee760ded6c6`.

Whole `expandEfrag`, other initial fragment constructors, merging of independent
fragments, collision optimization, constrained layout and runtime dispatch remain
separate stages. In particular, native whole layout initializes square-planar,
trigonal-bipyramidal and octahedral stereochemistry with separate coordination
constructors before expansion. Passing those tags to this low-level single-atom
constructor intentionally matches only that constructor; it is not a substitute
for their full depiction. Attribution, including the ordered matrix product from
`Numerics/SquareMatrix.h`, is in `licenses/rdkit/NOTICE` under BSD-3-Clause.
