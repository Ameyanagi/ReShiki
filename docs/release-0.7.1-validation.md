# ReShiki 0.7.1 release validation

[PR #19](https://github.com/Ameyanagi/ReShiki/pull/19) merged as `de0e043cb0a358dab63f560d6fbfd3585435637f` after its reviewed head `41b48e72d621b34718fe9129efbcba1f1d2d1a71` passed every applicable check. The merged tree matches that tested head. The release also includes the Windows popup-shadow fix from [PR #18](https://github.com/Ameyanagi/ReShiki/pull/18).

## Review and regression checks

Review covered image alpha handling, bounded per-ligand edits, aromatic bond styles and gesture previews, native serialization, charge-label rendering, and typed attachment exchange. No blocking code findings remained. No new production unsafe blocks, panicking unwrap/expect calls, or explicit panic paths were introduced. Computer Use caught and corrected an inaccurate documentation path to charge-label visibility.

The local combined run passed **401 Rust tests, with 3 ignored**. Cargo check, Clippy with warnings denied, and formatting passed. The [final PR checks](https://github.com/Ameyanagi/ReShiki/actions/runs/35894780573) passed native Rust tests, captured chemistry fixtures, Python tests, and lint on macOS, Windows, and Linux, plus web checks. The [documentation run](https://github.com/Ameyanagi/ReShiki/actions/runs/35894780608) passed separately.

The website passed diagnostics, production build, formatting, and local link/asset validation. Safari inspection covered the new Codex setup guide and illustrated change log. Seven new native screenshots show the actual macOS app; the [capture record](images/assistant-updates/README.md) identifies their source build and operations. Existing user drafts were preserved.

## Publication gates

Version 0.7.1 is being prepared; this record does not yet claim that public downloads are available. Tagged publication requires all five platform packages, Windows installer checks, twelve live chemistry-reference shards across macOS/Windows/Linux, and macOS Developer ID signing, notarization, stapling, and Gatekeeper verification. The release workflow verifies all eight package checksums before publishing.

After publication, the public downloads will be checked against `SHA256SUMS` and each portable archive's version, architecture, and source commit. Public macOS packages will also be checked locally. The [illustrated release notes](changes-0.7.1.md) retain the coordination-chemistry and editable-exchange limits; packaging does not establish chemical validity.
