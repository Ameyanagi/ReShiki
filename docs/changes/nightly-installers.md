# Nightly installers and release downloads

Nightly publishing reuses the stable package and signing workflow. A successful
nightly includes macOS DMG and ZIP packages for Apple Silicon and Intel, Windows
setup and ZIP packages for x64 and ARM64, and Linux tar.gz packages for x64 and
ARM64. Both macOS apps and disk images are signed, notarized, stapled and verified
before publication. Windows and Linux remain unsigned. A failed signing job
blocks publication, with no unsigned macOS fallback.

The shared release download table validates the exact ten filenames and all
checksum sidecars, verifies the signed Mac archive manifests, and writes
`SHA256SUMS`. Stable and Nightly both retain generated GitHub release notes.
Stable retains the Latest designation; Nightly remains a prerelease with
`latest=false`.

## Update window

![Nightly downloads prefer a published installer and remain manually installed.](../images/nightly-installers/nightly.png)

This is an unmodified 560 × 520 PNG from the application renderer on macOS arm64,
with a blank drawing, light appearance, automatic checks disabled, installed
version `0.9.1`, and the existing fixture version
`0.9.1-nightly.20260929.36501221724.1`. The fixture identifies a historical
archive-only release; the image demonstrates the current dialog text, not a new
published installer or an installation test. Render scale is 1×; canvas zoom does
not apply. Dark appearance was inspected separately.

Reproduce the light and dark captures with:

```sh
cargo test --release --locked --bin reshiki \
  app::updates::tests::update_channels_headless_snapshot -- --ignored --exact --nocapture
```

**Download nightly ↗** requests the selected release's asset names, chooses the
matching DMG or Windows setup when present, and falls back to the legacy portable
ZIP; Linux uses tar.gz. If the asset lookup is unavailable or rate-limited, the
button still opens the cached nightly's trusted portable URL. Removed releases
(HTTP 404/410), other permanent client errors and redirects do not open a browser. Malformed
or oversized responses remain errors. URLs are constructed locally for the expected GitHub
repository rather than accepting a server-provided download URL. Missing assets,
unrelated tags, drafts and nonnightly releases are rejected. Installation remains
manual. **Release notes** includes both installer and portable choices. Installers
replace the existing ReShiki installation; use a separate portable folder to keep
Stable.

The original [update-channel screenshots and desktop checks](update-channel.md)
retain the earlier archive-only UI. These checks do not establish successful
installation or notarization of a future nightly; those are publication gates in
the release workflow.

## Validation

Targeted Python tests cover exact asset names, table links, missing/extra assets,
checksums, signed Mac metadata, numeric installer versions and signing provenance.
The actual public `v0.9.1` Mac ZIP was inspected to confirm the manifest path and
fields used by the publisher. Both bundle version fields retain the numeric base
version; the executable, manifest, custom Mac package-version field and Windows
text-version fields retain the full nightly identity. The signing job stamps the
same Cargo version before checking archive provenance. See the
[release process](../releasing.md) for platform version constraints and signing
configuration.

Rust update tests cover installer preference and legacy fallback across all six
targets, unavailable/rate-limited API fallback without bypassing version or
metadata validation, real local HTTP responses for transient versus permanent
failures, locally constructed URLs, existing channel persistence, stale channel
checks and exclusion of nightlies from automatic stable installation. The
application update tests and renderer capture check the corresponding dialog.

The modified workflows pass actionlint 1.7.12. Normal Python/Rust formatting,
linting and type/compile checks run through the repository's commit hooks. Actual
Inno installation/upgrade/uninstall and Apple notarization are exercised by the
release workflow on their native platforms, not by the mocked local packaging
unit tests.

## Release-note material

Caption: Nightly builds offer installers and portable downloads, with signed Mac
packages and a download table for every supported platform.

Reusable image: `docs/images/nightly-installers/nightly.png`.
