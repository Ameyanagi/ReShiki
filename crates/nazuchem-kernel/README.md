# nazuchem-kernel

Internal molecular perception kernel for nazuchem. This first extraction copies
the transitive chemistry modules used by OPSIN and openclatura from ReShiki at
`be63a799aa03a83dc6ec74bb294f4d42b89298d3`, preserving the existing algorithms,
platform sort policies, attribution comments, and relevant unit tests.

The module inventory records 88 copied Rust module files and 27,886 copied
source lines, plus the required `AtomFacts`, `Element`, atomic table exports,
module declarations, and crate attributes from the original root. The unused
generated-wedging module and the host allocation-measurement test stay outside
the extraction. Four existing JSON test fixtures are copied without changes.

Molecular perception is adapted from RDKit under BSD-3-Clause. Extended
tetrahedral CIP code is adapted from SiMolecule centres under BSD-2-Clause.
Apple and Windows sorting policies adapt LLVM libc++ and Microsoft STL under
Apache-2.0 WITH LLVM-exception. Original ReShiki/nazuchem code uses
MIT OR Apache-2.0. Complete applicable third-party texts and notices travel
with this crate in `licenses/rdkit/`, `licenses/centres/` and `licenses/stdlib/`;
`LICENSE-MIT` and `LICENSE-APACHE` cover the local contributions. Root copies
remain because ReShiki also retains the chemistry implementations.

The manifest combines these established terms as
`(MIT OR Apache-2.0) AND BSD-3-Clause AND BSD-2-Clause AND (Apache-2.0 WITH LLVM-exception)`.
This expression does not resolve the source/licensing allocation of
`native_order/linux_sort.rs` and `native_order/sort_common.rs`: their ledger
licences remain `NOASSERTION`, pending the owner's sort-policy decision.
`publish = false` remains in force. `PROVENANCE-nazuchem.toml` at the repository
root records per-file ancestry, local function boundaries and the unresolved
facts; the crate's `NOTICE` explains its incorporated scope.

`budget::Budget` is explicit at entry points that perform aromaticity
perception, including SMILES import, sanitization, and stereo axis validation.
`Budget::default()` is unbounded. A host can supply heap and work limits, an
optional analysis timeout, and a reservation callback whose returned guard is
held for the operation. The kernel does not detect machine capabilities or
maintain a process-wide reservation pool. `budget::Cancellation` supports
cooperative cancellation through `aromaticity::perceive_with_budget`.

```rust
use nazuchem_kernel::{budget::Budget, smiles};

let budget = Budget::default();
let molecule = smiles::read("C[C@H](O)C(=O)O", &budget)?;
let canonical = smiles::write::write(&molecule.prepared.state, Default::default())?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

This crate performs no I/O and creates no threads. Its resource policy has no
global mutable state. Existing structural and algorithmic limits remain in
place; worst-case time on adversarial input is unbounded. Hosts needing hard
limits should supervise the operation externally.
