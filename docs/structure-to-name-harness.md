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

## Baseline (rules namer, 2026-10-31)

No wrong and no unparseable names in any check. Everything not correct was
declined. Fast subset 6.7 s; full corpus about 40–120 s on a busy machine, most
of it in OPSIN parsing.

| Check               |      Fast |                                  Full |
| ------------------- | --------: | ------------------------------------: |
| structure           |  52 / 434 |                     145 / 1597 (9.1%) |
| name                |  29 / 246 | 31 / 685 (4.5%), 18 input unparseable |
| parity (exact text) |  40 / 434 |                     105 / 1592 (6.6%) |
| generated           | 308 / 500 |                   1717 / 3000 (57.2%) |

Most common full-run declines: multiple-ring systems (fused, spiro, bridged,
assemblies), N-substituted amides, other N/O/S groups, charges and isotopes,
and elements outside C, N, O, halogens and aromatic S.

### Structure round trip

| Category            | Fast correct / total | Full correct / total |
| ------------------- | -------------------: | -------------------: |
| alcohol_amine       |               6 / 13 |      6 / 13 (46.15%) |
| alkenyl_benzyl      |               0 / 10 |       0 / 10 (0.00%) |
| chain_ring_prefix   |               0 / 12 |       0 / 12 (0.00%) |
| data                |               0 / 13 |       0 / 63 (0.00%) |
| drug                |               0 / 20 |       0 / 20 (0.00%) |
| exploratory         |                0 / 1 |        0 / 4 (0.00%) |
| fixtures            |              13 / 22 |    70 / 106 (66.04%) |
| heterocycle         |               4 / 25 |      4 / 25 (16.00%) |
| integration         |               0 / 16 |       0 / 80 (0.00%) |
| mr_assembly         |               0 / 10 |       0 / 10 (0.00%) |
| mr_bridged          |               0 / 14 |       0 / 14 (0.00%) |
| mr_fused            |               0 / 20 |       0 / 20 (0.00%) |
| mr_macro_other      |                1 / 7 |       1 / 7 (14.29%) |
| mr_spiro            |                0 / 7 |        0 / 7 (0.00%) |
| natural_product     |               3 / 19 |      3 / 19 (15.79%) |
| pubchem_random      |               0 / 50 |      2 / 500 (0.40%) |
| ring_ON_ester_amide |               0 / 10 |       0 / 10 (0.00%) |
| roundtrip           |               9 / 77 |     37 / 383 (9.66%) |
| stereo              |               9 / 16 |      9 / 16 (56.25%) |
| suffix_on_ring      |               6 / 20 |      6 / 20 (30.00%) |
| unit                |               1 / 52 |      7 / 258 (2.71%) |
| total               |             52 / 434 |   145 / 1597 (9.08%) |

### Name round trip

| Category            | Fast correct / total | Full correct / total | Full input unparseable |
| ------------------- | -------------------: | -------------------: | ---------------------: |
| alcohol_amine       |               6 / 13 |      6 / 13 (46.15%) |                      0 |
| alkenyl_benzyl      |               0 / 10 |       0 / 10 (0.00%) |                      0 |
| chain_ring_prefix   |               0 / 12 |       0 / 12 (0.00%) |                      0 |
| drug                |               0 / 19 |       0 / 19 (0.00%) |                      1 |
| heterocycle         |               4 / 25 |      4 / 25 (16.00%) |                      0 |
| mr_assembly         |               0 / 10 |       0 / 10 (0.00%) |                      0 |
| mr_bridged          |               0 / 14 |       0 / 14 (0.00%) |                      0 |
| mr_fused            |               0 / 20 |       0 / 20 (0.00%) |                      0 |
| mr_macro_other      |                1 / 6 |       1 / 6 (16.67%) |                      1 |
| mr_spiro            |                0 / 7 |        0 / 7 (0.00%) |                      0 |
| natural_product     |               3 / 17 |      3 / 17 (17.65%) |                      2 |
| pubchem_random      |               0 / 49 |      2 / 488 (0.41%) |                     12 |
| ring_ON_ester_amide |               0 / 10 |       0 / 10 (0.00%) |                      0 |
| stereo              |               9 / 14 |      9 / 14 (64.29%) |                      2 |
| suffix_on_ring      |               6 / 20 |      6 / 20 (30.00%) |                      0 |
| total               |             29 / 246 |     31 / 685 (4.53%) |                     18 |

