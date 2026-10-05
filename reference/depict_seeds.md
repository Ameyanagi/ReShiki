The current ABI arithmetic policy and strict platform checks are documented in
[depict_numeric.md](depict_numeric.md). The historical cross-native differences
below describe platform build differences; same-platform Mac comparisons now
require exact bits, just as Linux comparisons do. Live replay is always strict.

This library checkpoint implements the initial square-planar, trigonal-bipyramidal
(TBP), octahedral, coordinate-map and cis/trans fragments from RDKit 2026.03.6,
commit `0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985`. The C++ reference calls the actual
`DepictorLocal::embedSquarePlanar`, `embedTBP`, `embedOctahedral` and `EmbeddedFrag`
constructors in the pinned library. It does not call whole `Compute2DCoords` or
obtain expected values from Rust.

`seeds::Input::new` borrows the graph, stereo `Metadata`, attachment `AtomData`
and explicit `RingCache`. `coordination(center, depict_ranks, ideal_lengths)`
returns an optional fragment for SP/TBP/OH tags. `from_coordinates` implements the
common coordinate-map constructor. `cis_trans` implements the direct bond
constructor. The across-atom, forward/reverse TBP axial, ideal ligand-angle and
ranked-neighbor queries are exposed and independently compared with native.

Coordination neighbor selection uses the supplied signed depict ranks and the
pinned platform's comparison-sort policy. This is separate from CIP-property
ranking used by neighbor setup. Across/axial lookups use original bond insertion
order and the original chiral permutation. Missing/out-of-range permutations and
truncated coordination spheres retain their source-defined behavior. TBP across
and axial tables are copied independently; deriving one from the other would
change the pinned source behavior.

A native quirk requires explicit state: each coordination function initializes
its ideal-point array only on its first invocation, capturing the then-current
`BOND_LEN`. Initialization precedes the chiral-tag check. `IdealLengths` supplies
those three captured values independently. The Rust library has no hidden mutable
global state. Final orchestration must retain the captured per-constructor values
and pass the current cis/trans bond length separately. Native fixture generation
warms each function once with its explicit length and a nonmatching tag, then
changes current bond lengths within that process. Three independent capture sets
exercise different SP/TBP/OH initial lengths.

The native decimal constants `0.707107` and `0.866025` are preserved. The former is
spelled as the exactly equivalent constant expression `707107 / 1000000` in Rust;
substituting a more precise square-root constant would change coordinates. SP
places the first ranked ligand and uses across-ligand queries for its opposite.
TBP selects native axial ligands, then places equatorial ligands in rank order.
OH selects the first usable axial pair and the first two equatorial directions,
including the source's partial-sphere behavior.

All coordination coordinates pass through the native-style coordinate-map
constructor. Every supplied atom starts fixed, with default normals/angles and
metadata. `setupNewNeighs` fills pending neighbors and attachment points. Attachment
geometry then considers zero, one, two or at least three already embedded
neighbors. One neighbor gives a normalized perpendicular; two give an angle;
three or more sort all angle/neighbor pairs, prefer a largest gap whose neighbors
belong to at most one ring, then select the smallest-angle anchor pair touching
the winning pair. Source adjacency order, angle ties, ring counts and rotation
sign are preserved. Coordinates are never realigned.

Ring information is read only by the three-or-more-neighbor branch. An
uninitialized cache remains usable for branches that do not read ring counts;
that branch returns `Invalid` if it needs an uninitialized cache. No ring
perception, stereo cleanup, property assignment or chemistry mutation occurs.

The direct cis/trans constructor creates only the double-bond endpoints, their
anchors, stereo controls, directions and default metadata. Z/cis shares one normal
direction; the other allowed native stereo codes use the opposite direction. Its
neighbor lists and attachment list remain empty. Native `embedCisTransSystems`
filters non-ring stereobonds, skips incomplete control lists, calls the constructor
and then calls `setupNewNeighs`; that selection/orchestration remains a later
stage. Controls supplied to the Rust constructor must be valid adjacent
substituents.

Every operation returns a new fragment. Invalid indices, property sizes,
nonadjacent controls, nonfinite inputs and missing required ring-cache state are
typed errors. Normalization failure is separate `Geometry(Numeric)`. Limits
include the shared 100,000-atom/300,000-bond graph bounds, 100,000 cached rings,
one million stored ring-atom entries and one million pairwise attachment angles.
Pair storage and sorting work are checked before allocation. Neighbor setup and
seed geometry have independent 50-million-unit work bounds, reducible with
`with_work_limit`. Inputs remain unchanged on success, failure and repeated calls.

