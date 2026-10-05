# Deterministic depiction collision repair

`depict::collision` ports the three repair stages in RDKit 2026.03.6
(commit `0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985`). It is a detached library
stage; application cleanup routing is integrated separately.

- `Input::new(graph, metadata, rings)` validates graph and ordered ring records.
- `find(fragment, include_bonds)` returns ordered collisions and a detached
  fragment with native density updates.
- `apply(fragment, stage)` runs one repair stage; `repair(fragment)` runs bond
  and spiro flips, angle opening, then bond shortening.

Errors leave the caller's fragment, graph, stereo metadata and ring cache intact.
Repair accepts complete connected components, including an empty fragment.
Neighbor iteration follows bond insertion order; embedded atoms follow atom-index
order. Fixed flags, stereo bond exclusions, normals and density side effects
retain native behavior. Rejected flips reflect again, including floating-point
rounding; they do not restore a coordinate snapshot. Bond-axis endpoints alias
the mutable native atom map, so the port rereads them between reflections.

Each repair request has one work budget (at most 50 million operations), a
one-million collision-pair cap and a four-million-entry distance-cache cap.
Shortest paths use bounded iterative BFS; one-sided and ring walks use iterative
DFS. Lazy unit-edge BFS distance rows equal the native default Floyd–Warshall
integer distances and preserve the native disconnected sentinel, without an
unbounded square allocation. Nonfinite inputs or intermediate arithmetic fail.

## Independent reference

The observer calls the original wheel's `EmbeddedFrag` methods. A generated
header changes only one `private:` access marker; it does not replace native
function bodies. Windows omits the `totalDensity` DLL export, so the observer
uses its exact source fold; Linux and macOS check that fold against the native
function on every capture. The fixture records pinned source, library and observer hashes.
Boost headers must match the wheel's 1.85 ABI. The generator preserves input
floating-point bits and molecule pickles when replaying another platform.

The 1,634 cases cover chains, branches, stereo bonds, heteroatoms, aromatic,
spiro, fused, bridged and macrocyclic rings, disconnected components and salts.
Atom permutations, seven coordinate arrangements and no/some/all fixed atoms
exercise each stage and combined repair. Linux x64 replay passes 590,058 exact
scalar comparisons, 43,021 ordered collision pairs, 224 native exceptions and
6,236 changed states. Tests also reject malformed, incomplete, nonfinite,
overflowing and over-budget inputs without mutating their source.

```sh
.venv/bin/python tests/build_depict_collision_oracle.py \
  --rdkit-source /path/to/pinned/rdkit --boost-include /path/to/boost185
.venv/bin/python tests/depict_collision_reference.py \
  --oracle artifacts/depict-collision-oracle --rdkit-source /path/to/pinned/rdkit \
  --output tests/fixtures/depict-collision-linux-native.json.gz
CARGO_BUILD_JOBS=4 cargo +1.95.0 test --locked --test depict_collision -- --nocapture
```

For a second platform, add `--replay` and select its fixture output name.
Windows uses the x64 MSVC developer environment and `.exe` observer suffix.
No application runtime depends on these reference tools.

## Native arithmetic and scope

Verified macOS ARM64 Depictor instruction sites use the existing private
arithmetic helpers. Atom distances at `30a58–30a5c`, bond-midpoint distances at
`31344–3134c`, and terminal shortened-bond lengths at `35f60–35f70` and
`362a8–362b8` round `y*y` before `fmadd(x,x,...)`. Cross products at
`2e710–2e714`, `3171c–3172c` and `3312c–33130` round the negated second
product before the first-product accumulation. Ring shortening calls native
Point2D length through its vtable before component-wise division and scaling.
The dylib hashes and input/output bits are retained in the Mac fixture.
Mac and Windows native captures are recorded; Rust replay on those hosts remains
a separate validation step. No
rounding, coordinate alignment or epsilon is used as a parity condition.

Source: `EmbeddedFrag.cpp:1436–1497,1699–2310`,
`DepictUtils.cpp:373–478`, `Matrices.cpp:166–235,335–393`, and
`EmbeddedFrag.h:75–90`. Attribution is in `licenses/rdkit/NOTICE`.

All application `Compute2DCoords` calls omit `nSamples` and `nFlipsPerSample`;
the pinned defaults are zero. `engine/cleanup.py:122` additionally requests
fixed coordinates, bond length, `canonOrient=false`, `forceRDKit=true` and ring
templates. `RDDepictor.cpp:609–620` therefore selects deterministic repair.
Random flip sampling is outside this supported workflow and is not implemented.
