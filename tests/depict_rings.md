The current ABI arithmetic policy and strict platform checks are documented in
[depict_numeric.md](depict_numeric.md). The historical cross-native differences
below describe platform build differences; same-platform Mac comparisons now
require exact bits, just as Linux comparisons do. Live replay is always strict.

This checkpoint implements RDKit 2026.03.6's three ordered ring-selection helpers
and `EmbeddedFrag(molecule, ordered_rings, false)`. It uses commit
`0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985`. The independent C++ helper calls those
four native APIs directly. It observes all public EmbeddedAtom fields, fragment
completion state and initial box values; it does not run final `Compute2DCoords`.

`depict::rings::Input::new` borrows an explicit `Graph`, `Metadata`, `RingCache`
and an ordered list of selected cache indices. `pick_first`, `core` and `next`
return indices within that selected list. `source_ring_ids` retains the original
cache indices. `embed_without_templates` returns the initial ordered atom map,
including coordinates, normals, neighbor anchors, angle, clockwise state,
rotation direction, unembedded neighbors, density and fixed flags.

The cache's ring kind is not read by the native no-template constructor. Selected
cycles must contain distinct, valid atoms in bond traversal order. Duplicate
selections remain supported. Invalid topology, disconnected constructor inputs,
empty constructor input, numeric failure and resource exhaustion are checked
errors. The input graph, stereo metadata and cache remain unchanged.

The source stages are regular polygon embedding, ordered trans-bond mirroring,
first-ring choice, shared-atom selection, one- or two-anchor alignment, density
reflection and metadata merging. The first ring minimizes the number of atoms
with degree greater than two; ties prefer the largest ring and then the first
input ring. Core pruning removes one ring per pass and preserves the source's
two remembered intersection atoms. Next-ring selection immediately prefers
exactly two shared atoms; otherwise it maximizes overlap and rotates wrapped
shared chains. Replacing these rules with set sorting changes native behavior.

The no-template constructor does not call `setupNewNeighs`; RDDepictor invokes
that afterward. Neighbor arrays are observed empty, and the attachment list is
initialized empty by the constructor's source. The native class has no public
attachment-list getter, so that one invariant is checked separately in Rust.
Normals, angle updates and clockwise flips are preserved during transforms and
merges. Bonds' original stereo controls determine trans-ring mirroring; no stereo
cleanup, ranking, ring perception or cache recomputation is performed.

Work is bounded by 50 million charged operations per method, selectable downward
with `with_work_limit`. At most 100,000 selected rings and one million selected
ring-atom entries are accepted, within the existing 100,000-atom/300,000-bond graph
bounds. Constructor merges additionally require selection progress. Bounded
containers and checked lookups replace native assumptions about valid cycles.

Each native fixture contains 1,338 cases and 1,042 complete constructors over
9,442 embedded atom states. The fixtures include ordinary, aromatic, fused,
spiro, bridged and cage systems; atom renumbering; rotated/reversed ring traversal;
selection permutations and duplicates; empty/complete/partial selections;
different bond lengths; trans macrocycles; and 80 explicit E/Z/cis/trans metadata
and adjacent-control variants. All selections and observed integer/default
metadata agree with both pinned native builds. Every constructor is repeated,
and the complete input state is checked for mutation.

Linux x86_64 Rust 1.95 matches all **60,820 scalar values exactly**, including
signed zeros, and every independently projected drawing-space f32 value. Both
checked-in fixture and live native replay comparisons pass. The fixture-based
two-test run takes approximately 1.5 seconds on the validation host; live replay
takes approximately 1.7 seconds.

Linux-versus-macOS ARM64 native is audited separately. Its FMA/libm results differ in 3,450
f64 scalars and 174 projected f32 scalars. Maximum absolute coordinate error is
`2.11232904945291e-7`; maximum relative error is `1.9999999625361358` near nominal
zero. Relative error uses `abs(a-b)/max(abs(a),abs(b))`. Projection uses `x*28`,
`-y*28`, then f32 conversion; its maximum difference is `7.62939453125e-6`.
No epsilon, coordinate alignment or rounding converts those results into an
exact-parity claim.

The largest difference is `traversal/30/1/0/1`, atom 2's x coordinate, for a
multiply fused steroid. Independent native prefix-constructor probes locate the
amplification in the first two-anchor alignment. The input vectors are opposites:

```text
r = (-0.7499999999999994,  1.299038105676658)
p = ( 0.7499999999999994, -1.299038105676658)
```

Unfused `p.x*r.y - p.y*r.x` is zero. The macOS library contracts this expression
into `fnmul`/`fmadd`, producing `-5.497719261874257e-17`. The normalized dot product
is `-0.9999999999999998`, so `acos` gives an angle slightly below pi, and the tiny
cross-product sign selects opposite rotations. The next native anchor's y
residual is `+3.161013717445371e-8` on Linux and `-3.161013628627529e-8` on macOS.
Later merges amplify that difference. Rust now reproduces each pinned ABI
operation policy; the source-order difference remains visible across platforms.

Build and regenerate in an isolated checkout, using the pinned Python wheel,
matching RDKit source headers and Boost headers:

```sh
# macOS, with Apple clang:
CC=/usr/bin/clang CXX=/usr/bin/clang++ .venv/bin/python \
  tests/build_depict_geometry_oracle.py --component rings \
  --rdkit-source /path/to/pinned/rdkit --boost-include /path/to/boost185

# Linux: replay identical input bits and pickles, preserving the original corpus.
.venv/bin/python tests/build_depict_geometry_oracle.py --component rings \
  --rdkit-source /path/to/pinned/rdkit --boost-include /path/to/boost185 --replay \
  --output tests/fixtures/depict-rings-linux-native.json.gz
```

Windows x64 generation and fixture hashes are in the numeric audit. Use
`tests/build_depict_windows_oracle.py --component rings`.

For live Rust-test replay, set `RESHIKI_DEPICT_RINGS_ORACLE` to the native helper,
`RESHIKI_RDKIT_SOURCE` to its pinned source tree, and the wheel's native library
directory in `DYLD_LIBRARY_PATH` (macOS), `LD_LIBRARY_PATH` (Linux) or `PATH`
(Windows). Live replay always requires raw bit equality. Run `cargo +1.95.0 test --locked --test depict_rings -- --nocapture`.

Each fixture's first JSON line records all seven source hashes, helper executable
hash, native library hashes and platform. Compressed and uncompressed SHA-256:

- macOS: `c40da64d88b013327be1528b9a2d23c6c36040ba784ae07cbc6f080cf7fd45fe`,
  `55c34a0324a05649a09b54a8bff86b620e2d3876ccdb9e839e245fbc56d7f53f`.
- Linux: `a7c21135d8513b3d4a2571ff4f5b4ca5c77c89b46e8b9a85dcaae7c7e0bdc810`,
  `2e944eedc3e0aae02be1da3ada05b05073b159d1f24436bdfcc2b25a11b5b5d8`.

Template matching, post-constructor neighbor ordering, fragment expansion,
collision correction, final orientation and packing remain later layers. Cleanup
still requires its template-enabled path before any runtime replacement. This
checkpoint adds no runtime dispatch or dependency. Attribution is recorded in
`licenses/rdkit/NOTICE` under the accompanying BSD-3-Clause license.
