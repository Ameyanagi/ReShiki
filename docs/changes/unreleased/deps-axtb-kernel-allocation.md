---
section: Fixes
issues: []
---

- Small xTB calculations no longer reserve matrix-packing buffers proportional to an entire large CPU cache. The packaged calculation check keeps its 256 MiB memory limit, with calculation settings and results unchanged.
