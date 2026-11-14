---
section: Fixes
issues: [1181]
---

- **Clean up…** with **Selected atoms** no longer places the selected atoms on
  top of fixed atoms. From a layout such as RDKit's for a spiro compound, a
  selected ring could fold onto the ring beside it with no warning. ReShiki now
  turns the ring back to the free side. If the result is close to other atoms,
  or the free side is taken and the selection stays unchanged, a warning says so
  (fixes [issue #1181](https://github.com/Ameyanagi/ReShiki/issues/1181)).
- **Invert selection** shows the color of the newly selected bond in
  **Properties**, as every other selection change does.
- For developers: `cargo test --release --bin reshiki` compiles again, so the
  performance harnesses can run. Two caret tests that need a debug build are
  left out of a release test build, and CI builds the release test executable
  on Linux.
