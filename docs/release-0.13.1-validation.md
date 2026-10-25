# ReShiki 0.13.1 release validation

This is a release-preparation record, started on October 26, 2026. At the time
of this preparation, **no v0.13.1 tag, release workflow, package review or
publication had run**. Empty checks below are prerequisites, not completed
validation. Later workflow and package results are recorded in the preparation
pull request before a documentation follow-up.

[Prepared release notes](changes-0.13.1.md) · [Release procedure](releasing.md).

## Source and preparation status

The complete functional source base is
[`8e1392684dc627ab93f91f5c6443ddaa61ff2b39`](https://github.com/Ameyanagi/ReShiki/commit/8e1392684dc627ab93f91f5c6443ddaa61ff2b39).
It contains all 28 QA pull request groups below, including the corrected #538,
accepted #578 source and evidence, and #582. The release branch incorporated it
with an ordinary fast-forward merge before restoring the metadata draft.
Production behavior is unchanged relative to that complete base.

The final #578 matched native comparison met its three predeclared spatial
checks: 14.08, 7.16 and 13.53 pt cursor-to-hotspot gaps, compared with
215.63, 121.16 and 199.51 pt before. The bond length was 43.68 pt, with ±2 pt
measurement uncertainty. This source-pinned result is not an all-frame maximum,
event-to-paint latency, FPS or package-runtime claim. Its original host,
framing, sources and limits remain in the [QA record](changes/qa-pointer-hover.md).

- Release preparation pull request: pending.
- Tested preparation head and merge commit: pending.
- Manual six-platform package run and exact source head: pending.

These identities and results will be linked once observed. Keep the source and
version head stable during package validation; later run results can be added
externally and in the pull-request description without moving that head solely
for checkmark bookkeeping. Publication results belong in a later documentation
follow-up. Individual evidence keeps the commits it actually checked.

The preparation changes the root `reshiki` package version from 0.13.0 to
0.13.1 in `Cargo.toml` and `Cargo.lock`, and updates the published colour
reference version. Member crate and Python development-project versions stay
unchanged. The npm package manifest retains its development placeholder; the
release workflow sets the package version from `Cargo.toml`.

Document format 30 is already implemented for distance-derived bond provenance.
The release preparation does not change the format or application behavior.
Every new save uses format 30. Version 0.13.0 reads at most 29, and must refuse
these files rather than discard the provenance. Older drawings remain readable.

## Finding coverage and credits

The release inventory contains 64 findings, grouped into the 28 pull requests
below. Each group is credited to @Ameyanagi in the release notes and contributor
list. The linked review records retain their original source, input, platform,
measurements and limits; their successful checks do not establish checks on the
final release preparation or published packages.

| Pull request                                          | Findings                                                                                                                                                                                                                                                                                                                                             | Original evidence and limits                        |
| ----------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------- |
| [#535](https://github.com/Ameyanagi/ReShiki/pull/535) | [#513](https://github.com/Ameyanagi/ReShiki/issues/513)                                                                                                                                                                                                                                                                                              | [Review](changes/qa-513.md)                         |
| [#536](https://github.com/Ameyanagi/ReShiki/pull/536) | [#522](https://github.com/Ameyanagi/ReShiki/issues/522)                                                                                                                                                                                                                                                                                              | [Review](changes/qa-522.md)                         |
| [#537](https://github.com/Ameyanagi/ReShiki/pull/537) | [#490](https://github.com/Ameyanagi/ReShiki/issues/490)                                                                                                                                                                                                                                                                                              | [Review](changes/qa-490.md)                         |
| [#538](https://github.com/Ameyanagi/ReShiki/pull/538) | [#515](https://github.com/Ameyanagi/ReShiki/issues/515)                                                                                                                                                                                                                                                                                              | [Review](changes/artemisinin-depiction.md)          |
| [#539](https://github.com/Ameyanagi/ReShiki/pull/539) | [#507](https://github.com/Ameyanagi/ReShiki/issues/507)                                                                                                                                                                                                                                                                                              | [Review](changes/long-chain-layout.md)              |
| [#540](https://github.com/Ameyanagi/ReShiki/pull/540) | [#514](https://github.com/Ameyanagi/ReShiki/issues/514)                                                                                                                                                                                                                                                                                              | [Review](changes/qa-514.md)                         |
| [#541](https://github.com/Ameyanagi/ReShiki/pull/541) | [#521](https://github.com/Ameyanagi/ReShiki/issues/521)                                                                                                                                                                                                                                                                                              | [Review](changes/qa-521.md)                         |
| [#542](https://github.com/Ameyanagi/ReShiki/pull/542) | [#516](https://github.com/Ameyanagi/ReShiki/issues/516), [#520](https://github.com/Ameyanagi/ReShiki/issues/520)                                                                                                                                                                                                                                     | [Review](changes/qa-crystal-metadata.md)            |
| [#543](https://github.com/Ameyanagi/ReShiki/pull/543) | [#492](https://github.com/Ameyanagi/ReShiki/issues/492), [#493](https://github.com/Ameyanagi/ReShiki/issues/493), [#498](https://github.com/Ameyanagi/ReShiki/issues/498), [#500](https://github.com/Ameyanagi/ReShiki/issues/500), [#509](https://github.com/Ameyanagi/ReShiki/issues/509)                                                          | [Review](changes/qa-atom-label-editor.md)           |
| [#544](https://github.com/Ameyanagi/ReShiki/pull/544) | [#496](https://github.com/Ameyanagi/ReShiki/issues/496), [#501](https://github.com/Ameyanagi/ReShiki/issues/501), [#510](https://github.com/Ameyanagi/ReShiki/issues/510)                                                                                                                                                                            | [Review](changes/drawing-panel-state.md)            |
| [#545](https://github.com/Ameyanagi/ReShiki/pull/545) | [#494](https://github.com/Ameyanagi/ReShiki/issues/494), [#495](https://github.com/Ameyanagi/ReShiki/issues/495), [#497](https://github.com/Ameyanagi/ReShiki/issues/497), [#504](https://github.com/Ameyanagi/ReShiki/issues/504), [#506](https://github.com/Ameyanagi/ReShiki/issues/506)                                                          | [Review](changes/qa-drawing-geometry-2026-10-25.md) |
| [#557](https://github.com/Ameyanagi/ReShiki/pull/557) | [#511](https://github.com/Ameyanagi/ReShiki/issues/511), [#523](https://github.com/Ameyanagi/ReShiki/issues/523), [#525](https://github.com/Ameyanagi/ReShiki/issues/525), [#526](https://github.com/Ameyanagi/ReShiki/issues/526), [#527](https://github.com/Ameyanagi/ReShiki/issues/527), [#528](https://github.com/Ameyanagi/ReShiki/issues/528) | [Review](changes/qa-browser-polish.md)              |
| [#558](https://github.com/Ameyanagi/ReShiki/pull/558) | [#517](https://github.com/Ameyanagi/ReShiki/issues/517)                                                                                                                                                                                                                                                                                              | [Review](changes/qa-crystal-substitution.md)        |
| [#559](https://github.com/Ameyanagi/ReShiki/pull/559) | [#508](https://github.com/Ameyanagi/ReShiki/issues/508)                                                                                                                                                                                                                                                                                              | [Review](changes/qa-508-import-performance.md)      |
| [#560](https://github.com/Ameyanagi/ReShiki/pull/560) | [#502](https://github.com/Ameyanagi/ReShiki/issues/502), [#503](https://github.com/Ameyanagi/ReShiki/issues/503), [#505](https://github.com/Ameyanagi/ReShiki/issues/505)                                                                                                                                                                            | [Review](changes/qa-label-feedback.md)              |
| [#561](https://github.com/Ameyanagi/ReShiki/pull/561) | [#512](https://github.com/Ameyanagi/ReShiki/issues/512), [#518](https://github.com/Ameyanagi/ReShiki/issues/518), [#519](https://github.com/Ameyanagi/ReShiki/issues/519), [#524](https://github.com/Ameyanagi/ReShiki/issues/524)                                                                                                                   | [Review](changes/qa-model-controls.md)              |
| [#562](https://github.com/Ameyanagi/ReShiki/pull/562) | [#546](https://github.com/Ameyanagi/ReShiki/issues/546), [#547](https://github.com/Ameyanagi/ReShiki/issues/547), [#549](https://github.com/Ameyanagi/ReShiki/issues/549)                                                                                                                                                                            | [Review](changes/qa-help-polish.md)                 |
| [#563](https://github.com/Ameyanagi/ReShiki/pull/563) | [#491](https://github.com/Ameyanagi/ReShiki/issues/491)                                                                                                                                                                                                                                                                                              | [Review](changes/recovery-manager.md)               |
| [#564](https://github.com/Ameyanagi/ReShiki/pull/564) | [#548](https://github.com/Ameyanagi/ReShiki/issues/548), [#550](https://github.com/Ameyanagi/ReShiki/issues/550), [#551](https://github.com/Ameyanagi/ReShiki/issues/551), [#554](https://github.com/Ameyanagi/ReShiki/issues/554)                                                                                                                   | [Review](changes/qa-property-continuity.md)         |
| [#565](https://github.com/Ameyanagi/ReShiki/pull/565) | [#499](https://github.com/Ameyanagi/ReShiki/issues/499)                                                                                                                                                                                                                                                                                              | [Review](changes/qa-export-feedback.md)             |
| [#566](https://github.com/Ameyanagi/ReShiki/pull/566) | [#533](https://github.com/Ameyanagi/ReShiki/issues/533), [#534](https://github.com/Ameyanagi/ReShiki/issues/534)                                                                                                                                                                                                                                     | [Review](changes/qa-inspector-motion.md)            |
| [#567](https://github.com/Ameyanagi/ReShiki/pull/567) | [#552](https://github.com/Ameyanagi/ReShiki/issues/552), [#553](https://github.com/Ameyanagi/ReShiki/issues/553), [#555](https://github.com/Ameyanagi/ReShiki/issues/555), [#556](https://github.com/Ameyanagi/ReShiki/issues/556)                                                                                                                   | [Review](changes/qa-drawing-polish.md)              |
| [#568](https://github.com/Ameyanagi/ReShiki/pull/568) | [#530](https://github.com/Ameyanagi/ReShiki/issues/530), [#531](https://github.com/Ameyanagi/ReShiki/issues/531), [#532](https://github.com/Ameyanagi/ReShiki/issues/532)                                                                                                                                                                            | [Review](changes/qa-stereo-repeat.md)               |
| [#577](https://github.com/Ameyanagi/ReShiki/pull/577) | [#575](https://github.com/Ameyanagi/ReShiki/issues/575), [#576](https://github.com/Ameyanagi/ReShiki/issues/576)                                                                                                                                                                                                                                     | [Review](changes/qa-properties-clarity.md)          |
| [#578](https://github.com/Ameyanagi/ReShiki/pull/578) | [#572](https://github.com/Ameyanagi/ReShiki/issues/572), [#573](https://github.com/Ameyanagi/ReShiki/issues/573)                                                                                                                                                                                                                                     | [Review](changes/qa-pointer-hover.md)               |
| [#579](https://github.com/Ameyanagi/ReShiki/pull/579) | [#569](https://github.com/Ameyanagi/ReShiki/issues/569), [#570](https://github.com/Ameyanagi/ReShiki/issues/570), [#571](https://github.com/Ameyanagi/ReShiki/issues/571)                                                                                                                                                                            | [Review](changes/qa-model-face-handles.md)          |
| [#581](https://github.com/Ameyanagi/ReShiki/pull/581) | [#580](https://github.com/Ameyanagi/ReShiki/issues/580)                                                                                                                                                                                                                                                                                              | [Review](changes/qa-remember-appearance.md)         |
| [#582](https://github.com/Ameyanagi/ReShiki/pull/582) | [#574](https://github.com/Ameyanagi/ReShiki/issues/574)                                                                                                                                                                                                                                                                                              | [Review](changes/large-selection-copy-feedback.md)  |

## Local preparation checks

These checks ran on macOS ARM64 against the complete source base above plus
this release metadata. They do not establish checks on signed or published
packages. The final preparation head, CI and package run remain separate gates.

| Check                            | Observed result                                                                                                           |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| Theme generation                 | Regenerated all five themes; parsed data matches the source base after removing only `reshiki_version` (0.13.0 → 0.13.1). |
| Theme and version-resource tests | 1 theme-reference test and 4 host-independent Windows resource-version tests passed.                                      |
| Release Python modules           | `tests.test_release`, `tests.test_prepare_nightly` and `tests.test_release_downloads`: 28 tests passed.                   |
| Documentation diagnostics        | 0 errors, 0 warnings and 0 hints.                                                                                         |
| Documentation build              | 13,829 local links/assets, llms.txt, agent-api.txt and 1,360 published colours verified.                                  |
| Coverage and inherited evidence  | All 64 findings, 28 PR groups, 23 central-note images and original review links retained.                                 |

## Checks on the release preparation

Record command, exact source head, environment and result after each check.
No result is carried over merely because an individual fix passed it.

- [ ] Root Cargo and lockfile versions are 0.13.1; member versions and the npm
      placeholder are unchanged. `scripts/build_release.py --tag v0.13.1 --check-tag-only` accepts the package version.
- [ ] The final source base contains all 28 QA pull requests above. The release
      notes retain all 64 findings, credits, source links and material limits.
- [ ] `cargo fmt --all --check` and normal commit hooks pass.
- [ ] Native all-feature Clippy and the required three-package WASM Clippy pass.
- [ ] The theme reference matches the application; the published colour data
      has version 0.13.1 and no unintended colour changes.
- [ ] JavaScript formatting and lint pass.
- [ ] `bun run docs:check` and `bun run docs:build` pass, including local links,
      the documentation tree, llms.txt and palette downloads.
- [ ] Before/after release-documentation evidence is inspected. The version,
      current links and format 30 notice are readable in the built website.
- [ ] The exact release preparation PR head passes its required CI and has no
      unresolved review comments.

## Package review before merge and tag

A manual **Release builds** run on the release preparation feature branch
builds unsigned test packages without publishing. Record its run URL, exact
head and results. Review those packages before merging or tagging. The stable
tag workflow separately requires all live-reference checks and both macOS
signing/notarization jobs; do not describe unsigned test artifacts as signed
release downloads.

- [ ] All six manual package jobs pass: macOS ARM64/x64, Windows ARM64/x64 and
      Linux ARM64/x64. Record any hardware or runtime checks that could not run.
- [ ] Extracted packages report 0.13.1 and the expected source commit. Their
      `build.json` version, OS, architecture and target match the package name.
- [ ] Portable applications and installers pass the workflow's relocation,
      offline chemistry, worker, CLI, experimental agent API and installation
      checks. Graphical smoke results are recorded separately from those tests.
- [ ] On available hardware, open a drawing, edit and repeat an atom label,
      import artemisinin, substitute a crystal atom, rotate/resize a 3D model,
      check whole/selected Properties, copy/export and save/reopen. Preserve
      any platform coverage limits instead of checking unavailable hardware.
- [ ] Format compatibility: an older drawing opens in 0.13.1, a 0.13.1 save
      records format 30, and v0.13.0 refuses that save with update guidance.
- [ ] The selected preparation merge commit is on main and its final CI is
      green. No release tag is created before this gate.

## Tag and publication

Fill this section only after the corresponding actions occur:

- Tag and exact source commit: pending.
- Stable release workflow URL and conclusion: pending.
- Complete live-reference checks on macOS ARM64, Linux x64 and Windows x64:
  pending.
- Six builds and both macOS signing/notarization jobs: pending.
- Public release URL and Latest designation: pending.

Under the current configuration, macOS publication requires signed, notarized,
stapled and verified ZIPs and DMGs. Windows production signing is disabled;
Windows and Linux downloads remain unsigned. A signing failure must not be
replaced by an unsigned macOS package. Do not activate Windows signing as part
of this release preparation.

## Public-download and web verification

- [ ] Download all ten packages and `SHA256SUMS` outside the checkout. Compare
      each asset's digest and every checksum entry; record the receipts.
- [ ] Verify all six portable manifests, version/source/architecture, included
      notices and the single application executable. Record runtime coverage
      separately from inspection of archive contents.
- [ ] Verify macOS app and DMG signatures, notarization tickets and Gatekeeper
      assessment for both architectures. Record Rosetta use explicitly where
      an Intel app is checked on Apple Silicon.
- [ ] The browser demo asset records 0.13.1, retains the previous hashed files
      needed by cached pages, and the Documentation deployment succeeds.
- [ ] `@ameyanagi/reshiki@0.13.1` exists on npm and is the expected `latest`
      version. A green npm job alone is insufficient: refused credentials can
      leave a notice without publishing.
- [ ] The installed release's update/download links and the current website
      point to the intended stable release.

Append results and remaining limits here without rewriting the historical
[0.13.0 record](release-0.13.0-validation.md) or individual fix receipts.
