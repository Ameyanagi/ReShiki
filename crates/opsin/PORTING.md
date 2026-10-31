# Source-to-Rust migration ledger

Pinned source: OPSIN main (3.0-SNAPSHOT),
`42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91`. The port started from OPSIN 2.9.0
(`b91b610af5ab07560fedb20730d7aef46bb2bca0`) and now includes every later
upstream change that affects parsing or structures; see the commit ledger below. The embedded data has
41 XML files, seven DTDs and 48 native DFA tables. Integration resources contain
1,312 active rows across 31 nomenclature families, including 174 phane names.
The upstream MIT license covers translated algorithms, resources and copied
fixtures. Module headers retain the base source and identify subsequent updates.

The following ledger records source phases and executed native acceptance
gates. Passing a frozen corpus does not promise interpretation of every possible
chemical name or reproduce behavior from a newer upstream revision.

| Upstream source                                                                                                                                          | Rust module                                                               | State and evidence                                                                                                                                                                                                       |
| -------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `ResourceManager`, `AutomatonInitialiser`                                                                                                                | `resources`, `automaton`, `resource_data`, embedded XML/DTD/DFA           | Complete resource loading; all 41 XML and seven DTD files byte-equal the pinned source; deterministic migration of all 48 forward/reverse DFA tables                                                                     |
| `OpsinRadixTrie`                                                                                                                                         | `trie`                                                                    | All dictionary prefix matches and empty marker tokens; exercised by the full lexical oracle                                                                                                                              |
| `PreProcessor`, normalization in `StringTools`                                                                                                           | `preprocess`, `preprocess_characters`                                     | Exact source mappings and normalization order; upstream cases plus corpus normalization                                                                                                                                  |
| `ParseRules`, `ReverseParseRules`, `AnnotatorState`                                                                                                      | `parse_rules`                                                             | Native DFS, upstream annotation order/longest acceptance/ambiguity bound, case-sensitive DFA tokens, both zero-width predicates                                                                                          |
| `Tokeniser`, `WordTools`                                                                                                                                 | `tokenizer`                                                               | Omitted-space splitting, longest functional term, bracket-space repair, reverse-space recovery, CAS index/compound-with handling; frozen oracle gate passes                                                              |
| `Element`, `GroupingEl`, `TokenEl`, `Parser`, `WordRules`, omitted-space corrector, `CASTools`, `SortParses`                                             | `parse_tree`, `frontend`, `word_rules`, `word_rules_omitted_space`, `cas` | Complete frontend gate: 5,436 exact raw/ranked XML comparisons; native CAS and omitted-space fixtures and the complete pipeline gate pass                                                                                |
| `Atom`, `AtomParity`, `Bond`, `BondStereo`, `ChemEl`, `Fragment`, `OutAtom`, `ValencyChecker`, `AtomProperties`, `SMILESFragmentBuilder`, `SMILESWriter` | `graph`, `valence`, `smiles`                                              | Source foundation present; resource SMILES snapshots, source writer/parser examples and graph/stereo/substitution gates pass; complete assembly fidelity still gated below                                               |
| `ComponentGenerator`, XML declarations, tree helpers                                                                                                     | `component_generator`, `xml_declarations`, `tree_tools`                   | Complete intermediate gate: all 5,932 ranked candidates' ordered XML/errors/warnings match; 25 focused tests pass                                                                                                        |
| `FragmentManager`, `FragmentTools`, `BuildState`, `BuildResults`                                                                                         | Construction state and graph utilities                                    | Source port present; stable registration, cloning, shared fragment/out-atom views, identity-aware ambiguous atom sets and graph helpers; 12 manager regression tests and complete corpus gate pass                       |
| `ComponentProcessor`, `SuffixRules`, `SuffixApplier`, `FunctionalReplacement`                                                                            | Component processing and suffix/replacement modules                       | All source method inventories present; 997 exact suffix queries, 27 processing tests, 38 ring tests, 37 carbohydrate tests, 26 bracket tests and 40 suffix/isotope tests pass; complete corpus gate passes               |
| `PhaneAmplifier`                                                                                                                                         | `phane_amplifier`                                                         | All source methods ported; 348 exact strict/radical phane structures, 21 upstream rejection cases and focused connectivity/error/warning regressions pass                                                                |
| `StructureBuilder`, `StructureBuildingMethods`                                                                                                           | Assembly modules                                                          | All source word rules and attachment modes connected; complete strict/radical/configuration corpus gates and 19 source-derived assembly fixtures pass                                                                    |
| `FusedRingBuilder`, `FusedRingNumberer`, cycles/SSSR helpers                                                                                             | Ring modules                                                              | Source methods present; 81 exact numbering/SSSR cases, 11 exact resolved-fragment fusion snapshots and nine branch tests pass                                                                                            |
| `StereoAnalyser`, `CipSequenceRules`, `StereochemistryHandler`, `AmbiguityChecker`                                                                       | Stereo/ambiguity modules                                                  | Source methods present; CIP gate includes all 15 source priorities and unsupported tie; 14 handler cases plus frozen analyser/environment/substitution graph gates pass; complete stereo/configuration corpus gates pass |
| `NameToStructure`, `OpsinResult`, `NameToStructureConfig`                                                                                                | `Parser`, `api`, `pipeline`                                               | Complete stage chain and source candidate selection connected; all 2,624 strict/radical and 640 configuration records match exactly                                                                                      |

