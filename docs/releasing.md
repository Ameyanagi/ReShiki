# Release builds and macOS signing

The Release builds workflow produces native portable packages for Apple Silicon macOS, Windows x64/ARM64 and Linux x64/ARM64. Intel macOS is not supported. Users install uv. Packages include the chemistry source and a dependency lockfile; uv installs Python and chemistry libraries into a local user environment on first use. Build outputs contain dependency notices, source/version metadata and SHA-256 checksums.

## Test a build

Run **Actions → Release builds → Run workflow** on main. The default produces unsigned test artifacts without creating a release. Enable **Sign and notarize macOS test packages** to exercise the full signing path after configuring credentials.

```sh
gh workflow run release.yml --ref main
# Includes Developer ID and Apple notarization checks:
gh workflow run release.yml --ref main -f sign_macos=true
```

Each archive is extracted into a temporary directory with spaces outside the checkout. The extracted executable must report clear missing-uv instructions, set up a fresh local environment and return the expected ethanol formula and SMILES. A second launch must work with uv in offline mode. On macOS, the app signature is verified again after setup. This is a packaging check, not a complete graphical acceptance test. Record desktop checks separately from package verification.

The [2026-09-20 release validation](https://github.com/Ameyanagi/ReShiki/actions/runs/35510386732) passed all five package checks, including native ARM Windows/Linux applications, fresh chemistry setup, and offline reuse. Apple Silicon also passed Developer ID signing, notarization, stapling, and Gatekeeper assessment. This manual run did not publish a release.

## Publish a version

1. Update the package version in `Cargo.toml`, update `Cargo.lock`, and record release changes.
2. Run the checks and a manual release build. Review the resulting packages.
3. Commit and push, then create and push a matching tag, for example `v0.3.0` for version `0.3.0`.

```sh
git tag -a v0.3.0 -m "ReShiki 0.3.0"
git push origin v0.3.0
```

A `v*` tag triggers builds. A mismatched version fails before packaging. The macOS archive must be signed, notarized, stapled and verified before the release publishes; missing credentials fail the job instead of silently publishing an unsigned macOS download. All five targets must succeed. Windows and Linux packages remain unsigned. Tags containing a prerelease suffix create a GitHub prerelease. Manual builds never publish a release.

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
uv run --locked python scripts/configure_macos_signing.py --certificate /path/to/developer-id.p12
```

The script prompts for the certificate password, team, identity and notarization credentials. Password entry is hidden. Credentials are sent to GitHub through standard input; they are never written into tracked files or printed. To add or rotate only the Apple account credentials after the certificate is configured:

```sh
uv run --locked python scripts/configure_macos_signing.py
```

Create an app-specific password at [Apple Account](https://account.apple.com/). Do not use your normal account password. GitHub secrets cannot be read back or copied directly from another repository. Keep the original encrypted certificate and password in your secure credential store.

The configuration script limits the environment to main and `v*` tags. Signing jobs are separate from compilation and use a temporary keychain that is removed afterward. Every bundled native helper and app is signed from the inside out with Hardened Runtime and a timestamp. Signing verifies the input archive's checksum and commit before submitting to Apple. The final extracted app must pass signature, stapling, Gatekeeper and chemistry checks.

Setup references: [Apple notarization](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution) and [GitHub certificate guidance](https://docs.github.com/en/actions/how-tos/deploy/deploy-to-third-party-platforms/sign-xcode-applications).

## Build locally

```sh
uv sync --locked --python 3.12
uv run --locked python scripts/build_release.py
```

Windows ARM uses a native ARM64 application and an x64 Python/RDKit worker through Windows 11's built-in emulation, because RDKit does not publish Windows ARM wheels. Linux ARM uses native aarch64 chemistry packages. The lock resolver checks the supported chemistry environments. Packaging checks the CPU architecture of both the application and its installed worker, and every build runs the Python regression suite before packaging.

Build runners are macOS 14, Windows Server 2022 x64, Windows 11 ARM, Ubuntu 22.04 x64, and Ubuntu 24.04 ARM. Pass `--target` to `scripts/build_release.py` to select the explicit Rust target; package names derive from that target, including when packaging Python uses a different architecture.

Build on the target operating system. Python and RDKit are installed by the user’s uv at runtime. Archives are written to `dist/releases/`. On macOS, `scripts/build_macos_app.py` still builds the development app, and `--portable --release` builds an optimized app with the worker source and lockfile included.