Native zero-degree SP accesses `nbrs[0]` without checking. TBP with more than three
unclassified equatorial ligands can exceed its ideal-point array. These native UB
cases are documented from source and tested only through safe Rust rejection;
no native crash or parity claim is invented. Defined unusual inputs, including
zero-degree TBP/OH and oversized SP/OH spheres, are captured separately and match.

The 991-case corpus includes every SP/TBP/OH permutation, all truncated degrees,
missing/zero/invalid permutation properties, depict-rank ties and extremes,
CIP/chiral-rank differences, atom renumbering, retained ideal lengths, current
length changes, rings, coordinate-map omissions/repeated points, uninitialized
caches and all eight native bond-stereo codes. It contains 913 returned fragments,
3,603 atom states, 25,270 fragment scalar values, 9,478 ideal ligand-angle queries
and 75 native exceptions. Wrong-tag calls return no fragment. Every fragment
field, attachment/neighbor order and direct across/axial/rank query is checked.

Linux x86_64 Rust 1.95 matches every finite scalar bit and drawing-space f32
projection. The five dedicated tests pass; live same-host native replay also
passes in about 1.2 seconds. The prior four attachment tests remain unchanged and
pass after the two minimal internal helper visibility additions.

Linux-versus-macOS ARM64 is a separate numerical audit: all coordinates, projected drawing-space
f32 values and discrete metadata agree. There are 2,160 differing normal-component
scalars and six differing 30-degree attachment angles. Maximum absolute difference
is `1.1102230246251565e-16`; maximum relative difference is
`2.2204452727879493e-16`. Relative difference is `abs(a-b)/max(abs(a),abs(b))`.
These are cross-native differences; current Mac tests reproduce the native
contractions and require zero same-platform differences.

The largest normal difference is reproduced directly from native source operands:

```text
x = 0.866025 * 1.5; y = -0.75
source x*x + y*y:       2.2499984264062505, bits 4001ffff2ccbbb00
contracted fma(x,x,y*y): 2.24999842640625,  bits 4001ffff2ccbbaff
source -y/sqrt(sum):    0.5000001748438416, bits 3fe000005dde5ac8
native -y/sqrt(sum):    0.5000001748438417, bits 3fe000005dde5ac9
```

The six angle differences occur in `coordinates/3..8/2/1`: the normalized-vector
angle is `0.5235987755982987` under source-order arithmetic versus
`0.5235987755982988` in the macOS native build. Normals and angles feed later layout
decisions, so this checkpoint makes no full cross-platform runtime parity claim.
No epsilon, coordinate alignment or zero cleanup hides these differences.

The test-only observation header uses the same single-line ` private:` to
` public:` change as the attachment helper. Original `EmbeddedFrag.h` SHA-256 is
`a45692bbffd2dff60aa608565dc98d366a2aae7cfb75eeb372f1cf35a8f3f0f4`; observation-header
SHA-256 is `bf49eb06e2296abcf79f215ebb147930344171b565386835fc6dc3ec701dc7a2`. No member
layout or linked native code changes. Eleven pinned source hashes, executable and
native library hashes, observation patch and platform are recorded in each fixture.

Generate in an isolated checkout with the pinned native wheel and source headers:

```sh
CC=/usr/bin/clang CXX=/usr/bin/clang++ .venv/bin/python \
  tests/build_depict_geometry_oracle.py --component seeds \
  --rdkit-source /path/to/pinned/rdkit --boost-include /path/to/boost185

.venv/bin/python tests/build_depict_geometry_oracle.py --component seeds \
  --rdkit-source /path/to/pinned/rdkit --boost-include /path/to/boost185 --replay \
  --output tests/fixtures/depict-seeds-linux-native.json.gz
```

Windows x64 generation, its verbatim source adapter and fixture hashes are
documented in the numeric audit. Use
`tests/build_depict_windows_oracle.py --component seeds`. For optional direct
replay set `RESHIKI_DEPICT_SEEDS_ORACLE`, `RESHIKI_RDKIT_SOURCE` and the native
library loader path. Replay always requires raw bit equality. Run
`cargo +1.95.0 test --locked --test depict_seeds -- --nocapture`.

Compressed/JSONL fixture SHA-256:

- macos: `a0cb928f3eaa41380da5bf3449ce207bb5f2fa54414373e0e60810ce6e248a95`,
  `92e5738d051af38f4b08652992ecc7b264cfee9de50acef94e80399a5640a3fd`.
- linux: `47f6485f35996eaafaec756ecf4b69e00c5288b768c5a015f70011cd85ec72bd`,
  `1ae5d764f2f2158a3ce9a7d848e5cf4c7eb83cbf32052b5ca9516f41be06a944`.

Full fragment expansion/merging, constrained layout, templates, collision repair,
orientation, packing and runtime dispatch remain separate. Attribution is in
`licenses/rdkit/NOTICE` under BSD-3-Clause. No dependency or runtime path changes.
