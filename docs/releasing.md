# Release builds and macOS signing

The Release builds workflow produces signed macOS disk images, Windows x64/ARM64 setup programs, and portable packages for all six targets. macOS supports Apple Silicon and Intel; Windows and Linux support x64 and ARM64. Packages include the Rust application and native InChI helper. Drawing and chemistry work offline without Python, RDKit or uv.

Windows setup uses Inno Setup 6.7.3, downloaded with a pinned SHA-256 checksum. It installs per user, adds a Start menu shortcut, and offers a desktop shortcut and `.rsk` file association. Setup and uninstall preserve user data. CI installs twice to check upgrades, runs native chemistry, and checks uninstallation. Upgrades remove the old app-owned worker while preserving user drawings and caches.

The macOS disk image contains the signed app and an Applications shortcut. Both the app and disk image are notarized and stapled. CI mounts the image, copies the app out, and verifies chemistry, its signature and Gatekeeper status. The release has ten downloads plus `SHA256SUMS`. Both publishers validate the exact ten filenames and their checksum sidecars before creating the shared OS/architecture/installer/portable download table. macOS archive manifests must declare the matching version, architecture, signature and notarization. Generated GitHub release notes are retained after this table.

Keep the homepage, README, and installation guide's primary download links pointed
at [the latest stable release](https://github.com/Ameyanagi/ReShiki/releases/latest).
Nightly publication uses `--prerelease --latest=false`; reserve GitHub's **Latest**
designation for stable releases. This designation does not reorder the all-releases
list, where newer nightlies can appear above stable. See [GitHub's release options](https://cli.github.com/manual/gh_release_create).
Link to the [nightly installation instructions](https://reshiki.com/guide/install/#nightly-builds)
as a secondary path for testers.

## Test a build

Open development pull requests against `main`, the default and development
branch. Tag tested commits on `main` for stable releases.

The **Nightly builds** workflow builds `main` daily at 18:37 UTC (03:37 JST), or on
manual dispatch with `main` selected. Each successful run publishes a downloadable
GitHub prerelease with ten downloads and `SHA256SUMS`: signed and notarized macOS
DMG/ZIP for both architectures, Windows setup/ZIP for both architectures, and
Linux archives for both architectures. Nightlies reuse the stable installer,
Developer ID signing, notarization and extracted-application verification paths. The complete
live reference suite remains a stable-release requirement; nightly package checks
do not replace it.

Each nightly has a unique `VERSION-nightly.YYYYMMDD.RUN_ID.ATTEMPT` package version,
embedded in the executable and recorded with its source commit in `build.json`.
Build and signing jobs stamp the same version into their temporary Cargo manifest
and lockfile, so archive provenance must match before signing. Numeric macOS
`CFBundleVersion`/`CFBundleShortVersionString` and Windows setup `VersionInfoVersion`
retain the three-component base version. The full nightly identifier is retained in
the executable, `build.json`, macOS `ReShikiPackageVersion`, and Windows setup display
and text-version fields. GitHub's large run ID is never put in a Windows 16-bit
version component. See [Apple bundle versions](https://developer.apple.com/documentation/bundleresources/information-property-list/cfbundleversion),
[Apple display versions](https://developer.apple.com/documentation/bundleresources/information-property-list/cfbundleshortversionstring),
and [Inno Setup version fields](https://jrsoftware.org/ishelp/topic_setup_versioninfoversion.htm).

The workflow stamps only its temporary checkout. Published nightly tags start with
`nightly-`, so they do not trigger the stable `v*` release workflow. Prereleases are
excluded from the stable updater. Keep the stable installation and test with copies
of drawings. Windows and Linux packages remain unsigned. Installers replace the
existing installation; use a portable archive in a separate folder to retain Stable.
Older nightlies published before this installer workflow contain unsigned archives.

```sh
gh workflow run nightly.yml --ref main
```

GitHub schedules run from the repository's default branch, `main`.
The workflow publishes only runs from `main` and only
after all six package jobs and both macOS signing jobs pass. Signing failures
prevent publication; there is no unsigned macOS fallback. Standard build runners do not establish hardware
GPU performance: use the [Windows renderer checks](windows.md#release-performance-and-debugging)
on the test machine and record its adapter separately.

For an individual PR, run **Actions → Release builds → Run workflow** on its
feature branch to produce unsigned test artifacts without publishing. Enable
**Sign and notarize macOS test packages** on `main` to exercise the full signing
path after configuring credentials.

```sh
gh workflow run release.yml --ref YOUR_FEATURE_BRANCH
# Exercise nightly versioning and installers before merging the PR:
gh workflow run release.yml --ref YOUR_FEATURE_BRANCH -f nightly=true
# Includes Developer ID and Apple notarization checks:
gh workflow run release.yml --ref main -f sign_macos=true
```

Each archive is extracted into a temporary directory with spaces outside the checkout. Two launches must return the expected ethanol formula and SMILES with an empty executable search path and unavailable Python/uv overrides. Packages must contain no Python worker or interpreter and create no chemistry environment. The relocated app must discover its bundled helper. macOS signatures are checked again afterward. Record graphical acceptance separately.

Before the native-runtime cutover, the [2026-09-20 release validation](https://github.com/Ameyanagi/ReShiki/actions/runs/35510386732) passed all five package checks, including native ARM Windows/Linux applications, fresh chemistry setup, and offline reuse. Apple Silicon also passed Developer ID signing, notarization, stapling, and Gatekeeper assessment. This manual run did not publish a release.

The [0.9.0 validation record](release-0.9.0-validation.md) records the completed source review, documentation validation, package checks and public-download verification. The [0.8.0 record](release-0.8.0-validation.md) retains the previous release’s evidence.

## Publish a version

1. Update the package version in `Cargo.toml`, update `Cargo.lock`, and record release changes.
2. Run the checks and a manual release build. Review the resulting packages.
3. Merge the release preparation pull request to `main`, then create and push a matching tag on its tested commit, for example `v0.9.0` for version `0.9.0`.

```sh
git tag -a v0.9.0 -m "ReShiki 0.9.0"
git push origin v0.9.0
```

A `v*` tag triggers builds. A mismatched version or a tagged commit outside `main` fails before packaging. The macOS archive must be signed, notarized, stapled and verified before the release publishes; missing credentials fail the job instead of silently publishing an unsigned macOS download. All six packages and the complete live reference tests on macOS ARM64, Linux x64, and Windows x64 must pass before publication. Windows and Linux packages remain unsigned. Tags containing a prerelease suffix create a GitHub prerelease. Manual **Release builds** runs never publish a release; **Nightly builds** runs publish prereleases from `main`.

Package staging uses `build/release-bundles`, outside Cargo’s `target` directory, so cache pruning cannot traverse bundled dependency license sources.

Packaging and signing use only Python's standard library. Intel macOS runs the native Rust chemistry tests, packaging/clipboard/print checks and extracted-application tests without installing RDKit: the pinned reference release has no Intel macOS wheel. Full live RDKit comparisons remain required on macOS ARM64, Windows x64 and Linux x64 before any package is published.

Both Mac builds use the fixed abbreviation drawing coordinates recorded on Apple Silicon. Intel's native tests exercise every predefined group as an isolated and attached fragment; these are application checks, not an independent Intel RDKit geometry capture. The updater selects a separate DMG for each Mac architecture.

## Configure macOS signing

Use a **Developer ID Application** certificate with its private key exported as a password-protected `.p12`. A development certificate cannot replace it. An existing Developer ID Application certificate can sign multiple apps from the same team.

The Nightly workflow calls the reusable release workflow with `secrets: inherit`.
Keep this explicit even though the signing jobs select `environment: macos-signing`:
without inheritance, environment variables can resolve while environment secrets
remain empty in the called workflow. See [GitHub's reusable-secret documentation](https://docs.github.com/en/actions/how-tos/reuse-automations/reuse-workflows#using-inputs-and-secrets-in-a-reusable-workflow)
and the matching [runner issue](https://github.com/actions/runner/issues/4453).
Signing and publication must still fail when credentials are unavailable.

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

Build runners are macOS 14 on Apple Silicon, macOS 15 on Intel, Windows Server 2022 x64, Windows 11 ARM, Ubuntu 22.04 x64, and Ubuntu 24.04 ARM. Pass `--target` to `scripts/build_release.py` to select the explicit Rust target; package names derive from that target, including when packaging Python uses a different architecture.

Build on the target operating system with Rust 1.95, a C/C++ compiler and Python 3.12. On Windows, use `python` instead of `python3`. Archives are written to `dist/releases/`.

On macOS, build the native helper first, then run `python3 scripts/build_macos_app.py` for a development app. Add `--portable --release` for an optimized bundle in `dist/ReShiki.app`. Both forms include the helper, license notices and an ad-hoc signature; release signing replaces that signature. `--inchi-helper` selects a matching prebuilt helper.

Install Inno Setup 6.7.3 for local Windows installer builds, or set `RESHIKI_ISCC` to its `ISCC.exe`. Installer verification installs and uninstalls the app, so run it in a disposable Windows account or CI runner. Omit `--installer` to build only a portable archive.

## In-app version checks

ReShiki defaults to **Stable** and saves a **Stable / Nightly** choice in the existing update window. Stable queries GitHub’s latest stable release. Nightly searches published prereleases whose tags match `nightly-<base>-nightly.<date>.<run>.<attempt>`; drafts and unrelated prereleases are excluded. Successful checks are cached separately for each channel for 24 hours; failures for one hour. Manual checks and channel changes bypass the cache, and outdated in-flight results cannot replace the current channel’s result.

Within a channel, semantic version comparison offers only newer builds. Switching between an installed stable and nightly version offers the selected channel’s release even if its semantic version is lower, including returning from a nightly to the latest stable. The automatic-check preference retains its older Boolean file format so returning to an older stable build preserves an opt-out.

**Release notes** opens the selected release page. Stable **Update and restart** downloads and verifies a matching package, waits for unsaved work and pending assistant/input state to be resolved, then installs and restarts. Nightly **Download nightly ↗** opens the matching DMG or Windows setup in the browser when that asset is published, falling back to the portable ZIP or Linux tar.gz for older nightlies. Installation remains manual; **Release notes** includes both installer and portable choices. Nightlies cannot enter the automatic stable installer. Automatic checks never install without a click. No signing credentials or drawing data are used by the update check. See the [installation and update guide](https://reshiki.com/guide/install/).
