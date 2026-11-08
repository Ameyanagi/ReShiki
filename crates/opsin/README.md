# OPSIN Rust migration

This independent crate ports the algorithms and resources of OPSIN main (3.0-SNAPSHOT),
commit `42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91`, into native Rust.
It has no Java, JAR, subprocess, Python, network service, or ReShiki dependency
at build time or runtime. The original MIT license is included in `LICENSE`.

The complete upstream stage chain is connected to `Parser::parse`, including
component processing, phane amplification, suffixes, fragment assembly, fused
rings and stereo.
The native differential gates match all 2,624 strict/radical corpus records and
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

`Parser::new()` uses an unbounded chemistry budget. Hosts can pass their own
limits and memory reservation hook to `Parser::with_budget(opsin::Budget)`;
the parser forwards it to chemistry perception during structure construction.
The library does not detect machine capabilities or access a process-wide
reservation pool. Compatibility tests inject ReShiki's existing resource policy
through a development-only adapter.

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

This port preserves the pinned upstream revision’s interpretation choices and limitations,
including its CIP rules 1–2 and unsupported stereochemistry behavior. The
frozen gates establish the checked corpus and configurations; they do not
promise interpretation of every possible chemical name or newer OPSIN behavior.

The native biaryl extension also interprets locanted M/P and Ra/Sa axial
descriptors (Ra = M, Sa = P). Use
`Structure::semantic_cxsmiles_with_locants_and_axes` to export these structures:
its axes and principal-parent locants share the accompanying SMILES atom order.
Each axis includes its two endpoints, a reference neighbour at each end, the
rotation from the first reference to the second looking along the endpoint
order, and the requested M/P descriptor. Native CIP assigns the configuration
using isotope masses and resonance-averaged mancude duplicates. The SMILES-only
semantic methods return an error when the structure has an axis, because SMILES
cannot retain that configuration.
Unlocanted helicity, planar Rp/Sp and unresolved ligand priorities remain
uninterpretable. See the native resource extension in [provenance](provenance/README.md).
