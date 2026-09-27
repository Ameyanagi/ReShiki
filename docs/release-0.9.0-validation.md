# ReShiki 0.9.0 release validation

[ReShiki 0.9.0](https://github.com/Ameyanagi/ReShiki/releases/tag/v0.9.0) was published as the latest stable release on September 27, 2026. This record distinguishes source review, package checks and public-download verification against [0.8.0](https://github.com/Ameyanagi/ReShiki/releases/tag/v0.8.0). [Release notes](changes-0.9.md).

## Source review and desktop checks

The release includes publisher styles and theme management from [PR #41](https://github.com/Ameyanagi/ReShiki/pull/41), default 100% zoom from [PR #39](https://github.com/Ameyanagi/ReShiki/pull/39), aromatic placement from [PR #40](https://github.com/Ameyanagi/ReShiki/pull/40), and acknowledgment/OH–HO documentation from [PR #51](https://github.com/Ameyanagi/ReShiki/pull/51). The original aromatic-fusion contributor commits are preserved.

PR #40 merged only after its [Checks workflow](https://github.com/Ameyanagi/ReShiki/actions/runs/36284064735) passed on macOS ARM64, Windows x64 and Linux x64, its documentation and license checks passed, and the latest review reported no actionable findings. The reviewed head was `1a7422d465f7a40ea8b73439cd9606eaacf99c7c`; the merge is `34861a8a5d0eadfb50f1a6e9c9f07aca11a64cd8`.

The aromatic desktop sweep used optimized macOS builds and 60 targeted cases. Five failures in the separate Aromatic circle path were corrected and replayed. Saved graphs were checked for native chemical identity, duplicate vertices and atomic rejection. Coverage includes benzene atoms/edges, inward notches, phenanthrene-to-pyrene and existing-atom gap closure, protected atoms, templates, repeated shortcuts and Undo/Redo. A later mixed circle/Kekulé review finding was reproduced before the fix and retested in the release app. Its regression covers 30 successive placement sequences. The latest targeted native run passed 487 tests with four ignored; formatting, all-target/all-feature Clippy and compile checks passed.

Publisher settings were compared with downloaded stationery and ChemDraw output during PR #41. The documentation separates journal typography/dimensions, canvas theme colors, transparent clipboard copies and file-export backgrounds. Existing OH/HO screenshots document the controls on 0.8.0 and remain labeled with their original capture conditions. Temporary internal-review galleries remain outside Git.

## Published release and package verification

The annotated `v0.9.0` tag points to `1fd2b6c4c2936007e6d36077b3b620e76f2047d5`. The [tagged release workflow](https://github.com/Ameyanagi/ReShiki/actions/runs/36292105416) passed all six platform builds, both Mac signing jobs, all twelve live-reference shards and the native/Python/documentation checks. GitHub published the release at 04:20 UTC on September 27 as a stable, non-draft release.

All ten public packages and `SHA256SUMS` were downloaded after publication. Every SHA-256 matched. All six portable manifests identify version `0.9.0`, the tagged source commit, the expected architecture and Rust target, and InChI `1.07.3`.

| Platform  | Architectures                                              | Downloads                               |
| --------- | ---------------------------------------------------------- | --------------------------------------- |
| macOS 14+ | Apple Silicon (`arm64`) and Intel (`x64`), separate builds | DMG and ZIP for each architecture       |
| Windows   | x64 and ARM64                                              | Setup EXE and ZIP for each architecture |
| Linux     | x64 and ARM64                                              | tar.gz for each architecture            |

Both signed Mac ZIPs and DMGs were checked locally outside the checkout. The checks verified the expected Developer ID team, Hardened Runtime, timestamped signatures, app/disk-image notarization tickets and Gatekeeper acceptance. ZIP extraction and DMG drag-to-install copies launched native chemistry twice with Python, uv and the checkout unavailable. App, InChI, clipboard and print-helper executable architectures matched their manifests. The four public Mac downloads have exactly the same SHA-256 values as these locally verified signed artifacts. Windows and Linux packages remain unsigned.

The Intel preflight app was also opened unchanged through Computer Use under Rosetta on Apple Silicon. The mixed aromatic regression drawing analyzed as C13H10 without chemistry errors. A new single bond followed by the Boc shortcut produced C6H12O2, eight atoms and seven bonds, with the expected canonical SMILES. This local graphical check was under Rosetta; the Intel compilation, installed-package checks and 226 native library tests ran on the Intel CI runner. Broader desktop interaction checks remain macOS-only.

## Release fixes and Intel Mac coverage

The first tagged attempt, [run 36286650032](https://github.com/Ameyanagi/ReShiki/actions/runs/36286650032), was blocked by stale reference expectations for accepted CDXML annotations and exact bond-spacing percentages. [PR #54](https://github.com/Ameyanagi/ReShiki/pull/54) corrected those narrow comparisons while retaining the remaining full-document comparisons. It also moved package staging outside Cargo's cache to prevent cleanup from traversing bundled license sources, and added separate Intel Mac packages.

The next attempt, [run 36289414535](https://github.com/Ameyanagi/ReShiki/actions/runs/36289414535), stopped because the optional pinned RDKit release has no Intel macOS wheel. [PR #55](https://github.com/Ameyanagi/ReShiki/pull/55) isolated native packaging from reference-only Python dependencies and made Intel helper protocol, allocator and source-patch tests mandatory. Its first preflight caught missing predefined-group support on Intel; the fix shares the fixed Mac template coordinates and selects the correct Intel DMG for updates. The coordinates retain their Apple Silicon provenance, with no claim of an independent Intel RDKit capture.

The successful [six-platform preflight](https://github.com/Ameyanagi/ReShiki/actions/runs/36291315673) tested the Intel app on an Intel runner, including every predefined group in isolated and attached configurations. Full live RDKit comparisons on macOS ARM64, Windows x64 and Linux x64 remained publication requirements.

## Intermittent CI follow-up

The final tagged workflow initially failed one macOS template-library test during an immediate lock handoff. The unchanged four-test suite passed 200 consecutive local runs. The [native job retry](https://github.com/Ameyanagi/ReShiki/actions/runs/36292105416/job/108548123737) then passed the complete native suite and saved chemistry/aromatic fixtures without source changes, altered assertions or skipped release gates. Publication remained blocked until that retry passed. The cause is still under investigation in [issue #56](https://github.com/Ameyanagi/ReShiki/issues/56); this record does not claim the intermittent test has been fixed.
