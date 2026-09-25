# Release builds and macOS signing

The Release builds workflow produces a signed macOS disk image, Windows x64/ARM64 setup programs, and portable packages for all five platforms. Linux supports x64 and ARM64; macOS supports Apple Silicon only. Packages include the Rust application and native InChI helper. Drawing and chemistry work offline without Python, RDKit or uv.

Windows setup uses Inno Setup 6.7.3, downloaded with a pinned SHA-256 checksum. It installs per user, adds a Start menu shortcut, and offers a desktop shortcut and `.rsk` file association. Setup and uninstall preserve user data. CI installs twice to check upgrades, runs native chemistry, and checks uninstallation. Upgrades remove the old app-owned worker while preserving user drawings and caches.

The macOS disk image contains the signed app and an Applications shortcut. Both the app and disk image are notarized and stapled. CI mounts the image, copies the app out, and verifies chemistry, its signature and Gatekeeper status. The release has eight downloads plus `SHA256SUMS`.

## Test a build

Run **Actions → Release builds → Run workflow** on main. The default produces unsigned test artifacts without creating a release. Enable **Sign and notarize macOS test packages** to exercise the full signing path after configuring credentials.

```sh
gh workflow run release.yml --ref main
# Includes Developer ID and Apple notarization checks:
gh workflow run release.yml --ref main -f sign_macos=true
```

Each archive is extracted into a temporary directory with spaces outside the checkout. Two launches must return the expected ethanol formula and SMILES with an empty executable search path and unavailable Python/uv overrides. Packages must contain no Python worker or interpreter and create no chemistry environment. The relocated app must discover its bundled helper. macOS signatures are checked again afterward. Record graphical acceptance separately.

Before the native-runtime cutover, the [2026-09-20 release validation](https://github.com/Ameyanagi/ReShiki/actions/runs/35510386732) passed all five package checks, including native ARM Windows/Linux applications, fresh chemistry setup, and offline reuse. Apple Silicon also passed Developer ID signing, notarization, stapling, and Gatekeeper assessment. This manual run did not publish a release.

The [0.8.0 validation record](release-0.8.0-validation.md) tracks the current review, documentation, package and publication checks. The [0.7.1 record](release-0.7.1-validation.md) retains the previous release’s evidence.

## Publish a version

1. Update the package version in `Cargo.toml`, update `Cargo.lock`, and record release changes.
2. Run the checks and a manual release build. Review the resulting packages.
3. Commit and push, then create and push a matching tag, for example `v0.8.0` for version `0.8.0`.

```sh
git tag -a v0.8.0 -m "ReShiki 0.8.0"
git push origin v0.8.0
```

A `v*` tag triggers builds. A mismatched version fails before packaging. The macOS archive must be signed, notarized, stapled and verified before the release publishes; missing credentials fail the job instead of silently publishing an unsigned macOS download. All five packages and the complete live reference tests on macOS ARM64, Linux x64, and Windows x64 must pass before publication. Windows and Linux packages remain unsigned. Tags containing a prerelease suffix create a GitHub prerelease. Manual builds never publish a release.

## Configure macOS signing

Use a **Developer ID Application** certificate with its private key exported as a password-protected `.p12`. A development certificate cannot replace it. An existing Developer ID Application certificate can sign multiple apps from the same team.

The `macos-signing` GitHub environment contains:

| Type     | Name                           | Purpose                                    |
| -------- | ------------------------------ | ------------------------------------------ |
| Secret   | `MACOS_CERTIFICATE_P12_BASE64` | Base64-encoded certificate and private key |
| Secret   | `MACOS_CERTIFICATE_PASSWORD`   | Certificate export password                |
| Secret   | `APPLE_ID`                     | Apple account for notarization             |
| Secret   | `APPLE_APP_SPECIFIC_PASSWORD`  | App-specific password for that account     |
| Variable | `APPLE_TEAM_ID`                | Developer team ID                          |
| Variable | `MACOS_SIGNING_IDENTITY`       | Full Developer ID Application identity     |

Authenticate with `gh auth login`. To configure an exported certificate and notarization credentials locally:

```sh
python3 scripts/configure_macos_signing.py --certificate /path/to/developer-id.p12
```

The script prompts for the certificate password, team, identity and notarization credentials. Password entry is hidden. Credentials are sent to GitHub through standard input; they are never written into tracked files or printed. To add or rotate only the Apple account credentials after the certificate is configured:

```sh
python3 scripts/configure_macos_signing.py
```

Create an app-specific password at [Apple Account](https://account.apple.com/). Do not use your normal account password. GitHub secrets cannot be read back or copied directly from another repository. Keep the original encrypted certificate and password in your secure credential store.

The configuration script limits the environment to main and `v*` tags. Signing jobs are separate from compilation and use a temporary keychain that is removed afterward. Every bundled native helper and app is signed from the inside out with Hardened Runtime and a timestamp. Signing verifies the input archive's checksum and commit before submitting to Apple. The final extracted app must pass signature, stapling, Gatekeeper and chemistry checks.

Setup references: [Apple notarization](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution) and [GitHub certificate guidance](https://docs.github.com/en/actions/how-tos/deploy/deploy-to-third-party-platforms/sign-xcode-applications).

## Build locally

```sh
python3 scripts/build_release.py --fetch-inchi-source --installer
```

The native InChI helper is built for the application’s architecture and bundled beside it.
`--fetch-inchi-source` downloads only the pinned official archive and verifies its checksum
and source hashes. For offline builds, use `--inchi-source /path/to/INCHI-1-SRC` or
`--inchi-archive /path/to/INCHI-1-SRC.zip`; `--inchi-helper /path/to/reshiki-inchi-helper`
reuses a matching build with its adjacent `build.json`. The installed app never builds
or downloads this helper. Developers can select an existing helper with an absolute
`RESHIKI_INCHI_HELPER` path.

The application and InChI helper must match the selected CPU architecture, including ARM64 on Windows and Linux. Packaging checks both executable headers. Python is used for build scripts and optional reference tests only; it is not copied into the package.

Build runners are macOS 14, Windows Server 2022 x64, Windows 11 ARM, Ubuntu 22.04 x64, and Ubuntu 24.04 ARM. Pass `--target` to `scripts/build_release.py` to select the explicit Rust target; package names derive from that target, including when packaging Python uses a different architecture.

Build on the target operating system with Rust 1.95, a C/C++ compiler and Python 3.12. On Windows, use `python` instead of `python3`. Archives are written to `dist/releases/`.

On macOS, build the native helper first, then run `python3 scripts/build_macos_app.py` for a development app. Add `--portable --release` for an optimized bundle in `dist/ReShiki.app`. Both forms include the helper, license notices and an ad-hoc signature; release signing replaces that signature. `--inchi-helper` selects a matching prebuilt helper.

Install Inno Setup 6.7.3 for local Windows installer builds, or set `RESHIKI_ISCC` to its `ISCC.exe`. Installer verification installs and uninstalls the app, so run it in a disposable Windows account or CI runner. Omit `--installer` to build only a portable archive.

## In-app version checks

ReShiki queries this repository’s latest stable GitHub release asynchronously. Successful checks are cached for 24 hours; failures for one hour. Manual checks bypass the cache. Semantic version comparisons prevent downgrade prompts, and prereleases are excluded. **Release notes** opens the release page. **Update and restart** downloads and verifies a matching package, waits for unsaved work and pending assistant/input state to be resolved, then installs and restarts. Automatic checks never install without a click. No signing credentials or drawing data are used by the update check. See the [installation and update guide](https://reshiki.com/guide/install/).
