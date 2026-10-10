These frozen drawings exercise secondary strokes on retained XYZ projections.
Their molecular coordinates and bond orders are identical before and after the
rendering fix; tests and visual comparisons never rerun geometry optimization.

- `c60.rsk`: 60 carbon atoms, 90 bonds, 30 Kekulé doubles. The initial XYZ and
  InChI come from `native/geometry/tests/fixtures/c60-reference.json`. RDKit
  2026.03.6 optimized that one conformer using MMFF94s, maxIters=2000, status=0.
  It is rotated 28° about X then 32° about Y.
- `c70.rsk`: 70 carbon atoms, 105 bonds, 35 Kekulé doubles. Its graph is the
  [PubChem CID 16131935 InChI](https://pubchem.ncbi.nlm.nih.gov/compound/16131935).
  RDKit 2026.03.6 ETKDGv3 (useBasicKnowledge=false, seed=5395275, one thread,
  maxIterations=100, timeout=15) generated one conformer, followed by MMFF94s
  optimization with maxIters=2000, status=0. It uses the resulting front view.

Both drawings scale their mean XYZ bond length to 42 drawing units. C70 is a
rendering control for a frozen optimized cage, not a claim that every C70
geometry-generation path or larger fullerene is validated. Optimized local
faces are slightly warped: maximum edge/centroid-plane deviation divided by
mean bond length is approximately 0.028 for C60 and 0.154 for C70. The renderer
uses a bounded local tangent approximation and clips against the actual
projected face; it does not flatten or edit the molecular coordinates.