## Current lexical gate

`tests/grammar_oracle.rs` compares 8,100 frozen pinned-OPSIN records: 1,350
distinct names from all active integration resources and explicit edge cases,
in six modes (forward/reverse annotation, forward/reverse tokenization with and
without space removal). It checks normalized input, every token and annotation
in order, all word alternatives, and exact failure/error fields. The gate does
not run component transformations or molecular assembly.

These frozen comparisons check the pinned revision; they do not predict
complete construction or worst-case input performance.

## Complete result gate

`tests/structure_oracle.rs` compares all 1,312 integration names under strict
defaults and radicals allowed: 2,624 frozen records. It checks exact status,
message, ordered warnings, semantic CXSMILES, and ordered graph metadata and
stereo references. It reports every nomenclature family's counts; optional
`OPSIN_ORACLE_FAMILY` and `OPSIN_ORACLE_NAME` filters support diagnosis without
changing the default complete gate. `OPSIN_ORACLE_REPORT` optionally writes
compact per-record diagnostic JSONL to the provided local path.

All 2,624 complete records now match exactly in every one of the 31 families.
`tests/configuration_oracle.rs` also matches 640 complete results: 20 source and
edge names crossed with all 32 combinations of the five upstream flags. Those
fixtures cover radical wildcard attachment labels, polymer semantics, acid
omission, unsupported stereo warnings/failures, and detailed failure analysis.

The final Java-free `cargo test -p opsin --all-targets --offline -j4` run passes
all 371 test functions across 42 integration targets plus library tests, with no failed or ignored
tests. This includes the complete result gates and the intermediate oracles.
Strict `cargo clippy -p opsin --all-targets --offline -- -D warnings` also
passes. The reviewed lint changes preserve ordered traversal and the owned
snapshots required while parse-tree child lists are mutated.
`cargo fmt -p opsin -- --check` passes as well.

Application worker integration, release-artifact dependency/runtime checks and
the repository's broader required checks are maintained by the application
integration task; crate acceptance does not replace those checks.

## Required compatibility gates

1. Preserve source pin, license, resource hashes, migration-table provenance,
   and exact source/module ownership.
2. Match upstream preprocessing, token/word annotation, CAS uninversion,
   parse-tree transformations, and candidate ranking.
3. Port core graph mutation, fragment-resource, suffix, replacement, assembly,
   fused-ring numbering, configuration, failure and ambiguity test behavior.
