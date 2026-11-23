---
section: Changes
issues: []
---

- Python PNG rendering converts pixels in place. Batch rendering shares a
  128 MiB budget for active output bitmaps across simultaneous batches,
  preserving input order, warnings and per-item failures.
