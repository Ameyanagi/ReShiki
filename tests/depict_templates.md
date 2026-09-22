This library checkpoint implements the default ring-template catalog, matching
and template-assisted fused-ring constructor from RDKit 2026.03.6, commit
`0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985`. Sources are
`Code/GraphMol/Depictor/Templates.cpp`, `Templates.h`, `TemplateSmarts.h` and
`EmbeddedFrag.cpp` (`matchToTemplate`, `checkStereoChemistry` and the template
branches of `embedFusedRings`). Attribution is in `licenses/rdkit/NOTICE`.

`templates::Input::new` borrows a validated graph, stereo metadata, ring cache and
prepared attachment properties. `match_system` returns the first accepted
builtin's original catalog ordinal, query-to-target atom mapping and detached
fragment. `embed` takes ordered ring-cache indices and the current bond length,
and returns a fragment plus its selected template ordinal and whether it seeded
only the core. Neither API mutates chemical state, assigns coordinates to a
molecule, changes global template configuration or enables a runtime layout path.

The generated catalog contains all 578 builtins in original source order, with a
maximum of 50 atoms. The pinned queries use only `!#200`, optionally conjoined
with `D2`, `D3`, `D4` or `D6`, and unrestricted bonds. Generated predicates, bond
insertion order and exact binary64 coordinates come from original
`Chem.MolFromSmarts` parsing. Rust does not introduce a general SMARTS parser.
The validated chemical graph excludes the native masking sentinel 200; the
selection mask therefore implements every `!#200` predicate. Internal catalog
validation checks dimensions, provenance, connected traversal, endpoints,
predicates and finite coordinates before publishing a cached catalog.

Size buckets retain catalog order. Bond-count and capped-degree histograms use
only the induced selected graph. Explicit query degree predicates use each
atom's full target degree, including outside neighbors. The matcher traverses
query atoms and target adjacency in native unsorted order. It returns only the
first mapping, then checks stereo; a stereo failure advances to the next
catalog entry. Eight captured cases prove that a later stereo-compatible mapping
of the same template must remain unused.

Stereo checks preserve native alternate-neighbor selection, control-atom
precedence, cis/trans inversion and source arithmetic order. They inspect all
stereodefined double bonds in the full target, including outside bonds. Missing
mapped endpoints or controls can therefore reject an otherwise matching ring
system. A successful template keeps its absolute catalog coordinates regardless
of current bond length. Its atoms start fixed and undergo the native coordinate
constructor's neighbor and attachment setup.

Full-template matching precedes ordinary ring coordinates. It is attempted for
multiple selected rings or one ring larger than eight atoms. If it fails, all
regular ring coordinates and trans-bond mirrors are computed in native order
before core pruning and matching. A matched core seeds the existing ring merger;
its attachment list and fixed atom flags survive exactly as in the original
constructor. The small private `rings::Construction` continuation shares that
merger with `embed_without_templates`; the public no-template behavior is
unchanged.

Inputs have the existing 100,000-atom/300,000-bond graph and ring-cache storage
bounds. Template matching has a 50-million-unit work budget, reducible with
`with_work_limit`; ring construction and seed setup retain their own bounded
budgets. Index, dimension, duplicate selection, work and numeric failures are
typed and return no partial fragment. Empty or unknown template sizes return no
match. Native nonfinite ring geometry is a separate restriction: one zero-length
trans-ring case produces NaNs in the original constructor, while Rust returns
`Rings(Geometry(Numeric))`. This is counted separately from native acceptance
parity. Custom template files and arbitrary query predicates are outside this
builtin library API.

The independent C++ oracle calls the original wheel's private
`EmbeddedFrag::matchToTemplate` and public template-enabled constructor. Its
copied header changes access visibility only. A separately extracted source
observer retains the original matching conditions and records template slot and
first mapping before coordinate copying. Stereo diagnostics call the original
substructure matcher with further mappings and the unchanged source stereo
predicate; these diagnostics do not alter expected native results. The builder
verifies pinned source hashes. Captures record source, executable and wheel
library hashes and preserve all floating-point values as hexadecimal bits.

