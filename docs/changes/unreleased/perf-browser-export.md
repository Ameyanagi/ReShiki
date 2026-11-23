---
section: Changes
issues: []
---

- Browser figure exports prepare and render in workers so the editor and host
  page remain responsive. Exports retain added fonts and existing output
  formats, with the existing local exporter as a fallback when a worker cannot
  initialize. Timeouts and cancellation stop the worker without repeating a
  dispatched export on the page.
