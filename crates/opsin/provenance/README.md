# Pinned source and migration evidence

Source: [OPSIN 2.9.0](https://github.com/dan2097/opsin/tree/b91b610af5ab07560fedb20730d7aef46bb2bca0),
commit `b91b610af5ab07560fedb20730d7aef46bb2bca0`, MIT license,
copyright 2017 Daniel Lowe. `../LICENSE` is an unmodified upstream copy.
`manifest.json` records every original XML/DTD file and migrated table hash.
All original XML/DTD files are byte-equal to the pinned official core JAR.

The development oracle is the unmodified official
`opsin-core-2.9.0-jar-with-dependencies.jar`, SHA256
`627ee5da4af551f9c4d1d766f545eb7cf519a344776e0bb677247a47abac0252`.
It is never embedded, loaded, spawned or downloaded by this Rust crate.
Cargo builds and tests consume only native resources and frozen fixtures.

`ExportDfa.java.txt` is a record of the one-time data migration program, not a
Cargo input. It reads upstream `RunAutomaton` tables and emits a neutral binary
format; Rust does not parse Java serialization. All 46 automata are exported:
chemical grammar in both directions, and 22 DFA token recognizers in both
directions. The two remaining regex tokens are the directly ported zero-width
ASCII-letter boundary predicates `Ă` and `ă`.

The table format is big endian: `OPSINDFA`, u32 format version 2, u32 automaton
count; per automaton, u32 ASCII-name byte length and name, u32 initial state,
u32 state count, u32 character-interval count and those u32 UTF-16 intervals;
per state, u8 acceptance flag and u32 transition count, then (low UTF-16 code
unit, high UTF-16 code unit, target state) u32 triples. Omitted transitions
reject. Adjacent equal-target intervals are merged. Original state IDs and
annotation-interval ordering are retained. Exporting twice from separate
working directories produces identical bytes:
`1ea9b52a510d6681aaddc4d185101bbdeaddc0247e0a198a311d987826431492`.

To reproduce the migration with an already available trusted JDK, use a fresh
empty working directory so OPSIN's development `./resources` override cannot
intercept resource loading. Paths below should be absolute:

```sh
cp /path/to/crates/opsin/provenance/ExportDfa.java.txt /tmp/export/ExportDfa.java
javac -cp /path/to/pinned-opsin-core.jar -d /tmp/export /tmp/export/ExportDfa.java
cd /tmp/empty-working-directory
java -cp /path/to/pinned-opsin-core.jar:/tmp/export \
  uk.ac.cam.ch.wwmm.opsin.ExportDfa /tmp/export/automata.bin
```

`ExportFrontend.java.txt` records the frozen frontend fixture generator. Its
first argument is a UTF-8 file of distinct input names (one per line); its
second argument is the output JSONL path. Inputs comprise the first tab-separated
column of each nonblank, noncomment line in the 27 upstream integration resource
files, in filename/line order, followed by explicit tokenization edge cases.
Deduplication preserves first occurrence. Every name is processed through
upstream `PreProcessor` before six lexical modes are captured. The checked
fixture contains 1,073 names × six modes, requiring no live oracle in tests.

`ExportParseTrees.java.txt` records complete Parser output and stable SortParses
ranking for 1,080 names under four configurations: radicals false/true crossed
with detailed failure analysis false/true. `parse-tree-golden.jsonl.gz` has
4,320 records, including exact ordered XML attributes and errors. Its native
differential gate passes all records.

`ExportComponents.java.txt` uses the same tab-separated family/name input and
four configurations. After upstream ranking, it captures each candidate's XML
before and after ComponentGenerator, or its error, and ordered warnings. The
4,320 rows contain 4,736 candidates. `component-golden.jsonl.gz` therefore
supports testing transformations independently of the Rust frontend.

`ExportFusedNumbering.java.txt` takes tab-separated test name, SMILES and
expected labels. `fused-numbering-inputs.tsv` includes all 81 active
FusedRingNumbererTest examples. The frozen JSONL records SSSRFinder's ordered
atom/bond lists, final atom ordering, per-original-atom locants, and errors.

`ExportFusedBuilding.java.txt` reads compact root XML from
`fused-building-inputs.tsv`, attaches the root to a dummy word (the same
context required by ComponentProcessor), resolves each group with numeric
labels, and applies FusedRingBuilder. The 11 captured records compare ordered
XML and fragment atom/locant/spare-valency/bond snapshots, including failures.
`ExportSuffixRules.java.txt` captures all 997 group/suffix/subtype queries in
`suffix-rule-inputs.tsv`, including null, empty and mismatching subtypes and
unknown types/suffixes. It records ordered tags/attributes and exact errors.

`ExportStructures.java.txt` captures all 1,048 active integration rows twice,
under strict defaults and with radicals allowed; all other flags remain false.
The 2,096 records in `structure-golden.jsonl.gz` include status, message,
ordered warnings, plain SMILES, semantic CXSMILES (mask 13), and a lossless
ordered graph with explicit hydrogen atoms, charge, isotope, valency and stereo
references/groups. Capturing a fixture does not establish native equivalence;
the migration ledger records executed acceptance gates separately.

`ExportConfigurations.java.txt` crosses all five configuration flags for each
name in `configuration-inputs.txt`. The 20 names include upstream configuration
test examples, polymer/radical/stereo cases and failure analysis edges. All
640 frozen complete result records are exercised without a live oracle.

Compile each development exporter with the pinned JAR and the compiled
ExportFrontend class on its classpath; the latter supplies JSON quoting. Run
from a fresh working directory with absolute input/output paths. Exporter
source and every checked fixture hash are recorded in `manifest.json`.
Compressed JSONL uses gzip level 9 with timestamp zero and an empty filename.

The resource fingerprint is SHA256 over each resource in sorted UTF-8 path
order, concatenating path, NUL, raw file bytes, NUL. This independently covers
all XML/DTD files and the neutral DFA table. The exported crate constant is
`4269b61bf110f01d9fab808a13bac1aa4e29ec34800a7ff7cac5925d3774b9a4`.
