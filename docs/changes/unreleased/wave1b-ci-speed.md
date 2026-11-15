---
section: Developer
issues: []
---

- Pull request checks keep their build cache. Release, nightly and daily
  corpus builds no longer save caches, and the geometry job saves one cache
  instead of six. Together they had pushed the repository over GitHub's 10 GB
  cache limit.
- `scripts/build_inchi_helper.py` builds for the host without `--target` when
  none is requested. CI omits it so the test helper and the app share one build.
  A cold build of both took 409 seconds instead of 826 on an Apple Silicon Mac.
- CI runs two of the tests that start the naming worker at a time, not one.
  `cargo nextest` stops a test after 15 minutes, and each workspace test partition
  uploads its JUnit report.
- On a pull request, the summary of the **Changed fixture rows** job lists
  the rows that each changed test fixture gains and loses. See
  [CI](development.md#ci).
- The macOS Python checks and the Apple Silicon geometry tests run in the
  macOS lint job, so a full run takes two fewer macOS runners.
- Clippy runs in a pre-push hook, not on each commit. The Python hooks no
  longer run for `Cargo.toml` and `Cargo.lock`. Run
  `bun run --bun lefthook install` once to add the pre-push hook.
