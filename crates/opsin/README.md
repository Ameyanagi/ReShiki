# OPSIN Rust migration

This independent crate ports the algorithms and resources of OPSIN 2.9.0,
commit `b91b610af5ab07560fedb20730d7aef46bb2bca0`, into native Rust.
It has no Java, JAR, subprocess, Python, network service, or ReShiki dependency
at build time or runtime. The original MIT license is included in `LICENSE`.

The complete upstream stage chain is connected to `Parser::parse`, including
component processing, suffixes, fragment assembly, fused rings and stereo.
The native differential gates match all 2,096 strict/radical corpus records and
640 configuration records exactly, including status, warnings, semantic
CXSMILES and ordered graph/stereo metadata. See
[the source/module ledger and acceptance gates](PORTING.md).

```rust
use opsin::{Parser, ParseOptions, Status};

let parser = Parser::new()?;
let result = parser.parse("ethanol", &ParseOptions::strict());
if result.status == Status::Success {
    let cxsmiles = result.structure.as_ref().unwrap().semantic_cxsmiles()?;
}
```

The options preserve all five upstream flags, initially false. Results retain
the exact input, status, warnings, and failure message. The public graph model
preserves the construction state, locants, attachment points, isotope, charge,
hydrogen/parity references, and enhanced stereo groups needed by OPSIN.
Semantic CXSMILES uses upstream mask 13: enhanced stereo, polymers, and atom
labels; it omits cosmetic atom values such as naming locants.

`Parser::parse_word`, `parse_word_reverse`, `tokenize`, and `tokenize_reverse`
expose the actual upstream annotation and word-splitting algorithms. Their input
must already be ASCII-normalized with `preprocess::preprocess`, as in OPSIN's
normal conversion pipeline. A recognized word need not be a valid molecule.

Regular Cargo tests read frozen oracle fixtures and require no Java. Pinned
upstream source/resource hashes and the development-only export procedure are
recorded in [provenance](provenance/README.md).

This port preserves OPSIN 2.9.0's interpretation choices and limitations,
including its CIP rules 1–2 and unsupported stereochemistry behavior. The
frozen gates establish the checked corpus and configurations; they do not
promise interpretation of every possible chemical name or newer OPSIN behavior.
