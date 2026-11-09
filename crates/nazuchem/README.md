# nazuchem

Chemical naming through the existing string APIs: chemical names to semantic
CXSMILES and molecular SMILES to names with preference assessments.

The name-to-structure engine is a Rust port of OPSIN by Daniel Lowe (MIT).
The structure-to-name engine began as a port of openclatura 0.4.3, extended
in ReShiki. The chemistry kernel contains RDKit and SiMolecule centres
adaptations; an internal copy preserves their upstream provenance.

```rust
// Equivalent to CCO; atom order follows the parser.
assert_eq!(nazuchem::name_to_smiles("ethanol")?, "C(C)O");
let name = nazuchem::smiles_to_name("CCO")?;
assert_eq!(name.text, "ethanol");
# Ok::<(), nazuchem::Error>(())
```

The default features are `n2s` and `s2n`. Disable default features and select
either independently. `nazuchem::n2s` and `nazuchem::s2n` expose the engine
APIs; shared vocabulary and `Budget` are re-exported at the facade root.
`name_to_smiles` uses strict OPSIN options, rejects parser warnings and returns
semantic CXSMILES. Axial configurations require the engine API that returns
axes separately. `smiles_to_name` uses IUPAC isotope notation and returns the
engine's complete `Name` result.

Both convenience functions use `Budget::default()`, which is unbounded.
Hosts can pass explicit budgets to `n2s::Parser::with_budget` and the s2n
entry points. The library performs no I/O, creates no threads and has no
process-wide mutable naming state. The parser shares immutable embedded
resources; fusion retains bounded thread-local caches. Worst-case time on
adversarial input is unbounded; hosts needing hard limits should supervise
requests externally.

Original facade code is MIT OR Apache-2.0. The package expression also ANDs
the engines' and kernel's established terms: MIT (OPSIN and openclatura),
BSD-3-Clause (RDKit), BSD-2-Clause (centres), Apache-2.0 WITH LLVM-exception
(LLVM libc++ and Microsoft STL), and Python-2.0 (a CPython adaptation).
Complete license texts, copyright notices and dependency scope notices are
included in `licenses/`; `LICENSE-MIT` and `LICENSE-APACHE` cover local code.

The dependencies' unresolved native sort-policy and IUPAC Blue Book table
provenance remains unresolved. Those files retain `NOASSERTION` in the
repository-root `PROVENANCE-nazuchem.toml`; the established package expression
does not grant rights for that material. `publish = false` remains in force.
The crate's `NOTICE` and copied dependency notices describe the exact scope.
