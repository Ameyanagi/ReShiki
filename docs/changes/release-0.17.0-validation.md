# ReShiki 0.17.0 release validation

This release includes the changes on `main` through
[`f3b643c74`](https://github.com/Ameyanagi/ReShiki/commit/f3b643c74b2d350dcd90aca802a406f138be9372).
The preparation updates the application and Python workspace package to 0.17.0
and collects 141 release-note fragments. The tag publishes the two npm
packages using the same application version.

[Release notes](../changes-0.17.md) · [Release procedure](../releasing.md)

## Checks

- The seven final equipment and CI pull requests passed CI and Greptile.
  The combined tree passed 174 equipment model tests and 27 equipment
  application tests on macOS arm64.
- The changelog assembler validated all 141 source fragments and the
  release script accepted `v0.17.0` against the updated Cargo versions.
- The local Python suite passed 279 tests and skipped 12. Rust formatting,
  JavaScript lint and the site build passed. Astro checked 18 files with no
  diagnostics, and the build verified 14,500 local links and assets in the checked documentation build.
- The [package rehearsal](https://github.com/Ameyanagi/ReShiki/actions/runs/45179150561) uses `89bc0c32`.
  Follow-up commits update documentation and allow 150 minutes for Windows x64
  packaging in every release mode. The rehearsal reached the worker-pipe test
  after 80 minutes. That test and the geometry check recently took just over
  20 minutes in the [passing nightly build](https://github.com/Ameyanagi/ReShiki/actions/runs/45170775910/job/137679889278).
- The stable [Release builds workflow](https://github.com/Ameyanagi/ReShiki/actions/workflows/release.yml)
  requires the six desktop targets, signed and notarized macOS downloads, and
  the full live RDKit reference suite before publishing.

## Developer changes

- The headless JavaScript package predicts ¹H and ¹³C NMR shifts with `predictNmr`, returning the same rows, reference statistics and data credit as the command line and Python. ([#1232](https://github.com/Ameyanagi/ReShiki/issues/1232)) [PR #1326](https://github.com/Ameyanagi/ReShiki/pull/1326)
- The macOS disk image build converts a fresh copy of the intermediate image and
  tries each `hdiutil` step up to five times with a growing wait, after the
  0.16.0 tag build failed three times with "Resource temporarily unavailable"
  ([Releasing](../releasing.md)). [PR #1253](https://github.com/Ameyanagi/ReShiki/pull/1253)
