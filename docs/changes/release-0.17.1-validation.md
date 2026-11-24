# ReShiki 0.17.1 release validation

This release collects the equipment and release workflow changes on `main`
through [`4c3ea6b45`](https://github.com/Ameyanagi/ReShiki/commit/4c3ea6b45e38b5f0bb5e8d4dee097cae4ada3464).
The application and Python workspace package use version 0.17.1. The tag
sets the same version for both npm packages and the browser demo.

[Release notes](../changes-0.17.1.md) · [Release procedure](../releasing.md)

## Checks

- The equipment and Intel workflow pull requests passed CI and Greptile. The
  integrated equipment tree passed 204 model tests and 41 application tests on
  macOS arm64. The bend workflow was also tested in the running macOS app.
- The preparation's Cargo metadata reports 0.17.1 for the application and Python
  package. The release script's tag check accepts `v0.17.1`. All nine CLI
  reference tests passed. The Python suite passed 279 tests and skipped 12.
  The site build verified 14,738 local links and assets.
- The [Release builds workflow](https://github.com/Ameyanagi/ReShiki/actions/workflows/release.yml)
  records the package rehearsal and stable checks. Stable publication requires
  all six desktop targets, both signed and notarized Mac packages, and the
  live reference suite.

## Developer changes

- The Intel release job uses the built application for a bounded naming
  smoke test in place of a second full Rust library pass. The removed pass
  took 38 minutes 30 seconds in the v0.17.0 release job. Its replacement passed
  on the published Intel application under Rosetta. The Intel package, worker,
  clipboard and print checks and the ARM and live reference coverage continue.
  [PR #1409](https://github.com/Ameyanagi/ReShiki/pull/1409) · [Timing evidence](https://github.com/Ameyanagi/ReShiki/actions/runs/45269955885/job/138099388694)
