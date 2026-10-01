# ReShiki 0.10.0 release validation

[ReShiki 0.10.0](https://github.com/Ameyanagi/ReShiki/releases/tag/v0.10.0) was published as the latest stable release at 22:17 UTC on October 1, 2026 (October 2 in Japan). This record distinguishes source review, package checks and verification of the actual public downloads. [Release notes](changes-0.10.md).

## Source review and package preparation

[PR #104](https://github.com/Ameyanagi/ReShiki/pull/104) prepared the release, consolidated runtime work into one Rust application executable per platform, and pinned Rust 1.99.0 for local and CI builds. InChI, macOS clipboard and macOS printing run in isolated modes of the same executable. Each portable package retains four project license files and one complete `THIRD-PARTY-NOTICES.txt`; no Python runtime or companion worker executable is included.

The release includes document tabs and smart alignment guides from [PR #101](https://github.com/Ameyanagi/ReShiki/pull/101), and preserves @HiroYokoyama's credit for drag-to-copy and axis-locked moves in [PR #87](https://github.com/Ameyanagi/ReShiki/pull/87). Drawings saved with document version 17 cannot be opened in 0.9.1 or earlier. Linux users upgrading from 0.9.1 need to extract the new archive manually because the older updater requires the retired helper executable.

The [six-platform preflight](https://github.com/Ameyanagi/ReShiki/actions/runs/36919926706) passed builds, archive checks and installer checks using Rust 1.99.0. It exercised nightly version stamping without publishing a nightly release. Its Apple Silicon ZIP was also extracted locally outside the checkout and passed native chemistry and the application clipboard/print worker checks with an empty executable search path. This preflight preceded the reference and worker-stack fixes below; the stable workflow checks the final tagged source again.

An earlier optimized macOS preview passed an actual editable clipboard copy/paste round trip for ethanol, retaining three atoms, two bonds and formula C2H6O. Save to PDF produced a document that opened in Preview. Those graphical checks used the pre-1.99 preview; they are separate from the final compiler and package checks. Existing smart-guide and toolbar images were captured from the application renderer. Desktop interaction coverage remains macOS-only, and this validation makes no hardware GPU performance claim.

## Reference-suite fixes

The initial [complete reference run](https://github.com/Ameyanagi/ReShiki/actions/runs/36919925645) exposed failures beyond the normal PR checks. [PR #105](https://github.com/Ameyanagi/ReShiki/pull/105) addressed them before the stable tag:

- A 476-atom MOL fixture overflowed the default Windows worker stack in debug tests. Both executable entry points now run InChI on an operation thread with an explicit 32 MiB stack, retaining the process-wide heap budget and parent-enforced timeout. The production application is compiled with `cargo build --release --locked`.
- Rust 1.99's debug escaping differed from the pinned Python 3.12 error spelling. The corrected classification matched Python's Unicode 15 printability for all 1,112,064 valid Unicode scalar values.
- Historical reference requests predated explicit hexadecimal color tags. The optional reference bridge now translates document, atom-indicator and graphic-part colors, restoring unchanged values by stable object IDs or bond endpoints. Tests cover removal, reordering and changed RGB values. Palette-tag requests are rejected rather than silently resolved.
- CDX import comparisons now use the existing independent Python codec corrections for captured ChemDraw units and object codes. The historical Python worker and chemistry algorithms remain unchanged. Empty-input InChI help comparisons also account for Windows' slash option prefix.

The [final PR checks](https://github.com/Ameyanagi/ReShiki/actions/runs/36927320529) passed on macOS ARM64, Windows x64 and Linux x64, including native tests, all-target checks, Clippy, Python checks and web validation. All nine targeted reference-bridge tests and four worker protocol/heap-limit tests also passed locally. Both review findings were fixed and resolved before merge.

## Stable publication and public packages

The annotated `v0.10.0` tag points to `c8ffe8c01ca9d561f06805ddaf253f436f5ecbbe`. The [stable release workflow](https://github.com/Ameyanagi/ReShiki/actions/runs/36929874486) passed all six platform builds, twelve live-reference shards, native/Python checks and both macOS signing jobs before publishing. The first tagged run passed without retries, changed assertions or skipped release gates. GitHub identifies the release as stable, non-draft and latest.

All ten public packages and `SHA256SUMS` were downloaded after publication. Every package checksum matched the published checksum file, and all eleven downloads also matched GitHub's asset digests. All six portable manifests identify version `0.10.0`, the tagged commit, the expected architecture and Rust target, and InChI `1.07.5` with the `self-process` runtime. Every portable archive contains exactly one application executable and the five expected license files.

| Platform  | Architectures                             | Downloads                               |
| --------- | ----------------------------------------- | --------------------------------------- |
| macOS 14+ | Apple Silicon (`arm64`) and Intel (`x64`) | DMG and ZIP for each architecture       |
| Windows   | x64 and ARM64                             | Setup EXE and ZIP for each architecture |
| Linux     | x64 and ARM64                             | tar.gz for each architecture            |

Both public Mac ZIPs and DMGs were verified locally outside the checkout. App and disk-image signatures, notarization tickets and Gatekeeper assessments passed. The extracted archives and drag-to-install copies ran native chemistry twice with Python, uv and the checkout unavailable, and passed the packaged clipboard/print worker entry-point checks. Intel package execution on this Apple Silicon machine used Rosetta; Intel compilation and native tests ran on the Intel CI runner. Windows and Linux runtime and installer checks ran in CI; their public packages remain unsigned.

The release page retains the complete download table and generated contributor notes, with the 0.10.0 highlights, document compatibility warning and Linux upgrade instructions above them.

## Documentation validation

Local `bun run docs:check` passed without errors, warnings or hints. `bun run docs:build` produced 83 pages and verified 6,885 local documentation links and assets plus 1,088 colors in the JSON, CSV and CSS downloads. The documentation workflow checks and builds this record before deployment from `main`.