Each native-platform corpus has the same 1,921 inputs: every builtin in original and permuted
atom order, outside-degree perturbations, oversized ring systems requiring core
fallback, fused/spiro/bridged rings, macrocycles, side chains, alternate stereo
controls and several bond lengths. It records 1,712 direct matches, 209 direct
nonmatches, 1,920 finite embeddings (including six successful core seeds), zero
native exceptions and the one separately counted nonfinite restriction. Every
catalog choice, mapping, fragment field, pending-neighbor order, attachment
point and fixed flag agrees. All 695,876 compared binary64 scalars in 3,632
fragments match bit for bit, including signed zero; all drawing-space f32
projections agree. These counts are asserted, not merely printed.

The two dedicated tests also cover bounded error paths and source immutability.
Matching-platform fixture replay is strict on Linux x86_64, macOS ARM64 and
Windows. Mac and Windows fixtures reuse every original input pickle and bit
pattern; only native expected outputs and provenance change. Linux ARM64 still
audits the Linux x86_64 fixture. Windows ARM64 uses the x64 reference fixture and
requires separate CI verification; the physical Windows capture host is x64.
Cross-platform audits are not evidence of native parity. Live native replay
always requires exact binary64 values, independently of fixture platform.

The template builder requires the wheel's Boost version (1.85), records compiler,
reference, generated-observation and native library hashes, and never compiles
Rust. macOS execution sets the wheel library directory for its original install
names. The Windows builder derives import libraries from the wheel's DLL exports.
MSVC includes private/public access in method names, so a friend observer calls
`matchToTemplate` while retaining its original private declaration. Its method
body and the template-enabled constructor execute in the original DLL; no copied
replacement implementation is used. The matching trace and stereo diagnostics
remain independent source observers as described above.

New fixture SHA-256 values:

| Platform    | Compressed fixture                                                 | Uncompressed JSONL                                                 |
| ----------- | ------------------------------------------------------------------ | ------------------------------------------------------------------ |
| macOS ARM64 | `8e6d800c10616584e450ef24d8f15b5e151057c509cd2d8c81b34a576b2a4c4c` | `7b1c365e09b58c2dec2ee2c0eefca5cd2b3b188728555e41ca015b79a7713248` |
| Windows x64 | `33a099dee5644b1ff2af2433fc6a11277e3088d7f4ffb5bdb1661936c1755f71` | `94073e1235675784210b42bf58c014323b730c3f487b835328deb61f503f2fdb` |

Regenerate or verify the catalog from the pinned original checkout:

```sh
.venv/bin/python tests/generate_depict_templates.py --rdkit-source /path/to/rdkit --check
```

Omit `--check` to regenerate and run `oxfmt` on the generated JSON afterward.
Build and capture the independent native oracle on Linux or macOS:

```sh
.venv/bin/python tests/build_depict_templates_oracle.py --rdkit-source /path/to/rdkit --boost-include /path/to/boost185
.venv/bin/python tests/depict_templates_reference.py --rdkit-source /path/to/rdkit --oracle artifacts/depict-templates-oracle --replay --output /path/to/native-fixture.json.gz
CARGO_BUILD_JOBS=4 cargo +1.95.0 test --locked --test depict_templates --test depict_rings --test depict_seeds --test depict_attachment -- --nocapture
```

A provenance-checked header snapshot can replace the full source checkout for
the native builder and captures. `--replay --fixture <existing-capture>` reuses
identical native input pickles on another host. Plain fixture replay requires only
Python's standard library; expected values never come from Rust. The Rust test
supports live same-host replay with `RESHIKI_DEPICT_TEMPLATES_ORACLE` pointing to
the built observer and `RESHIKI_RDKIT_SOURCE` to the pinned source or snapshot.
Live replay is always strict. `RESHIKI_DEPICT_REQUIRE_EXACT=1` also opts a
cross-platform fixture audit into exact comparisons.

Windows capture uses the pinned x64 wheel's Python from an x64 MSVC developer
prompt, with the same isolated source and Boost headers:

```bat
python tests\build_depict_windows_oracle.py --component templates ^
  --rdkit-source F:\isolated\rdkit --boost-include F:\isolated\boost185
```
