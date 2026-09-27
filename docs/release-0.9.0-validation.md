# ReShiki 0.9.0 release validation

This record tracks 0.9.0 against [0.8.0](https://github.com/Ameyanagi/ReShiki/releases/tag/v0.8.0). Release preparation is in progress; a version bump or tag alone is not evidence that the packages have been published. [Release notes](changes-0.9.md).

## Source review and desktop checks

The release includes publisher styles and theme management from [PR #41](https://github.com/Ameyanagi/ReShiki/pull/41), default 100% zoom from [PR #39](https://github.com/Ameyanagi/ReShiki/pull/39), aromatic placement from [PR #40](https://github.com/Ameyanagi/ReShiki/pull/40), and acknowledgment/OH–HO documentation from [PR #51](https://github.com/Ameyanagi/ReShiki/pull/51). The original aromatic-fusion contributor commits are preserved.

PR #40 merged only after its [Checks workflow](https://github.com/Ameyanagi/ReShiki/actions/runs/36284064735) passed on macOS ARM64, Windows x64 and Linux x64, its documentation and license checks passed, and the latest review reported no actionable findings. The reviewed head was `1a7422d465f7a40ea8b73439cd9606eaacf99c7c`; the merge is `34861a8a5d0eadfb50f1a6e9c9f07aca11a64cd8`.

The aromatic desktop sweep used optimized macOS builds and 60 targeted cases. Five failures in the separate Aromatic circle path were corrected and replayed. Saved graphs were checked for native chemical identity, duplicate vertices and atomic rejection. Coverage includes benzene atoms/edges, inward notches, phenanthrene-to-pyrene and existing-atom gap closure, protected atoms, templates, repeated shortcuts and Undo/Redo. A later mixed circle/Kekulé review finding was reproduced before the fix and retested in the release app. Its regression covers 30 successive placement sequences. The latest targeted native run passed 487 tests with four ignored; formatting, all-target/all-feature Clippy and compile checks passed.

Publisher settings were compared with downloaded stationery and ChemDraw output during PR #41. The documentation separates journal typography/dimensions, canvas theme colors, transparent clipboard copies and file-export backgrounds. Existing OH/HO screenshots document the controls on 0.8.0 and remain labeled with their original capture conditions. Temporary internal-review galleries remain outside Git.

## Release gates

Before publication, verify the 0.9.0 version/tag match, documentation links and build, a relocated local package, all five platform builds, macOS Developer ID signing/notarization/stapling, and the full live-reference workflow on macOS, Windows and Linux. The tagged workflow publishes only after its build, signing and reference dependencies pass.

After publication, download all eight packages and `SHA256SUMS`; check every hash and the portable manifests' version, source commit, platform and architecture. Recheck the public Mac package's signature, Gatekeeper status, notarization ticket and native chemistry outside the checkout, and confirm the stable release and public documentation links.

Publication and package-verification results will be recorded here when those checks complete.
