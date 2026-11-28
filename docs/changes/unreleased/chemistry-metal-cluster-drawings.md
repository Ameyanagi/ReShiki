---
section: Fixes
issues: []
---

- Show as 2D drawing chooses a physical tilt for supported metal clusters, using one ruler in which the theme bond length represents 1.54 Å. After an xTB optimization, the calculation reports changes in hydride bridging alongside convergence.

  [Previous drawing](images/metal-clusters/diruthenium-baseline.png) · [Updated g-xTB drawing](images/metal-clusters/diruthenium-gxtb.png) · [GFN2-xTB drawing](images/metal-clusters/diruthenium-gfn2.png).

  Native `.rsk` and figure exports retain the examples' appearance. Direct CDXML
  export currently rejects their non-stereochemical front-bond emphasis; the
  topology-only CDXML regression does not cover that appearance.
