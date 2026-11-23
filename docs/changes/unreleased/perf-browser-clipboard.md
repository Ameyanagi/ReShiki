---
section: Changes
issues: []
---

- Browser clipboard preparation can run in a bounded local worker while
  preserving available formats, editable picture data and per-format notices.
  WebKit and unrecognized browser hosts keep clipboard preparation local to
  preserve their user-gesture access; separate exports still use the worker.
