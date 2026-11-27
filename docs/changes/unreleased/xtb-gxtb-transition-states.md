---
section: Changes
issues: []
---

- Reaction calculations now offer GFN2-xTB and g-xTB. Refine a verified GFN2-xTB
  transition state in g-xTB and compare the barriers and reaction paths on the
  same arrow. Each IRC checks endpoints relaxed with its saddle’s method.
  The CLI accepts `--method gxtb` and `--refine gxtb`, and records the method
  alongside the energies, frequencies and connectivity checks.
