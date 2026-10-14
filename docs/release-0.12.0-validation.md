# ReShiki 0.12.0 release validation

[ReShiki 0.12.0](https://github.com/Ameyanagi/ReShiki/releases/tag/v0.12.0) was published as the latest stable release at 15:12:49 UTC on October 14, 2026. GitHub identifies release `412860958` as stable, non-draft and latest. This record separates the source review, the tag runs that did not publish, the publishing run and the checks of the actual public downloads. [Release notes](changes-0.12.md).

## Source review and package preparation

[PR #319](https://github.com/Ameyanagi/ReShiki/pull/319) landed the last feature and wording changes. [PR #320](https://github.com/Ameyanagi/ReShiki/pull/320) prepared the release and merged after 22 checks passed and three expected checks were skipped. Its reviewed head was `cdf755a5c8f8f796e63a0b4c7ea26f4c93121c57`. Greptile rated the change 5/5. Existing authors and change credits were preserved.

Before the release, a manual [release build](https://github.com/Ameyanagi/ReShiki/actions/runs/38505899071) of the preparation branch passed on all six platforms. It did not publish.

A local optimized macOS ARM64 build was pasted into real applications. PowerPoint, Word and Excel pasted a vector picture. ChemDraw pasted an editable structure after asking whether to keep the copied drawing settings. A Safari image area pasted a picture, and a text area pasted nothing until **Cmd+Alt+C** supplied SMILES. A `CCO` cell copied from Excel pasted into ReShiki as ethanol. Keynote is not installed on the verification host and was not tested, although the release notes name it.

Drawings saved in 0.12.0 use document version 26 and cannot be opened in ReShiki 0.11.0 or earlier. The public applications report document version 26. Older drawings still open in 0.12.0.

macOS publication requires signed, notarized and stapled packages with signature and Gatekeeper verification. Windows and Linux packages remain unsigned; Windows signing is pending.

## Tag runs that did not publish

The annotated `v0.12.0` tag was first pushed on the PR #320 merge. It was moved three times, each time to a `main` commit whose fix had passed its checks. No earlier run published a release.

1. [Run 38525098213](https://github.com/Ameyanagi/ReShiki/actions/runs/38525098213) at `884da257` stopped while planning the live-reference shards: 38 OPSIN Rust-port test targets belonged to no shard. [PR #321](https://github.com/Ameyanagi/ReShiki/pull/321) assigned them and merged as `537b321a` after 19 checks passed.
2. [Run 38531861711](https://github.com/Ameyanagi/ReShiki/actions/runs/38531861711) at `537b321a` failed five live-reference shards. The live suite had not been updated for behavior that changed after 0.11.0; triage found no regressions. [PR #322](https://github.com/Ameyanagi/ReShiki/pull/322) classified the changes against the original reference worker and built the naming helper for shard 0. Its pull-request checks were cancelled in favor of a [manual Checks run](https://github.com/Ameyanagi/ReShiki/actions/runs/38555299550) on the same head, which ran all twelve live-reference shards: 29 jobs passed and one optional capture job was skipped. It merged as `94c8d18d`.
3. [Run 38560912614](https://github.com/Ameyanagi/ReShiki/actions/runs/38560912614) at `94c8d18d` passed validation, all six builds, all twelve live-reference shards and Apple Silicon signing. Intel macOS signing failed in all five attempts after Apple had accepted notarization. `stapler` lost its connection to Apple's ticket service (exit 68), and once `hdiutil create` reported "Resource busy". [PR #323](https://github.com/Ameyanagi/ReShiki/pull/323) retries stapling up to four times, 20 seconds apart, and disk-image creation up to three times, 10 seconds apart. It merged as `4bfadead` after 19 checks passed.

## Publishing run

The `v0.12.0` tag points to `4bfadead3ee131ab74a355c854ed2c0c3c5f4084`. The first attempt of the [stable release workflow](https://github.com/Ameyanagi/ReShiki/actions/runs/38576354535) at that commit passed from 13:58 to 15:13 UTC. Validation, all six platform builds, twelve live-reference shards, both macOS signing jobs and publication passed: 39 jobs passed, and the optional capture job and Windows signing were skipped.

The PR #323 retries were needed. Stapling the Intel disk image succeeded on the fourth and last attempt. Validating its ticket took two attempts, and validating the app copied from it took three. Every failed attempt lost its connection to Apple. Apple Silicon signing needed no retry.

## Public packages

All ten public packages and `SHA256SUMS` were downloaded after publication and checked outside the checkout. All eleven downloads matched GitHub's asset digests, and every package matched the published checksum file. All six portable manifests name version `0.12.0`, the tagged commit and the expected OS, architecture and Rust target, with the InChI work in the application's own process and statically linked geometry. Each portable archive contains exactly one application executable, five nonempty license files and no Python, uv or RDKit files.

| Platform  | Architectures                             | Packages                                |
| --------- | ----------------------------------------- | --------------------------------------- |
| macOS 14+ | Apple Silicon (`arm64`) and Intel (`x64`) | DMG and ZIP for each architecture       |
| Windows   | x64 and ARM64                             | Setup EXE and ZIP for each architecture |
| Linux     | x64 and ARM64                             | tar.gz for each architecture            |

Both public macOS ZIPs and DMGs were verified. Developer ID signing used team `XXN44W8X56`, with Hardened Runtime and secure timestamps. Both disk images and the applications from both ZIPs and both disk images passed strict deep signature checks, ticket validation and Gatekeeper assessment as notarized Developer ID software.

The applications were extracted, or copied out of each mounted disk image, into folders whose paths contain spaces. The release package checks from the tagged commit then ran against each copy:

- Chemistry ran twice with Python, uv and the checkout unavailable. `reshiki --cli analyze --smiles CCO` with an empty search path returned C2H6O and `InChI=1S/C2H6O/c1-2-3/h3H,2H2,1H3`.
- The experimental agent server and `reshiki --cli` passed, including a quarantined copy of the app. The server listened on no network socket.
- Rust MMFF94 and UFF geometry ran from the single relocated executable.
- The clipboard and print worker entry points returned the expected errors.
- Signatures still verified afterward.

Intel applications ran under Rosetta on the Apple Silicon verification host; Intel compilation and native tests ran in CI. These checks cover signing and the packaged workers; full graphical desktop acceptance remains separate.

Windows portable and setup checks and Linux portable-runtime checks passed in the publishing run. The public Windows and Linux downloads were checked for digests, manifests and contents only, not run natively. They remain unsigned.

## Documentation validation

The [Documentation workflow](https://github.com/Ameyanagi/ReShiki/actions/runs/38525081539) built and deployed the release preparation documentation at `884da257`. This record follows the same review and deployment path.

Publication and package checks do not cover Keynote, the Microsoft 365 add-in in real Office hosts, Windows signing, antivirus results, accessibility or full graphical desktop acceptance. Those limits remain in the release notes.
