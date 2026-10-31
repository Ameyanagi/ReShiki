# Structure-to-name harness

`src/naming/s2n_harness.rs` (test-only) measures the structure-to-name namer by
round trip through the in-process OPSIN Rust port. The namer sits behind one
function, `namer`, so the openclatura port replaces a single line.

## Run

```sh
# Fast subset (normal test, about 7 s in a debug build)
cargo test --no-default-features --lib naming::s2n_harness::s2n_fast_subset -- --nocapture
# Full corpus (ignored by default; nightly in .github/workflows/s2n.yml)
S2N_REPORT=s2n-report.tsv cargo test --no-default-features --lib naming::s2n_harness::s2n_full_corpus -- --ignored --nocapture
# Raise the baseline after an improvement, then review the diff
S2N_UPDATE_BASELINE=1 cargo test --no-default-features --lib naming::s2n_harness::s2n_fast_subset
```

`S2N_EXTRA=<file>` with the ignored `s2n_extra_file` test round-trips any extra
SMILES (first tab-separated column) and prints the wrong and unparseable rows.
`S2N_DUMP=<dir>` writes every classified row per check (class, category, SMILES,
name, openclatura text for parity, decline reason) for analysis.

The fast test fails on any Wrong name and when a correct count in
`tests/fixtures/s2n/baseline.tsv` drops; it prints a reminder when one rises.
The full test fails on any Wrong name. The workflow runs nightly, on manual
dispatch and on pushes to `feat/s2n`, and uploads the full report.

## Checks

| Check     | What it does                                                                                                                                                                                                                                                                                                                       |
| --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| structure | SMILES → namer → OPSIN → structure; compare canonical isomeric SMILES (stereo, charge, isotopes).                                                                                                                                                                                                                                  |
| name      | PubChem name → OPSIN → structure S → namer → OPSIN → compare with S. A PubChem name OPSIN rejects is counted as input unparseable and left out of the rate.                                                                                                                                                                        |
| parity    | Namer text equals openclatura 0.4.3's text exactly. This is the Phase 2 port target.                                                                                                                                                                                                                                               |
| generated | Structure round trip on a seeded, deduplicated set of molecules built from fragments: chains with branches and unsaturation, monocycles and heteroaromatics, ring assemblies, spiro, bicyclo/tricyclo, fused aromatics, functional groups, E/Z and R/S, and ring + linker + group combinations. Every generated SMILES must parse. |

