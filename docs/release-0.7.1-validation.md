# ReShiki 0.7.1 release validation

[PR #19](https://github.com/Ameyanagi/ReShiki/pull/19) merged as `de0e043cb0a358dab63f560d6fbfd3585435637f` after its reviewed head `41b48e72d621b34718fe9129efbcba1f1d2d1a71` passed every applicable check. The merged tree matches that tested head. The release also includes the Windows popup-shadow fix from [PR #18](https://github.com/Ameyanagi/ReShiki/pull/18).

## Review and regression checks

Review covered image alpha handling, bounded per-ligand edits, aromatic bond styles and gesture previews, native serialization, charge-label rendering, and typed attachment exchange. No blocking code findings remained. No new production unsafe blocks, panicking unwrap/expect calls, or explicit panic paths were introduced. Computer Use caught and corrected an inaccurate documentation path to charge-label visibility.

The local combined run passed **401 Rust tests, with 3 ignored**. Cargo check, Clippy with warnings denied, and formatting passed. The [final PR checks](https://github.com/Ameyanagi/ReShiki/actions/runs/35894780573) passed native Rust tests, captured chemistry fixtures, Python tests, and lint on macOS, Windows, and Linux, plus web checks. The [documentation run](https://github.com/Ameyanagi/ReShiki/actions/runs/35894780608) passed separately.

The website passed diagnostics, production build, formatting, and local link/asset validation. Safari inspection covered the new Codex setup guide and illustrated change log. Seven new native screenshots show the actual macOS app; the [capture record](images/assistant-updates/README.md) identifies their source build and operations. Existing user drafts were preserved.

## Published release and package verification

[ReShiki 0.7.1](https://github.com/Ameyanagi/ReShiki/releases/tag/v0.7.1) was published as a stable release on September 23, 2026 (UTC), from commit `b25f0a4dd0fcce6aa99fe59a7f07a2552faa7470`. All **28 jobs** in the [tagged release workflow](https://github.com/Ameyanagi/ReShiki/actions/runs/35897429706) passed, including five platform builds, twelve live chemistry-reference shards across macOS/Windows/Linux, Windows installer checks, and macOS Developer ID signing, notarization, stapling, and Gatekeeper verification. The [release commit's main checks](https://github.com/Ameyanagi/ReShiki/actions/runs/35896645076) and [documentation deployment](https://github.com/Ameyanagi/ReShiki/actions/runs/35896645262) also passed.

After publication, all eight public packages were downloaded and verified against the published `SHA256SUMS`. Each of the five portable archives contains the expected version, platform, architecture, and release commit. The macOS metadata also confirms signing and notarization.

The public macOS ZIP and DMG are byte-identical to the signed artifacts tested locally. Those checks verified the Developer ID signature, stapled notarization ticket, Gatekeeper acceptance, DMG mounting and drag-to-install copying, and native chemistry with Python, uv, and the checkout unavailable. Computer Use inspection of the signed app confirmed version 0.7.1 and both aromatic Cp* ellipses in the wedge regression drawing. Existing user drafts were left open. Windows and Linux verification ran in CI; this record does not claim a local GUI inspection on those platforms.

The [illustrated release notes](changes-0.7.1.md) retain the coordination-chemistry and editable-exchange limits; packaging does not establish chemical validity.