4. Preserve ordered locants/out atoms, spare/lambda/minimum valency, functional
   atoms, explicit hydrogens, isotope/charge, and parity references, including
   hydrogen/deoxy-hydrogen placeholders, during cloning and assembly.
5. Preserve OPSIN's CIP rules 1–2 and its deliberate unsupported stereo cases.
   A broader external CIP algorithm changes pinned-source behavior.
6. Return the first ranked warning-free candidate; if none succeeds without
   warnings, return the first warning-bearing candidate. Do not stop at the
   first constructible parse.
7. Run all 1,312 integration rows with their actual upstream setting
   `allow_radicals = true`, with per-family counts and explicit failures.
8. Compare status, warnings, semantic CXSMILES and graph identity, including
   isotope, charge, hydrogen placement, components, bond order and stereo
   groups. The upstream pruned-InChI gate omits fixed-H/reconnected layers and
   alone cannot establish exact tautomer or connectivity fidelity.
9. Verify the shipping dependency graph, release artifact and application
   naming worker have no Java/JAR invocation or network fallback. Preserve
   application rejection of unsupported identity semantics.

## Upstream commits after 2.9.0

Every non-merge commit from tag `2.9.0` to `42e87ac6` is classified below.
"Ported" commits change Rust code or the copied resources and are covered by the
regenerated fixtures; nothing was deliberately skipped.

| Commit                                                                 | Upstream change                                                    | Status                                                |
| ---------------------------------------------------------------------- | ------------------------------------------------------------------ | ----------------------------------------------------- |
| `9653b455`, `dbcc6ca6`                                                 | Greek locants for cycleformer suffixes start at the acid carbon    | Ported (`component_processor`)                        |
| `f57c9069`, `39390441`                                                 | Locant 8 for pyrrolizine/pyrrolizidine ring-fusion carbon          | Ported (resources)                                    |
| `73fbf170`                                                             | Valency 2 for Cl/Br/I/At at charge −1 (substituted -anuide)        | Ported (`valence`)                                    |
| `512dd257`, `e243e0bf`                                                 | Isotopes inside fusion names fail cleanly                          | Ported (`component_processor_rings`)                  |
| `cc7134fc`, `3e495273`                                                 | ResourceManager: no token dictionary repopulation on reverse parse | No Rust change: resources are built once              |
| `7c57ef00`, `4e2f78dc`                                                 | Peroxycyanate token                                                | Ported (resources)                                    |
| `3970f74d`                                                             | Prime position in stereo locants, (3'aS) → (3a'S)                  | Ported (`component_generator`)                        |
| `53a5b513`, `48c6e0fc`, `43108de6`                                     | Parse rules walk only live DFA transitions                         | Ported (`automaton`, `parse_rules`); no result change |
| `05eea6b6`, `d6f92c57`                                                 | Locanted hydro prefix on porphyrin is indicated hydrogen           | Ported (`component_processor`)                        |
| `6a002208`                                                             | Remove suffix-rule cache                                           | No Rust change: the port has no such cache            |
| `d03a43f1`                                                             | Amido/hydrazido followed by oxy is not functional replacement      | Ported (`component_generator`)                        |
| `2d5e7325`                                                             | Asparaginic acid                                                   | Ported (`component_generator`)                        |
| `58752af3`, `093a195c`, `adb97f16`, `75ff41fe`                         | Fused ring numbering: horizontal chains and grid layout            | Ported (`fused_ring_numberer`)                        |
| `836038e0`, `945df68f`, `a37ddaf6`                                     | Phane nomenclature                                                 | Ported (`phane_amplifier` and shared paths below)     |
| `6ef930b9`, `b1a4e66d`                                                 | O/S/Se/Te acid locants on non-carboxylic acids                     | Ported (`suffix_applier`, `functional_replacement`)   |
| `73edac3a`                                                             | Mixture delimiters honoured when applying word rules               | Ported (`tokenizer`, `frontend`, `word_rules`)        |
| `8135d25a`                                                             | Choline salicylate override                                        | Ported (resources)                                    |
| `f843de6d`                                                             | Zero stoichiometry ratio rejected                                  | Ported (`frontend`)                                   |
| `e64dd527`                                                             | D/L assignment with alcohol/ketone/aldehyde side chain             | Ported (`stereochemistry_handler`)                    |
| `1de09c38`                                                             | Incorrect locant promotion to a word-level locant                  | Ported (`component_processor`, `structure_builder`)   |
| `9f30df5c`, `1f6b06bb`                                                 | Olate is not an ateGroup; word-rule tests                          | Ported (`word_rules`)                                 |
| `e2e06af3`                                                             | Internal substituent locants on unsuffixed symmetric parents       | Ported (`component_processor`)                        |
| `eaa1ff38`                                                             | SSSR reduction loop exits early                                    | Ported (`fused_ring_numberer`)                        |
| `d30e5c4c`                                                             | Normalise repeated whitespace                                      | Ported (`preprocess`)                                 |
| `01c2e716`                                                             | Em dash separates components                                       | Ported (`preprocess`, `tokenizer`)                    |
| `d6efe224`                                                             | Unused import                                                      | Not relevant (Java only)                              |
| `6c5b0ced`, `996a5805`, `a6a53cb8`, `6f93f349`, `4dd9ca77`, `be2db719` | Release/dependency versions (pom)                                  | Not relevant (build)                                  |
| `bef2d8e4`, `9c6478cc`, `429c40f0`, `2684fb49`, `f29e2cb8`             | Command-line tool                                                  | Not relevant (CLI)                                    |
| `6e61e0eb`, `e819e565`                                                 | GitHub Actions                                                     | Not relevant (CI)                                     |

