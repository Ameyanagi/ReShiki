# Original Assistant benchmark fixtures · version 1

These are original synthetic ReShiki drawings, contributed under **MIT OR
Apache-2.0**. No third-party source image is copied. This small QA cohort is not
representative of chemical literature, photographs or scans.

The independently authored reference SMILES describe ethanol, Gly–L-Ala,
cyclo(Gly₄), and sodium N-Boc-L-alaninate. RDKit 2026.03.6 independently checks
formula, heavy-atom/bond counts, specified CIP chirality and fragment count.
The native `.rsk` files retain editable graph connectivity and source geometry;
reference `.json` files are never supplied to the model.

Source PNGs were rendered through the exact `51fa0991` Nightly's existing
`assistant_smoke --render` path. `manifest.json` pins the renderer binary hash,
input hashes, sizes, attribution, license and reference assignment. Half-resolution
and low-contrast variants retain the same complete references. Cropped and
11×5-pixel inputs have no unique complete graph reference; their uncertainty
messages are assessed separately.

Reproduce with `scripts/prepare_assistant_benchmark_inputs.py --renderer
/absolute/path/to/frozen/assistant_smoke --output /absolute/path/to/new/fixtures`.
Use the development Python dependencies from `uv.lock`. Generated native
coordinates and raster output can differ when tooling changes; preserve this
version and compare hashes before measuring another run.

The full method, budgets and limitations are in
[the benchmark guide](../../../../docs/assistant-benchmark.md).
