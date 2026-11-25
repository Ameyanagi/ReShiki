---
section: Developer
issues: []
---

- Opened Molden and Gaussian fchk files keep orbital shapes whose conventions
  are proven against independent references: PySCF 2.x Molden exports (pure and
  Cartesian d, f and g), ORCA `orca_2mkl` exports (s, p, pure d and pure f) and
  Gaussian 09/16 fchk pure d. Other shapes are still left out with a warning.
  `scripts/qc_orbital_reference.py` regenerates the signed reference values.
