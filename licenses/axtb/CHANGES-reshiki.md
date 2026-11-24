# Changes in the ReShiki fork of axtb

This branch (`reshiki` of https://github.com/Ameyanagi/axtb) is a modified version of axtb,
https://github.com/atomsforhumanity/axtb, taken at upstream commit
`65d0f64a9a8211f73b0738ff908b7a414975f50b` (axtb 0.2.0). axtb and these modifications are
licensed under the GNU Lesser General Public License, version 3 or (at your option) any later
version (COPYING, COPYING.LESSER, NOTICE). This file is the notice of modification that
section 5(a) of the GNU GPL v3, incorporated by the LGPL v3, asks for. Every change is a
separate commit on this branch; `git log 65d0f64..reshiki` lists them.

## 2026-11-25

- **Optional g-xTB (cargo feature `gxtb`, on by default).** `axtb-gxtb` becomes an optional
  dependency of `axtb-driver`, and the feature is forwarded by `axtb-cli` and `axtb-py`. The
  default build is unchanged. With `default-features = false` the g-xTB crate and its embedded
  parameter files are not in the dependency graph; the method names still parse, and a request
  for a g-xTB method fails with the input error `g-xTB is not included in this build of axtb
  (cargo feature "gxtb")`. Files: `Cargo.toml`, `crates/driver/{Cargo.toml,README.md,src,tests}`,
  `crates/axtb-cli/{Cargo.toml,src/main.rs,tests/gxtb.rs}`, `crates/axtb-py/Cargo.toml`,
  `README.md`. A g-xTB request to a build without the feature is refused before any other
  validation (`run`). Known limit: a Python wheel built without the feature still lists the
  g-xTB methods and reports them as unsupported elements; ReShiki builds only the Rust crates.
- **`third_party/` excluded from the workspace.** The patched `gemm-common` and
  `private-gemm-x86` copies are `[patch]` sources, not members; `exclude = ["third_party"]`
  in `Cargo.toml` lets a project that takes them from this repository (ReShiki mirrors the
  patches) build them with Nix's `importCargoLock`. No code or numerical change.
- **ReShiki validation.** New `reshiki/` directory (scripts, test structures) and
  `docs/reshiki/validation.md` (results). No engine code changes.
