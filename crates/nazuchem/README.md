# nazuchem

Chemical naming through the existing string APIs: chemical names to semantic
CXSMILES and molecular SMILES to names with preference assessments.

The name-to-structure engine is a Rust port of OPSIN by Daniel Lowe (MIT).
The structure-to-name engine began as a port of openclatura 0.4.3, extended
in ReShiki. The chemistry kernel contains RDKit and SiMolecule centres
adaptations; an internal copy preserves their upstream provenance.

Extracted from [ReShiki](https://github.com/Ameyanagi/ReShiki), a molecular drawing app.

```rust
fn main() -> Result<(), nazuchem::Error> {
    #[cfg(feature = "n2s")]
    {
        // Equivalent to CCO; atom order follows the parser.
        assert_eq!(nazuchem::name_to_smiles("ethanol")?, "C(C)O");
    }
    #[cfg(feature = "s2n")]
    {
        let name = nazuchem::smiles_to_name("CCO")?;
        assert_eq!(name.text, "ethanol");
    }
    Ok(())
}
```

The default features are `n2s` and `s2n`. Disable default features and select
either independently. `nazuchem::n2s` and `nazuchem::s2n` expose the engine
APIs; shared vocabulary and `Budget` are re-exported at the facade root.
`name_to_smiles` uses strict OPSIN options, rejects parser warnings and returns
semantic CXSMILES. Axial configurations require the engine API that returns
axes separately. `smiles_to_name` uses IUPAC isotope notation and returns the
engine's complete `Name` result.

## Scope and limits

`n2s` interprets supported IUPAC/CAS-style systematic and retained names
according to the pinned OPSIN 3.0-SNAPSHOT revision; it does not cover every
chemical name or promise newer OPSIN behavior. `s2n` names chains,
monocycles, retained and systematic fused parents, spiro and von Baeyer ring
systems, supported ions and charge-balanced salts. It also names supported
neutral mono-/diradicals and radical ions when a closed-shell neutral parent
can be recovered from one charge and one unpaired electron on the same atom
or the two ends of a broken pi bond.

`s2n` input is limited to 32,768 bytes and 512 atom tokens, with 1–512 atoms
after import. It rejects CXSMILES extensions, reaction SMILES, allene and
coordination stereo classes, mixtures, neutral components mixed with ions,
and isotope-labelled neutral radicals. Unsupported chemistry or naming paths
return `Err`; comprehensive IUPAC coverage is not claimed. A successful name
includes a preference assessment and is not necessarily the preferred IUPAC
name. This is a pre-release 0.x API; compatibility is not yet guaranteed.

Both convenience functions use `Budget::default()`, which is unbounded.
Hosts can pass explicit budgets to `n2s::Parser::with_budget` and the s2n
entry points. A budget's optional analysis timeout covers kernel molecular
perception only; it is not a deadline for the complete naming operation.
The library performs no I/O, creates no threads and has no
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
