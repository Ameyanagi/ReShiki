---
section: Developer
issues: []
---

- Each pull request adds its release note as one file in
  [`changes/unreleased/`](changes/unreleased/README.md) instead of editing this
  page or the contributor list. `scripts/assemble_changelog.py` checks the
  files, assembles this page and moves them into the release notes.
- macOS disk images use LZMA compression and are about 40% smaller.
- Release executables and the Python package carry no symbols. The Release
  builds workflow keeps them as a separate symbol file for crash reports.
- `vendor/README.md` lists every vendored crate with its upstream version, its
  reason and its patch; `scripts/check_vendor.py` checks each one against its
  crates.io source.
- The old getting-started page redirects to the
  [install guide](https://reshiki.com/guide/install/). The development,
  architecture and roadmap notes match the current toolchain, CI and crates.