Every molecule gets one class: **correct** (OPSIN rebuilds the same structure),
**wrong** (OPSIN rebuilds a different structure; always a failure),
**declined** (namer refused, or the input is outside naming's identity rules),
or **unparseable** (namer produced text that the OPSIN port rejects). The report
lists every wrong and unparseable case and the top decline reasons.

## Fixtures

| File                    | Rows | Source                                                                                                                                                                                                           |
| ----------------------- | ---: | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `bakeoff_corpus.tsv`    |  703 | Phase 0 bake-off: 500 random PubChem compounds and 203 curated ones in 14 categories, with PubChem IUPAC names and openclatura 0.4.3 names.                                                                      |
| `openclatura_tests.tsv` |  894 | SMILES mined from openclatura's tests (commit `90dc5bcb`), named by openclatura 0.4.3. `asserted` is 1 or 0 where the test asserts a name that does or doesn't match the output. Category is the test directory. |
| `baseline.tsv`          |  107 | Fast-subset correct counts per check and category; `parity_text` rows hold exact text matches.                                                                                                                   |

openclatura-derived data is MIT; see `licenses/openclatura/`.

The fast subset is all 203 curated rows, every 10th random PubChem row, every
5th openclatura row per category, and the first 500 generated molecules (934
structures). The full run uses all fixture rows and 3,000 generated molecules.

## Baseline (openclatura port, stage 1, 2026-10-31)

`namer` calls `naming::openclatura::name_smiles`, the Rust port of openclatura
0.4.3 for acyclic and monocyclic molecules; everything with more than one ring
or more than one component declines. No wrong names in any check. Unparseable
rows are openclatura names the OPSIN port cannot read (mostly chalcogen
peroxides and thioperoxy anhydrides); they are not counted as correct. Fast
subset about 10 s; full corpus about 70 s.

On the stage-1 molecules (at most one ring, one component; 659 corpus rows)
the port gives 615 correct, 0 wrong, 41 unparseable and 3 declined, and its
text equals openclatura's for 654 of 658 rows with a golden name. Of the other
4, two are inputs naming's identity check rejects (`[SeH]`, `[PH]`) and two
differ only by input atom order: the harness names its canonical SMILES, and
upstream's tie-breaking depends on atom order (`C=CCC#C` vs `C#CCC=C`).
Named directly from their original SMILES, 2,937 of 2,940 stage-1 molecules
(corpus plus generated) match upstream exactly, as do fresh held-out sets of
about 9,000 molecules, except where upstream's own name round-trips to a
different structure: there the port declines (lines marked
`Upstream divergence` in `src/naming/openclatura/`). Previous rules-namer baseline: structure 145, name 31,
parity 105, generated 1717 (full).

| Check                   |      Fast |                                    Full |
| ----------------------- | --------: | --------------------------------------: |
| structure               | 209 / 434 |                      615 / 1597 (38.5%) |
| name                    | 114 / 246 | 141 / 685 (20.6%), 18 input unparseable |
| parity (exact text)     | 218 / 434 |                      654 / 1592 (41.1%) |
| generated               | 381 / 500 |                     2281 / 3000 (76.0%) |
| unparseable (structure) |        10 |                                      41 |

### Structure round trip

| Category            | Fast correct / total | Full correct / total |
| ------------------- | -------------------: | -------------------: |
| alcohol_amine       |              13 / 13 |     13 / 13 (100.0%) |
| alkenyl_benzyl      |               8 / 10 |       8 / 10 (80.0%) |
| chain_ring_prefix   |              12 / 12 |     12 / 12 (100.0%) |
| data                |               0 / 13 |        0 / 63 (0.0%) |
| drug                |               4 / 20 |       4 / 20 (20.0%) |
| exploratory         |                1 / 1 |       4 / 4 (100.0%) |
| fixtures            |              22 / 22 |    102 / 106 (96.2%) |
| heterocycle         |              16 / 25 |      16 / 25 (64.0%) |
| integration         |               0 / 16 |        1 / 80 (1.2%) |
| mr_assembly         |               0 / 10 |        0 / 10 (0.0%) |
| mr_bridged          |               0 / 14 |        0 / 14 (0.0%) |
| mr_fused            |               0 / 20 |        0 / 20 (0.0%) |
| mr_macro_other      |                4 / 7 |        4 / 7 (57.1%) |
| mr_spiro            |                0 / 7 |         0 / 7 (0.0%) |
| natural_product     |               9 / 19 |       9 / 19 (47.4%) |
| pubchem_random      |               7 / 50 |      34 / 500 (6.8%) |
| ring_ON_ester_amide |               7 / 10 |       7 / 10 (70.0%) |
| roundtrip           |              41 / 77 |    210 / 383 (54.8%) |
| stereo              |              16 / 16 |     16 / 16 (100.0%) |
| suffix_on_ring      |              20 / 20 |     20 / 20 (100.0%) |
| unit                |              29 / 52 |    155 / 258 (60.1%) |
| total               |            209 / 434 |   615 / 1597 (38.5%) |

### Name round trip

| Category            | Fast correct / total | Full correct / total |
| ------------------- | -------------------: | -------------------: |
| alcohol_amine       |              13 / 13 |     13 / 13 (100.0%) |
| alkenyl_benzyl      |               8 / 10 |       8 / 10 (80.0%) |
| chain_ring_prefix   |              12 / 12 |     12 / 12 (100.0%) |
| drug                |               4 / 19 |       4 / 19 (21.1%) |
| heterocycle         |              16 / 25 |      16 / 25 (64.0%) |
| mr_assembly         |               0 / 10 |        0 / 10 (0.0%) |
| mr_bridged          |               0 / 14 |        0 / 14 (0.0%) |
| mr_fused            |               0 / 20 |        0 / 20 (0.0%) |
| mr_macro_other      |                4 / 6 |        4 / 6 (66.7%) |
| mr_spiro            |                0 / 7 |         0 / 7 (0.0%) |
| natural_product     |               9 / 17 |       9 / 17 (52.9%) |
| pubchem_random      |               7 / 49 |      34 / 488 (7.0%) |
| ring_ON_ester_amide |               7 / 10 |       7 / 10 (70.0%) |
| stereo              |              14 / 14 |     14 / 14 (100.0%) |
| suffix_on_ring      |              20 / 20 |     20 / 20 (100.0%) |
| total               |            114 / 246 |    141 / 685 (20.6%) |

### Parity (exact openclatura text)

| Category            | Fast exact / total | Full exact / total |
| ------------------- | -----------------: | -----------------: |
| alcohol_amine       |            13 / 13 |   13 / 13 (100.0%) |
| alkenyl_benzyl      |             8 / 10 |     8 / 10 (80.0%) |
| chain_ring_prefix   |            12 / 12 |   12 / 12 (100.0%) |
| data                |             0 / 13 |      0 / 63 (0.0%) |
| drug                |             4 / 20 |     4 / 20 (20.0%) |
| exploratory         |              1 / 1 |     4 / 4 (100.0%) |
| fixtures            |            22 / 22 |  102 / 106 (96.2%) |
| heterocycle         |            16 / 25 |    16 / 25 (64.0%) |
| integration         |             0 / 16 |      2 / 80 (2.5%) |
| mr_assembly         |             0 / 10 |      0 / 10 (0.0%) |
| mr_bridged          |             0 / 14 |      0 / 14 (0.0%) |
| mr_fused            |             0 / 20 |      0 / 20 (0.0%) |
| mr_macro_other      |              4 / 7 |      4 / 7 (57.1%) |
| mr_spiro            |              0 / 7 |       0 / 7 (0.0%) |
| natural_product     |             9 / 19 |     9 / 19 (47.4%) |
| pubchem_random      |             6 / 50 |    33 / 495 (6.7%) |
| ring_ON_ester_amide |             7 / 10 |     7 / 10 (70.0%) |
| roundtrip           |            41 / 77 |  209 / 383 (54.6%) |
| stereo              |            16 / 16 |   16 / 16 (100.0%) |
| suffix_on_ring      |            20 / 20 |   20 / 20 (100.0%) |
| unit                |            39 / 52 |  195 / 258 (75.6%) |
| total               |          218 / 434 | 654 / 1592 (41.1%) |

### Generated stress set

| Category            | Fast correct / total | Full correct / total |
| ------------------- | -------------------: | -------------------: |
| acid                |              21 / 21 |   125 / 125 (100.0%) |
| alcohol             |              21 / 21 |   125 / 125 (100.0%) |
| aldehyde            |              21 / 21 |   125 / 125 (100.0%) |
| alkene              |              21 / 21 |   125 / 125 (100.0%) |
| alkene-stereo       |              20 / 20 |   125 / 125 (100.0%) |
| alkyl               |              21 / 21 |   125 / 125 (100.0%) |
| alkyne              |              21 / 21 |   125 / 125 (100.0%) |
| amide               |              21 / 21 |   125 / 125 (100.0%) |
| amine               |              21 / 21 |   125 / 125 (100.0%) |
| bicyclo             |               0 / 21 |       0 / 125 (0.0%) |
| ester               |              21 / 21 |   125 / 125 (100.0%) |
| ether               |              20 / 20 |   125 / 125 (100.0%) |
| fused-aromatic      |               0 / 21 |       0 / 125 (0.0%) |
| halide              |              21 / 21 |   125 / 125 (100.0%) |
| heteroaromatic      |              21 / 21 |   125 / 125 (100.0%) |
| ketone              |              21 / 21 |   125 / 125 (100.0%) |
| monocycle           |              21 / 21 |   125 / 125 (100.0%) |
| n-substituted-amide |              21 / 21 |   125 / 125 (100.0%) |
| nitrile             |              21 / 21 |   125 / 125 (100.0%) |
| ring-assembly       |               0 / 21 |       0 / 125 (0.0%) |
| ring-with-group     |               6 / 20 |     31 / 125 (24.8%) |
| spiro               |               0 / 21 |       0 / 125 (0.0%) |
| tetrahedral-stereo  |              20 / 20 |   125 / 125 (100.0%) |
| tricyclo            |               0 / 21 |       0 / 125 (0.0%) |
| total               |            381 / 500 |  2281 / 3000 (76.0%) |

## Chemistry crate readiness for the port

Outside its optional self-audit (`audit/`, which the OPSIN round trip replaces)
and its own OPSIN check, openclatura uses RDKit only to read the structure:
SMILES parsing, kekulization, ring info (`FastFindRings`), legacy stereo
perception (`AssignStereochemistry` + `FindMolChiralCenters`, which decides
which atoms are stereocentres) and the modern `rdCIPLabeler` (which decides
their labels, including pseudo-asymmetric r/s). `crates/chemistry` has ports of
all of these: legacy perception in `stereo/perception.rs`, the new CIP labeler
in `stereo/cip/` (R/S, r/s, E/Z, M/P), `kekulize.rs` and `rings.rs`. Points to
settle in Phase 2:

- `rings::fast` is a depth-first cycle basis like RDKit's `FastFindRings`, not
  a smallest or symmetric set; `rings::perceive` gives the symmetric SSSR. Match
  each openclatura ring query to the right one.
- Canonical kekulization needs caller-supplied canonical ranks; backtracking is
  bounded.
- The potential-stereocentre predicate in perception is crate-private.
- The naming input boundary (`naming::canonical_smiles`) rejects allene and
  coordination stereo; today's rules namer also refuses r/s and axial
  descriptors. The port must keep both checks or widen them on purpose.