The port also fixes one 2.9.0-era divergence found by the wider parity check:
intra-fragment incoming valency ignores suffix atoms, as `Fragment.java` does, so
`hexahydropyridin-2-one` now parses as it does in the jar.

### Phane nomenclature

`phane_amplifier` ports `PhaneAmplifier.java`. Warnings about ambiguous locants
follow Java `HashSet` iteration (bucket order, then insertion order). Bins that
Java converts to trees need eight colliding locants in one bucket and are not
modelled; this only affects the order of those warnings.

Shared paths changed by these upstream commits:

- Unicode superscript digits following a digit receive one caret; initial
  isotope superscripts keep their old normalization. HTML `<sup>` tags normalize
  wherever they occur, using Java's case-insensitive matching behavior.
- Numeric and amino-acid locants accept an optional `^digits` segment. This
  propagates through locant lookup, fragment relabeling, indirect suffix/root
  matching, functional replacement and fused-ring validation. The independent
  `SortByLocants` comparator retains its narrower unchanged pattern.
- Added/indicated hydrogen, compound locants and element superscript forms
  accept amplified atom locants. Shared DFA expansions also affect isotope,
  lambda, spiro and stereochemical locant recognition.
- Bracketed stereochemistry accepts a second bracketed locant; E/Z and
  cis/trans assignment uses both bond endpoints, including ordinary names.
  Compound parsing follows Java's first match and global removal, preserving
  its accepted bracket forms.
- Every ring assembly receives ordered `ring^locant` aliases, including names
  originally written with primes; superscript assembly connections convert to
  primed locants before assembly.
- Fused-ring bridges are grouped by the following group and processed in
  first-encounter order, so multiple ring groups receive their own bridges.

## Wider parity

Outside Cargo, a development harness compares canonical SMILES (with the semantic
CXSMILES extension) from this crate and from a jar built at `42e87ac6`. Both agree
on every name, or both fail, in four corpora: upstream test resources at that commit
(1,486 names), this crate's earlier 2.9.0 fixture inputs (1,209), 702 PubChem names
from ReShiki's structure-to-name benchmark, and 261 phane names (upstream tests
and constructed examples).
