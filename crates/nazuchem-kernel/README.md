# nazuchem-kernel

Internal molecular perception kernel for nazuchem. This first extraction copies
the transitive chemistry modules used by OPSIN and openclatura from ReShiki at
`be63a799aa03a83dc6ec74bb294f4d42b89298d3`, preserving the existing algorithms,
platform sort policies, attribution comments, and relevant unit tests.

The module inventory records 88 copied Rust module files and 27,886 original
source lines, plus the required `AtomFacts`, `Element`, atomic table exports,
module declarations, and crate attributes from the original root. The unused
generated-wedging module and the host allocation-measurement test stay outside
the extraction. Four existing JSON test fixtures are copied without changes.

Molecular perception is adapted from RDKit under BSD-3-Clause. Extended
tetrahedral CIP code is adapted from SiMolecule centres under BSD-2-Clause.
Original ReShiki/nazuchem code uses MIT OR Apache-2.0. The complete existing
upstream notices remain at the repository root in `licenses/rdkit/` and
`licenses/centres/`; the per-crate notice packaging and full provenance audit
are scheduled for naming-split step 2. The current manifest retains the
chemistry crate's existing license expression during step 1.

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
