`depict::finalize::finish` implements the deterministic tail of RDKit 2026.03.6
`compute2DCoords`, after fragment expansion and collision correction. It consumes
an explicit atom count, the surviving fragment list in native order, the original
optional coordinate map, and canonical-orientation/work options. It returns
packed fragments and an original-atom-order 2D conformer with ID zero. The
supported conformer policy is the application's reachable `clearConfs=true`;
appending to an existing conformer collection is outside this API. No runtime
layout dispatch changes in this checkpoint.

The source is commit `0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985`. The Python wrapper
uses canonical orientation by default; selected cleanup explicitly disables it.
Expansion and collision preserve `Vec<rings::Fragment>` order, and finalization
does not sort that list. Its stages are:

1. With no coordinate map or an empty map, optionally canonicalize each fragment.
   Empty and single-atom fragments remain unchanged. Degenerate larger fragments
   remain centered after the native eigenvector early return. When a rotation
   exists, normals follow native `Transform(loc + normal) - Transform(loc)`,
   after centering; directly multiplying the normal would change rounding.
2. Compute each native signed bounding box. Pack subsequent fragments along x
   when the accumulated width is at most its height; otherwise use y. Preserve
   the source's addition order, fixed spacing of 1.0, unchanged accumulated minima
   and orthogonal maximum. Translation changes locations only. Fragment boxes
   intentionally retain their pre-translation values.
3. Assemble `(x,y,+0.0)` positions by fragment order and increasing atom IDs.
   Later fragments overwrite repeated IDs; missing atoms retain positive zeros.
   The new conformer is two-dimensional and replaces prior conformers with ID 0.
4. A coordinate map containing exactly one entry translates every conformer
   position after packing. It does not change the returned fragments or bounds.
   Any nonempty map suppresses canonical orientation. Multi-point maps are not
   reset or fitted here; cleanup's subsequent orientation, fixed-point checks
   and restoration remain separate application work.

The existing geometry wrapper now obtains centered coordinates and an optional
rotation from one private helper. Its arithmetic and public result are unchanged;
all prior geometry/rings/attachment/seed native regressions still pass. The
platform operation policy remains [depict_numeric.md](depict_numeric.md).

`finish_with_work` is a crate-private wrapper for cumulative solver work. It
charges completed work on success and error, with the same public behavior and
hard limit. One operation admits at most 100,000 molecule atoms, 100,000 fragments,
one million embedded atom entries and one million attachment/neighbor entries.
Storage counts are checked before cloning caller state; the 50-million operation
budget is charged throughout the stages. Input indices and finite values are validated; all caller inputs
remain unchanged on errors. Finite arithmetic overflow returns
`Error::Geometry(geometry::Error::Numeric)`, separately from nonfinite input and
later editable-document coordinate limits.

## Independent native evidence

The three platform fixtures use identical input bits and direct native C++
observations. Their 2,312 cases cover canonical eigenvector thresholds and extreme
axes, signed zeros, singleton/empty fragments, repeated coordinates, custom
normals/metadata, packing ties and fragment permutations, absent/empty/one/many
coordinate maps, missing and overwritten atom positions, existing conformers and
native ring/attachment/stereochemical seed fragments. Each capture records input,
post-canonical and packed fragment state, output positions, conformer ID/count
and dimensionality. No Rust result, alignment or rounding produces expectations.

There are 3,433 finite packed fragments, 23,848 embedded atom states and 232,981
checked scalar values. Linux x86_64 debug and Mac ARM64 ordinary debug/release checks are
bit-exact, including independent f32 drawing projection. The Windows x64 native
fixture agrees with Linux for this corpus; a Rust run on Windows is a separate
same-platform verification, and its test gate requires exact bits. ARM64 Windows
requires CI verification against its reference ABI. Cross-platform raw differences
remain descriptive audits, never passing tolerances.
The Linux-versus-Mac audit observes 51,675 differing scalar bits and 1,106 differing
projected values. Maximum absolute f64 difference is `2.9802322387695313e-8`
(a transformed normal near large translated coordinates), maximum relative
difference is `2.0` near zero, and maximum drawing-space f32 difference is
`1.300270469073439e-8`. Those counts describe the corpus, not acceptance limits.

