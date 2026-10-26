# Release builds and code signing

See the [Code signing policy](code-signing-policy.md) for current platform signing
status. SignPath Open Source Code Signing access is approved; production Windows
signing remains inactive while the release certificate and its pin are pending.
Follow [SignPath setup](signpath-setup.md) to validate and activate it. Existing
Windows downloads remain unsigned. Production repository variables control stable
and nightly Windows signing separately; enabling one makes that publication path require
verified release-signed Windows packages.

The Release builds workflow produces signed macOS disk images, Windows x64/ARM64 setup programs, and portable packages for all six targets. macOS supports Apple Silicon and Intel; Windows and Linux support x64 and ARM64. Each package has one Rust application executable; InChI and macOS clipboard/printing run in isolated modes of that executable. Drawing and chemistry work offline without Python, RDKit or uv.

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
of drawings. Windows packages are unsigned unless `SIGNPATH_NIGHTLY_ENABLED=true`;
Linux packages remain unsigned. Installers replace the
existing installation; use a portable archive in a separate folder to retain Stable.
Older nightlies published before this installer workflow contain unsigned archives.

After a manual nightly-version test-signing run passes, enable
`SIGNPATH_NIGHTLY_TEST_ENABLED=true` to retain internal test-signed Windows
nightly artifacts. The same six-target build supplies both the public unsigned
Windows packages and the input for four test-policy requests: application and
setup for x64 and ARM64. Completed test packages are workflow artifacts named
`test-signed-windows-x64` and `test-signed-windows-arm64`; the public release
continues to publish unsigned Windows downloads with matching notes and checksums.
Keep the production switches disabled while the release certificate is pending.
If `SIGNPATH_NIGHTLY_ENABLED=true`, release signing takes precedence over this
internal test mode. See [internal test-signed nightlies](signpath-setup.md#run-internal-test-signed-nightlies).

```sh
gh workflow run nightly.yml --ref main
```

GitHub schedules run from the repository's default branch, `main`.
The workflow publishes only runs from `main` and only
after all six package jobs, both macOS signing jobs, and any enabled Windows
signing jobs pass. Signing failures
prevent publication; there is no unsigned macOS fallback, or unsigned Windows
fallback when production nightly Windows signing is enabled. An enabled internal
test-signing job must also pass before the nightly publishes. Standard build runners do not establish hardware
GPU performance: use the [Windows renderer checks](windows.md#release-performance-and-debugging)
on the test machine and record its adapter separately.

For an individual PR, run **Actions → Release builds → Run workflow** on its
feature branch to produce unsigned test artifacts without publishing. Enable
**Sign and notarize macOS test packages** on `main` to exercise the Apple signing
path after configuring credentials. Use `sign_windows=true` with
`windows_signing_policy=test-signing` to exercise Windows signing internally;
release-policy validation must run from `main` or an eligible release tag.
Test-signed Windows artifacts are never published. The
[SignPath setup guide](signpath-setup.md#validate-with-the-test-policy) describes
the credentials, certificate pins and approval sequence.

```sh
gh workflow run release.yml --ref YOUR_FEATURE_BRANCH
# Exercise nightly versioning and installers before merging the PR:
gh workflow run release.yml --ref YOUR_FEATURE_BRANCH -f nightly=true
# Includes Developer ID and Apple notarization checks:
gh workflow run release.yml --ref main -f sign_macos=true
# Internal Windows signing and installer validation; does not publish:
gh workflow run release.yml --ref main -f sign_windows=true -f windows_signing_policy=test-signing
```

Each archive is extracted into a temporary directory with spaces outside the checkout. Two launches must return the expected ethanol formula and SMILES with an empty executable search path and unavailable Python/uv overrides. Packages must contain no Python worker or interpreter and create no chemistry environment. The relocated app must relaunch itself for InChI work; packages with companion executables are rejected. macOS signatures are checked again afterward. Record graphical acceptance separately.

Each extracted archive, installed Windows copy and macOS disk-image copy also runs the experimental headless agent API from the same sanitized environment, with `scripts/agent_api_client.py` as an independent standard-library MCP client. `reshiki --mcp` must answer `server/discover` within 5 seconds (a warning above 2) with the 2026-07-28 revision and an experimental title, list the final tool catalog, analyze ethanol with its InChI, render a PNG and save only inside its granted `Out` folder. Escaping and `.exe` paths, a cancelled render, a malformed notification and an unsupported revision must get the documented answers, and the 2025-11-25 and 2025-06-18 handshakes must work in their own processes. On Unix the server also starts through a symlink. At the end of input it must exit 0 within 10 seconds (20 on Windows ARM64) with stdout closed. Every stdout line must be JSON-RPC, stderr must say experimental and never report a panic, only `Out` may change, the server must own no listening TCP socket (or the check reports `unavailable` when `lsof`, `ss` or `netstat` is missing), and no process from the package may remain. `reshiki --cli` must convert, analyze and print help, and an unknown command must exit 2 instead of opening the GUI. Signed macOS archives repeat the server start from a quarantined copy of the app, which must answer within 20 seconds. Windows setup is also rerun over a running `reshiki.exe --mcp`: it passes when setup closes the server and upgrades, or fails nonzero with the old installation intact, and the log records which; a setup still running after 5 minutes fails.

Extracted macOS archives and installed disk-image copies also run the actual application with its clipboard and print worker flags. Malformed, empty-write, missing-snapshot and oversized requests must return the expected errors without changing the clipboard or opening a print dialog. Private-pasteboard round trips and Save-to-PDF coordinate tests separately verify native behavior.

Before the native-runtime cutover, the [2026-09-20 release validation](https://github.com/Ameyanagi/ReShiki/actions/runs/35510386732) passed all five package checks, including native ARM Windows/Linux applications, fresh chemistry setup, and offline reuse. Apple Silicon also passed Developer ID signing, notarization, stapling, and Gatekeeper assessment. This manual run did not publish a release.

The [0.13.1 validation record](release-0.13.1-validation.md) tracks the current preparation, package review and publication prerequisites. The [0.13.0 validation record](release-0.13.0-validation.md) lists the automated checks for the release preparation and the manual smoke tests still to run. The [0.12.0 validation record](release-0.12.0-validation.md) records the source review, the tag runs that did not publish, the publishing run and public-download verification. The [0.11.0 record](release-0.11.0-validation.md) and [0.10.0 record](release-0.10.0-validation.md) retains the single-executable release evidence, and the [0.9.0 record](release-0.9.0-validation.md) and [0.8.0 record](release-0.8.0-validation.md) retain earlier checks.

## Publish a version

1. Update the package version in `Cargo.toml`, update `Cargo.lock`, and record release changes. If the website color reference is marked as Nightly, set `NIGHTLY` to `false` in `examples/theme_colors.rs`, regenerate it as the [theme guide](../presets/themes/README.md) describes, and remove the Nightly note from `website/src/content/docs/guide/color-palettes.mdx`.
2. Run the checks and a manual release build. Review the resulting packages.
3. Merge the release preparation pull request to `main`, then create and push a matching tag on its tested commit, for example `v0.13.1` for version `0.13.1`.

```sh
git tag -a v0.13.1 -m "ReShiki 0.13.1"
git push origin v0.13.1
```

A `v*` tag triggers builds. A mismatched version or a tagged commit outside `main` fails before packaging. The macOS archive must be signed, notarized, stapled and verified before the release publishes; missing credentials fail the job instead of silently publishing an unsigned macOS download. All six packages and the complete live reference tests on macOS ARM64, Linux x64, and Windows x64 must pass before publication. With `SIGNPATH_ENABLED=true`, the Windows application and final setup program must also pass release-signature verification before publication; missing credentials, approval, or signature verification fail the release. Until activation, Windows packages are unsigned. Linux packages remain unsigned. Tags containing a prerelease suffix create a GitHub prerelease. Manual **Release builds** runs never publish a release; **Nightly builds** runs publish prereleases from `main`.

After a release publishes, the **web_demo** job builds the browser editor with `scripts/build_wasm.sh`, packages it with `scripts/package_web_demo.sh --previous` and the published archive, replaces `reshiki-web-demo.tar.gz` on the `web-demo` prerelease, and runs the **Documentation** workflow. It leaves the asset alone when the published demo's `demo/VERSION` is newer. The Documentation workflow copies the asset into the site at [reshiki.com/demo](https://reshiki.com/demo/). Without that release the site builds without a demo. To publish the demo by hand:

```sh
gh release download web-demo -p reshiki-web-demo.tar.gz -D build/published
scripts/build_wasm.sh && scripts/package_web_demo.sh --previous build/published/reshiki-web-demo.tar.gz
gh release create web-demo --prerelease --latest=false --title "Browser editor demo" --notes "The browser editor at https://reshiki.com/demo/."
gh release upload web-demo dist/reshiki-web-demo.tar.gz --clobber
gh workflow run docs.yml --ref main
```

Skip the download and `--previous` for the first demo, and `gh release create` when the `web-demo` release exists.

The packaged demo names its JavaScript and WebAssembly by content hash (`src/editor.<hash>.js`, `dist/editor/reshiki_bg.<hash>.wasm` and so on), and only `index.html` keeps a fixed name, so a page always loads the JavaScript and WebAssembly from one build. `--previous` also keeps the files the published demo's `index.html` loads, because a page cached for up to 10 minutes before a release still asks for them; files from older releases drop out. GitHub Pages serves every file with `Cache-Control: max-age=600` and cannot set headers, and Cloudflare does not cache `.wasm` at the edge by default. To cache the hashed files for a year, add a Cloudflare Cache Rule on the reshiki.com zone:

- **When:** `starts_with(http.request.uri.path, "/demo/src/") or starts_with(http.request.uri.path, "/demo/dist/")`
- **Then:** Eligible for cache; Edge TTL: ignore cache-control header, 1 year; Browser TTL: override origin, 1 year.

Leave `/demo/` and `/demo/index.html` out of the rule, so a release reaches visitors within the 10-minute default.

The **npm** job then publishes the browser package `@ameyanagi/reshiki` (`packages/editor`) from the build the **web_demo** job tested, so the WebAssembly is built only once. It runs the package tests, sets the package version to the app's version from `Cargo.toml` (which the tag matches, so `v0.13.1` publishes `0.13.1`; the `version` in `packages/editor/package.json` is only a placeholder), and publishes with `npm publish --provenance`. Stable versions get the `latest` dist-tag. Prerelease tags such as `v0.13.1-rc.1` get `next`, and a stable version older than the current `latest` gets `release-<major.minor>`, so `bun add @ameyanagi/reshiki` keeps installing the newest stable release, the same way the GitHub release is marked as a prerelease. Nightly builds never publish to npm. A version already on npm is skipped, so rerunning a release is safe. Without a credential, or when npm refuses it (an expired token, or a trusted publisher that is not set up), the job leaves a notice and succeeds, so npm never fails a release. See [Configure npm publishing](#configure-npm-publishing).

Package staging uses `build/release-bundles`, outside Cargo’s `target` directory, so cache pruning cannot traverse bundled dependency license sources.

Packaging and macOS signing use Python's standard library. Windows signing uses
the SignPath GitHub action and a PowerShell verification helper. Intel macOS runs the native Rust chemistry tests, packaging/clipboard/print checks and extracted-application tests without installing RDKit: the pinned reference release has no Intel macOS wheel. Full live RDKit comparisons remain required on macOS ARM64, Windows x64 and Linux x64 before any package is published.

Reference/source HTTPS download helpers require the updated certifi bundle in
the locked development environment. Offline packaging and signing do not load
that dependency. Workflow jobs verify the runner's bundled Node before any
JavaScript action; selecting Node with `setup-node` cannot replace this check.
See the [security review](changes/security-dependency-audit.md) for exact runtime
baselines, remaining scoped findings and the release gate.

Both Mac builds use the fixed abbreviation drawing coordinates recorded on Apple Silicon. Intel's native tests exercise every predefined group as an isolated and attached fragment; these are application checks, not an independent Intel RDKit geometry capture. The updater selects a separate DMG for each Mac architecture.

## Configure npm publishing

The **npm** job runs in the `npm` GitHub environment, which GitHub creates on its first run. It publishes with npm trusted publishing (OIDC) when the repository variable `NPM_TRUSTED_PUBLISHING` is `true`, and otherwise with the `NPM_TOKEN` secret. The npm account `ameyanagi` owns the `@ameyanagi` scope.

npm can only add a trusted publisher to a package that already exists, so the first version is published with a token, and every later one through trusted publishing:

1. **First publish, with a token.** Create a [granular access token](https://docs.npmjs.com/creating-and-viewing-access-tokens) with **Packages and scopes: Read and write**, for all packages or the `@ameyanagi` scope, and a short expiry. Add it as the repository (or `npm` environment) secret `NPM_TOKEN`. Then publish main's version without a new tag:

   ```sh
   gh workflow run release.yml --ref main -f npm_only=true
   gh run watch "$(gh run list --workflow release.yml --limit 1 --json databaseId -q '.[0].databaseId')"
   ```

   `npm_only` skips every build, signing, GitHub release and demo job. The npm job builds and tests the package, then publishes the version in `Cargo.toml` on main. The next `v*` tag also uses the token while it is set. If npm refuses the token, the job ends with a notice: npm is restricting tokens that bypass two-factor authentication for direct publishing. In that case, publish the first version by hand from a checkout of main, with two-factor authentication:

   ```sh
   scripts/build_wasm.sh --test
   cd packages/editor
   bun pm pkg set version="$(awk -F '"' '/^version = / { print $2; exit }' ../../Cargo.toml)"
   npm login
   npm publish --access public
   git checkout package.json
   ```

2. **Add the trusted publisher.** Open **npmjs.com → Packages → @ameyanagi/reshiki → Settings → Trusted publishing**, choose **GitHub Actions** and enter:

   | Field                   | Value         |
   | ----------------------- | ------------- |
   | Organization or user    | `Ameyanagi`   |
   | Repository              | `ReShiki`     |
   | Workflow filename       | `release.yml` |
   | Environment name        | `npm`         |
   | Allow direct publishing | on            |

   Then set the repository variable `NPM_TRUSTED_PUBLISHING` to `true` (**Settings → Secrets and variables → Actions → Variables**). From then on the job uses OIDC (`id-token: write`, npm 11.5.1 or later from Node 24, provenance included) and ignores `NPM_TOKEN`. npm expires a new trusted publisher that has not published within 2 days. If yours expired before a release, delete it and add it again.

3. **Delete the token.** Once a version has published through trusted publishing, delete the `NPM_TOKEN` secret and revoke the token on npmjs.com. Under the package's **Settings → Publishing access**, choose to require two-factor authentication and disallow tokens. Trusted publishing keeps working.

After that, every version tag publishes to npm with no further steps.

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

The configuration script limits the environment to main and `v*` tags. Signing jobs are separate from compilation and use a temporary keychain that is removed afterward. The application executable and app bundle are signed with Hardened Runtime and a timestamp. Signing verifies the input archive's checksum and commit before submitting to Apple. The final extracted app must pass signature, stapling, Gatekeeper and chemistry checks.

Setup references: [Apple notarization](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution) and [GitHub certificate guidance](https://docs.github.com/en/actions/how-tos/deploy/deploy-to-third-party-platforms/sign-xcode-applications).

## Configure Windows signing

Follow [SignPath setup](signpath-setup.md) to register the checked-in artifact
configurations, configure the `windows-signing` environment and certificate pins,
validate the test policy, then validate the issued Foundation release certificate
with manual approvals. Keep `SIGNPATH_ENABLED` and `SIGNPATH_NIGHTLY_ENABLED`
disabled until their respective production checks pass.
Keep `SIGNPATH_NIGHTLY_TEST_ENABLED=false` until both architectures pass a manual
nightly-version test-policy run, then enable it separately for internal nightly
workflow artifacts. It does not activate public Windows signatures.

For each architecture, signing starts from the qualified portable archive,
verifies its checksum and provenance, and signs `reshiki.exe`. The workflow builds
the portable ZIP and Inno installer from that same signed application, signs the
final setup executable, repeats package and installer checks, then generates the
final checksum sidecars. The portable ZIP is a container for the signed app; it
does not receive its own Authenticode signature. The Inno-generated uninstaller
remains unsigned. Publication uses checksums of the final signed packages in
`SHA256SUMS`.

Production uses only `release-signing`, with separate manual application and setup
approvals for x64 and ARM64. Internal `test-signing` requests use a pinned untrusted
certificate and cannot enter public stable or nightly releases. Nightly production
signing is separately enabled because it requires approvals for each daily build.

## Build locally

```sh
python3 scripts/build_release.py --installer
```

Cargo builds the application and its InChI worker from the locked dependency graph. No separate C InChI source download, Swift compiler step or companion executable is needed. For an offline build, populate Cargo's cache first and set `CARGO_NET_OFFLINE=true`. Developers can still select a test-only worker with an absolute `RESHIKI_INCHI_HELPER` path; packaged runtime verification removes that override.

Packaging checks the application's selected CPU architecture, including ARM64 on Windows and Linux, and verifies that each portable archive contains exactly one executable. Python is used for build scripts and optional reference tests only; it is not copied into packages. License packaging retains four project files and one `THIRD-PARTY-NOTICES.txt`, with full source/dependency texts and indexed attribution. Windows upgrades remove only the former app-owned worker and expanded license layout.

Build runners are macOS 14 on Apple Silicon, macOS 15 on Intel, Windows Server 2022 x64, Windows 11 ARM, Ubuntu 22.04 x64, and Ubuntu 24.04 ARM. Pass `--target` to `scripts/build_release.py` to select the explicit Rust target; package names derive from that target, including when packaging Python uses a different architecture.

Build on the target operating system with Rust 1.99.0, a C/C++ compiler and Python 3.12. On Windows, use `python` instead of `python3`. Archives are written to `dist/releases/`.

On macOS, run `python3 scripts/build_macos_app.py` for a development app. Add `--portable --release` for an optimized bundle in `dist/ReShiki.app`. Both forms include one executable, the icon, license notices and an ad-hoc signature; release signing replaces that signature.

Install Inno Setup 6.7.3 for local Windows installer builds, or set `RESHIKI_ISCC` to its `ISCC.exe`. Installer verification installs and uninstalls the app, so run it in a disposable Windows account or CI runner. Omit `--installer` to build only a portable archive.

## In-app version checks

ReShiki defaults to **Stable** and saves a **Stable / Nightly** choice in the existing update window. Stable queries GitHub’s latest stable release. Nightly searches published prereleases whose tags match `nightly-<base>-nightly.<date>.<run>.<attempt>`; drafts and unrelated prereleases are excluded. Successful checks are cached separately for each channel for 24 hours; failures for one hour. Manual checks and channel changes bypass the cache, and outdated in-flight results cannot replace the current channel’s result.

Within a channel, semantic version comparison offers only newer builds. Switching between an installed stable and nightly version offers the selected channel’s release even if its semantic version is lower, including returning from a nightly to the latest stable. The automatic-check preference retains its older Boolean file format so returning to an older stable build preserves an opt-out.

**Release notes** opens the selected release page. Stable **Update and restart** downloads and verifies a matching package, waits for unsaved work and pending assistant/input state to be resolved, then installs and restarts. Nightly **Download nightly ↗** opens the matching DMG or Windows setup in the browser when that asset is published, falling back to the portable ZIP or Linux tar.gz for older nightlies. Installation remains manual; **Release notes** includes both installer and portable choices. Nightlies cannot enter the automatic stable installer. Automatic checks never install without a click. No signing credentials or drawing data are used by the update check. See the [installation and update guide](https://reshiki.com/guide/install/).
