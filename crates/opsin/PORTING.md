# Source-to-Rust migration ledger

Pinned source: OPSIN 2.9.0, `b91b610af5ab07560fedb20730d7aef46bb2bca0`.
The source contains 84 core Java files (approximately 36,000 lines), 40 XML
files, seven DTDs, 31 core test files, 570 ordinary test declarations and three
parameterized declarations. Integration resources contain 1,048 active rows
across 27 nomenclature families. The upstream MIT license covers translated
algorithms, resources and copied fixtures.

The following ledger records source phases and executed native acceptance
gates. Passing a frozen corpus does not promise interpretation of every possible
chemical name or reproduce behavior from a newer upstream revision.

| Upstream source                                                                                                                                          | Rust module                                                               | State and evidence                                                                                                                                                                                                       |
| -------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `ResourceManager`, `AutomatonInitialiser`                                                                                                                | `resources`, `automaton`, `resource_data`, embedded XML/DTD/DFA           | Complete resource loading; all 40 XML and seven DTD files byte-equal the pinned JAR; deterministic migration of all 46 forward/reverse DFA tables                                                                        |
| `OpsinRadixTrie`                                                                                                                                         | `trie`                                                                    | All dictionary prefix matches and empty marker tokens; exercised by the full lexical oracle                                                                                                                              |
| `PreProcessor`, normalization in `StringTools`                                                                                                           | `preprocess`, `preprocess_characters`                                     | Exact source mappings and normalization order; upstream cases plus corpus normalization                                                                                                                                  |
| `ParseRules`, `ReverseParseRules`, `AnnotatorState`                                                                                                      | `parse_rules`                                                             | Native DFS, upstream annotation order/longest acceptance/ambiguity bound, case-sensitive DFA tokens, both zero-width predicates                                                                                          |
| `Tokeniser`, `WordTools`                                                                                                                                 | `tokenizer`                                                               | Omitted-space splitting, longest functional term, bracket-space repair, reverse-space recovery, CAS index/compound-with handling; frozen oracle gate passes                                                              |
| `Element`, `GroupingEl`, `TokenEl`, `Parser`, `WordRules`, omitted-space corrector, `CASTools`, `SortParses`                                             | `parse_tree`, `frontend`, `word_rules`, `word_rules_omitted_space`, `cas` | Complete frontend gate: 4,320 exact raw/ranked XML comparisons; native CAS and omitted-space fixtures and the complete pipeline gate pass                                                                                |
| `Atom`, `AtomParity`, `Bond`, `BondStereo`, `ChemEl`, `Fragment`, `OutAtom`, `ValencyChecker`, `AtomProperties`, `SMILESFragmentBuilder`, `SMILESWriter` | `graph`, `valence`, `smiles`                                              | Source foundation present; resource SMILES snapshots, source writer/parser examples and graph/stereo/substitution gates pass; complete assembly fidelity still gated below                                               |
| `ComponentGenerator`, XML declarations, tree helpers                                                                                                     | `component_generator`, `xml_declarations`, `tree_tools`                   | Complete intermediate gate: all 4,736 ranked candidates' ordered XML/errors/warnings match; 25 focused tests pass                                                                                                        |
| `FragmentManager`, `FragmentTools`, `BuildState`, `BuildResults`                                                                                         | Construction state and graph utilities                                    | Source port present; stable registration, cloning, shared fragment/out-atom views, identity-aware ambiguous atom sets and graph helpers; 12 manager regression tests and complete corpus gate pass                       |
| `ComponentProcessor`, `SuffixRules`, `SuffixApplier`, `FunctionalReplacement`                                                                            | Component processing and suffix/replacement modules                       | All source method inventories present; 997 exact suffix queries, 27 processing tests, 38 ring tests, 37 carbohydrate tests, 26 bracket tests and 40 suffix/isotope tests pass; complete corpus gate passes               |
| `StructureBuilder`, `StructureBuildingMethods`                                                                                                           | Assembly modules                                                          | All source word rules and attachment modes connected; complete strict/radical/configuration corpus gates and 19 source-derived assembly fixtures pass                                                                    |
| `FusedRingBuilder`, `FusedRingNumberer`, cycles/SSSR helpers                                                                                             | Ring modules                                                              | Source methods present; 81 exact numbering/SSSR cases, 11 exact resolved-fragment fusion snapshots and nine branch tests pass                                                                                            |
| `StereoAnalyser`, `CipSequenceRules`, `StereochemistryHandler`, `AmbiguityChecker`                                                                       | Stereo/ambiguity modules                                                  | Source methods present; CIP gate includes all 15 source priorities and unsupported tie; 14 handler cases plus frozen analyser/environment/substitution graph gates pass; complete stereo/configuration corpus gates pass |
| `NameToStructure`, `OpsinResult`, `NameToStructureConfig`                                                                                                | `Parser`, `api`, `pipeline`                                               | Complete stage chain and source candidate selection connected; all 2,096 strict/radical and 640 configuration records match exactly                                                                                      |

## Current lexical gate

`tests/grammar_oracle.rs` compares 6,438 frozen pinned-OPSIN records: 1,073
distinct names from all active integration resources and explicit edge cases,
in six modes (forward/reverse annotation, forward/reverse tokenization with and
without space removal). It checks normalized input, every token and annotation
in order, all word alternatives, and exact failure/error fields. The gate does
not run component transformations or molecular assembly.

The complete lexical test process on the development macOS host used 51,757,056
bytes peak RSS and 3.60 seconds wall time in an unoptimized test build. This
does not predict complete construction or worst-case inputs.

## Complete result gate

`tests/structure_oracle.rs` compares all 1,048 integration names under strict
defaults and radicals allowed: 2,096 frozen records. It checks exact status,
message, ordered warnings, semantic CXSMILES, and ordered graph metadata and
stereo references. It reports every nomenclature family's counts; optional
`OPSIN_ORACLE_FAMILY` and `OPSIN_ORACLE_NAME` filters support diagnosis without
changing the default complete gate. `OPSIN_ORACLE_REPORT` optionally writes
compact per-record diagnostic JSONL to the provided local path.

All 2,096 complete records now match exactly in every one of the 27 families.
The unoptimized development test process completes this gate in 7.13 seconds.
`tests/configuration_oracle.rs` also matches 640 complete results: 20 source and
edge names crossed with all 32 combinations of the five upstream flags. Those
fixtures cover radical wildcard attachment labels, polymer semantics, acid
omission, unsupported stereo warnings/failures, and detailed failure analysis.

The final Java-free `cargo test -p opsin --all-targets --offline -j4` run passes
all 318 test functions across 38 integration targets, with no failed or ignored
tests. This includes the complete result gates and the intermediate oracles.
Strict `cargo clippy -p opsin --all-targets --offline -j4 -- -D warnings` also
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
7. Run all 1,048 integration rows with their actual upstream setting
   `allow_radicals = true`, with per-family counts and explicit failures.
8. Compare status, warnings, semantic CXSMILES and graph identity, including
   isotope, charge, hydrogen placement, components, bond order and stereo
   groups. The upstream pruned-InChI gate omits fixed-H/reconnected layers and
   alone cannot establish exact tautomer or connectivity fidelity.
9. Verify the shipping dependency graph, release artifact and application
   naming worker have no Java/JAR invocation or network fallback. Preserve
   application rejection of unsupported identity semantics.