Four deliberately extreme finite inputs overflow the native center, transformed
normal, packed location or one-anchor subtraction. Native produces nonfinite
values without a useful finite layout; Rust requires a typed numeric error. These
cases are counted separately from successful parity comparisons. Additional Rust
tests cover invalid indices, nonfinite input, early/late work exhaustion, storage
limits, caller immutability and cumulative-budget consumption across calls.

On Linux and Mac the helper calls original `canonicalizeOrientation`,
`DepictorLocal::_shiftCoords` and `copyCoordinate` in the pinned wheel. The
one-anchor tail is not exported, so the builder extracts the original source
bytes into a minimal namespace/parameter wrapper. Windows also needs verbatim
adapters for the two unexported packing/copy functions. The canonicalization
method, bounding boxes, transforms and conformer APIs remain original native
calls. These extracts come from `Code/GraphMol/Depictor/RDDepictor.cpp`, SHA-256
`f1ae9e705405d5254575c595c09c5d2ee240d4d2abf05209dd42dad5ae99b31f`:

| Body                           | Original byte range | SHA-256                                                            |
| ------------------------------ | ------------------- | ------------------------------------------------------------------ |
| Packing (Windows only)         | `[11495,12454)`     | `66b0c210af235d3c6e03c5dbc2c1cf6fa26061dd47d4ed24b9118aa2e65bcd27` |
| Coordinate copy (Windows only) | `[15699,16729)`     | `518e6b4134c503c841fd2c0e5d7c4d83e9c48d2c4e0a64d7283694736aa1c571` |
| Single-anchor tail             | `[20846,21383)`     | `0ae2bb59b3bb0e51836b530a06ab048a9efd0ce8dc10de946fd94cc95e78cfec` |

The observation header changes only the single ` private:` line in
`EmbeddedFrag.h` to ` public:`. Member layout and function bodies remain intact.
The fixture provenance records all seven original source hashes, generated
adapter/header/reference hashes, compiler command/log hash, wheel library hashes,
Boost version and executable hash. The adapter is development-only and retains
RDKit's BSD-3-Clause attribution in `licenses/rdkit/NOTICE`.

Use matching Boost 1.85 headers and the pinned wheel's Python in an isolated
checkout. Windows uses an x64 MSVC developer prompt and matching x64 Python;
Mac uses Apple clang. The builder performs no Rust compilation or global changes.
For each target platform, replay the existing inputs:

```sh
CXX=/usr/bin/clang++ .venv/bin/python tests/build_depict_finalize_oracle.py \
  --rdkit-source /path/to/pinned/rdkit --boost-include /path/to/boost185 \
  --fixture tests/fixtures/depict-finalize-linux-native.json.gz --replay \
  --output tests/fixtures/depict-finalize-macos-native.json.gz
```

On Linux omit the Apple compiler override. On Windows use `python` and native
paths; the builder generates import libraries from the actual wheel DLL exports,
compiles one job with `/O2 /MD`, and scopes crash-dialog suppression to its process.
Default Rust fixture reading uses only Python's standard library. Optional live
replay sets `RESHIKI_DEPICT_FINALIZE_ORACLE`, `RESHIKI_RDKIT_SOURCE`, the wheel's
loader path and, when needed, `RESHIKI_REFERENCE_PYTHON`; live replay is strict.
Run `cargo +1.95.0 test --locked --test depict_finalize -- --nocapture`.

Compressed / uncompressed fixture SHA-256:

| Platform     | Compressed                                                         | JSONL                                                              |
| ------------ | ------------------------------------------------------------------ | ------------------------------------------------------------------ |
| Linux x86_64 | `5a66c11c27b801dfcd795a0d2b3227dfa5e7ee774689747bd4cbff8ecd2d6d3c` | `ade9d1cc2f7420a4ca97677fe4508d43fe4e3435dc6652a8122b638c85774dcf` |
| macOS ARM64  | `f482e4ce318bedfab0ada36f74a285895ef2a564a773cc7d93b64c78bc71e363` | `0ea75f6fcacfe2bd8aab2e95c1dfd21885d2b1059c148e0cbe6d871959162114` |
| Windows x64  | `c91b1dad3e165857943f0fa65dcacbf1647d78ea012b89ad7b6aece3ebb39540` | `6bb12302e29172ea1d90496d8a3dac7c899eefe6b09fee7bcd02c97c1026f7e7` |
