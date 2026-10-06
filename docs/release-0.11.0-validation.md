# ReShiki 0.11.0 release validation

[ReShiki 0.11.0](https://github.com/Ameyanagi/ReShiki/releases/tag/v0.11.0) was published as the latest stable release at 02:13:53 UTC on October 6, 2026. GitHub identifies release `404240067` as stable, non-draft and latest. This record distinguishes source review, package checks and verification of the actual public downloads. [Release notes](changes-0.11.md).

## Source review and package preparation

[PR #185](https://github.com/Ameyanagi/ReShiki/pull/185) prepared the release and merged after 22 checks passed and three expected checks were skipped. Its reviewed head was `3af109fb249ae591f86a1ea608c1ceedb01a9161`. Greptile rated the change 5/5 and reported no actionable defects. Existing authors and change credits were preserved.

The application runtime matches the October 5 nightly source, `eed953f7b381a31476cdbc42534fa6db84007921`. A tagged-source comparison across `src`, `crates`, `engine`, `integrations`, `examples`, `presets`, `assets` and `build.rs` found only `src/engine/reference/tests/protocol.rs` changed: a test fixture now uses Python UTF-8 mode on Windows. Ongoing refactoring was excluded. Preparation updates version metadata and documentation, and includes two narrow CI/test fixes from [PR #158](https://github.com/Ameyanagi/ReShiki/pull/158): assigning the C60/embedding integration tests to a live-reference shard, and matching the fixture worker's encoding to the production reference worker.

A local optimized macOS ARM64 package completed successfully with the native Rust runtime and no Python or RDKit runtime. This local preparation preceded the tagged six-platform checks and is separate from verification of the public downloads.

Drawings saved in 0.11.0 use document version 19 and cannot be opened in ReShiki 0.10.0 or earlier. Older drawings still open in 0.11.0; users who need an earlier release should keep a separate copy before saving. C60 starting-geometry recovery is included, but floating double-bond strokes in its projection remain unresolved.

macOS publication requires signed, notarized and stapled packages with signature and Gatekeeper verification. Windows and Linux packages remain unsigned; SignPath Foundation approval and Windows signing setup are pending. Earlier development-file antivirus results do not qualify new release packages.

## Stable workflow and rerun

The annotated `v0.11.0` tag points to `3b83766df57ae20f2adf855d14d12d91c287c434`. Attempt 2 of the [stable release workflow](https://github.com/Ameyanagi/ReShiki/actions/runs/37385717894) completed successfully at that unchanged commit. Validation, all six platform builds, twelve live-reference shards, both macOS signing jobs and publication passed, with no failed final steps.

The first attempt reported two PowerShell timeouts in the restart-argument fixture, `tests/test_update_handoff.py`, on Windows ARM64. The zero- and one-drawing cases exceeded the unchanged 30-second limit; no argument mismatch was reported. Windows x64 passed. The second attempt passed with the same source, fixture, assertions and release gates.

## Public packages

All ten public packages and `SHA256SUMS` were downloaded after publication and independently audited. All eleven downloads matched GitHub's asset digests, and every package matched the published checksum file. All six portable manifests identify version `0.11.0`, the tagged commit and the expected OS, architecture and Rust target. Each portable archive contains exactly one native application executable and five nonempty license files. Native chemistry uses the application's self-process runtime and statically linked Rust geometry; packages include no Python runtime and require no runtime chemistry download.

Both public macOS ZIPs and DMGs were verified outside the checkout. Developer ID signing used team `XXN44W8X56`, with Hardened Runtime and timestamps checked. App and disk-image signature, stapling and Gatekeeper checks passed. Relocated ZIP applications ran native chemistry twice with Python, uv and the checkout unavailable, passed Rust MMFF/UFF geometry without Python or RDKit, and passed the packaged clipboard/print worker entry points. Temporary drag-installed DMG copies also passed two native chemistry launches, clipboard/print worker checks and app signature, stapling and Gatekeeper verification.

Intel package execution used Rosetta on the Apple Silicon verification host; Intel compilation and native tests ran in CI. These checks cover package signing and native worker execution; full graphical desktop acceptance remains separate.

Windows portable/setup checks and Linux portable-runtime checks passed in the final CI run. Their public-asset digest and checksum checks are separate from native desktop or antivirus acceptance; the packages remain unsigned.

| Platform  | Architectures                             | Packages                                |
| --------- | ----------------------------------------- | --------------------------------------- |
| macOS 14+ | Apple Silicon (`arm64`) and Intel (`x64`) | DMG and ZIP for each architecture       |
| Windows   | x64 and ARM64                             | Setup EXE and ZIP for each architecture |
| Linux     | x64 and ARM64                             | tar.gz for each architecture            |

## Documentation validation

Local `bun run docs:check` passed with zero errors, warnings or hints. With this validation record included, `bun run docs:build` produced 91 pages and verified 7,815 local documentation links and assets plus 1,088 colors. The [main documentation workflow](https://github.com/Ameyanagi/ReShiki/actions/runs/37385685879) built and deployed the release preparation documentation. This record follows the same documentation review and deployment path. Direct public documentation probes were blocked by Cloudflare error 1010; public visual acceptance was not performed.

Publication and package checks do not close the documented C60 display issue or the remaining desktop, accessibility, performance, Office/LibreOffice and antivirus acceptance work. Those limits remain in the release notes.