### Parity (exact openclatura text)

| Category            | Fast exact / total | Full exact / total |
| ------------------- | -----------------: | -----------------: |
| alcohol_amine       |     5 / 13 (38.5%) |     5 / 13 (38.5%) |
| alkenyl_benzyl      |      0 / 10 (0.0%) |      0 / 10 (0.0%) |
| chain_ring_prefix   |      0 / 12 (0.0%) |      0 / 12 (0.0%) |
| data                |      0 / 13 (0.0%) |      0 / 63 (0.0%) |
| drug                |      0 / 20 (0.0%) |      0 / 20 (0.0%) |
| exploratory         |       0 / 1 (0.0%) |       0 / 4 (0.0%) |
| fixtures            |     8 / 22 (36.4%) |   43 / 106 (40.6%) |
| heterocycle         |     4 / 25 (16.0%) |     4 / 25 (16.0%) |
| integration         |      0 / 16 (0.0%) |      0 / 80 (0.0%) |
| mr_assembly         |      0 / 10 (0.0%) |      0 / 10 (0.0%) |
| mr_bridged          |      0 / 14 (0.0%) |      0 / 14 (0.0%) |
| mr_fused            |      0 / 20 (0.0%) |      0 / 20 (0.0%) |
| mr_macro_other      |      1 / 7 (14.3%) |      1 / 7 (14.3%) |
| mr_spiro            |       0 / 7 (0.0%) |       0 / 7 (0.0%) |
| natural_product     |     2 / 19 (10.5%) |     2 / 19 (10.5%) |
| pubchem_random      |      0 / 50 (0.0%) |     2 / 495 (0.4%) |
| ring_ON_ester_amide |      0 / 10 (0.0%) |      0 / 10 (0.0%) |
| roundtrip           |      7 / 77 (9.1%) |    30 / 383 (7.8%) |
| stereo              |     9 / 16 (56.2%) |     9 / 16 (56.2%) |
| suffix_on_ring      |     3 / 20 (15.0%) |     3 / 20 (15.0%) |
| unit                |      1 / 52 (1.9%) |     6 / 258 (2.3%) |
| total               |    40 / 434 (9.2%) |  105 / 1592 (6.6%) |

### Generated stress set

| Category            | Fast correct / total | Full correct / total |
| ------------------- | -------------------: | -------------------: |
| acid                |              21 / 21 |  125 / 125 (100.00%) |
| alcohol             |              21 / 21 |  125 / 125 (100.00%) |
| aldehyde            |              21 / 21 |  125 / 125 (100.00%) |
| alkene              |              21 / 21 |  125 / 125 (100.00%) |
| alkene-stereo       |              20 / 20 |  125 / 125 (100.00%) |
| alkyl               |              21 / 21 |  125 / 125 (100.00%) |
| alkyne              |              21 / 21 |  125 / 125 (100.00%) |
| amide               |              21 / 21 |  125 / 125 (100.00%) |
| amine               |               8 / 21 |    35 / 125 (28.00%) |
| bicyclo             |               0 / 21 |      0 / 125 (0.00%) |
| ester               |              21 / 21 |  125 / 125 (100.00%) |
| ether               |               9 / 20 |    53 / 125 (42.40%) |
| fused-aromatic      |               0 / 21 |      0 / 125 (0.00%) |
| halide              |              12 / 21 |    45 / 125 (36.00%) |
| heteroaromatic      |              14 / 21 |    40 / 125 (32.00%) |
| ketone              |              21 / 21 |  125 / 125 (100.00%) |
| monocycle           |              13 / 21 |    39 / 125 (31.20%) |
| n-substituted-amide |               0 / 21 |      0 / 125 (0.00%) |
| nitrile             |              21 / 21 |  125 / 125 (100.00%) |
| ring-assembly       |               0 / 21 |      0 / 125 (0.00%) |
| ring-with-group     |               2 / 20 |      5 / 125 (4.00%) |
| spiro               |               0 / 21 |      0 / 125 (0.00%) |
| tetrahedral-stereo  |              20 / 20 |  125 / 125 (100.00%) |
| tricyclo            |               0 / 21 |      0 / 125 (0.00%) |
| total               |            308 / 500 | 1717 / 3000 (57.23%) |

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
