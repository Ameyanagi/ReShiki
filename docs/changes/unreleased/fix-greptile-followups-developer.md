---
section: Developer
issues: []
---

- The website build reads the assembled unreleased changes as UTF-8 on Windows
  whatever the code page is, and a change to `scripts/assemble_changelog.py`
  alone now runs the documentation checks and deployment.
