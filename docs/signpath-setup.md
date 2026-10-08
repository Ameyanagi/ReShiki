# Configure Windows signing with SignPath

ReShiki has SignPath Open Source Code Signing access. Production Windows signing
is not active yet: the Foundation release certificate is pending, the release
certificate pin must be configured, and the complete signing path must
pass a manual run. The two artifact configurations and GitHub environment are
configured, including the CI token and test certificate pin. Existing Windows downloads remain unsigned. This record describes
the configuration inspected on **2026-10-09** and the remaining setup.

## Confirm the SignPath project

Open [SignPath](https://app.signpath.io/) using the maintainer account with MFA.
The inspected settings are:

| Setting                  | Value                                                                                       |
| ------------------------ | ------------------------------------------------------------------------------------------- |
| Organization ID          | `c91a089d-e2bc-458e-8453-0d11619e584e`                                                      |
| Project slug             | `ReShiki`                                                                                   |
| Repository               | `https://github.com/Ameyanagi/ReShiki.git`                                                  |
| Trusted build system     | `GitHub.com`, active and linked to `ReShiki`                                                |
| Release policy           | `release-signing`, currently invalid while its certificate is pending                       |
| Release certificate      | `Release certificate 2026`, HSM key, CSR pending                                            |
| Release submitter        | `CI builds`                                                                                 |
| Release approver         | `Ryuichi Shimogawa`, one approval required                                                  |
| Release verification     | Trusted build system and origin verification required                                       |
| Release allowed branches | `**` at inspection; review before activation                                                |
| Test policy              | `test-signing`, valid; no approval, trusted-build-system or origin-verification requirement |

The valid test policy does not establish production readiness. Its certificate
is not publicly trusted. See SignPath's [test and release certificate
documentation](https://docs.signpath.io/managing-certificates).

The GitHub App installation was not confirmed during the inspection. In GitHub's
**Settings → Applications → Installed GitHub Apps**, inspect the
[SignPath App](https://github.com/apps/signpath). Install or configure it with
**Only select repositories → ReShiki**. Then verify the project is still linked to the active
GitHub.com trusted build system. SignPath documents the App and GitHub-hosted
runner requirements in its [GitHub integration
guide](https://docs.signpath.io/trusted-build-systems/github).

Review the current `**` branch allowance with the Foundation before enabling
production. Restrict the policy to the reviewed release origins accepted by
SignPath. Check the origin shown for both a `main` build and a release-tag build;
do not assume that a tag pattern is a branch name. The workflow also restricts
production signing to `main` and version tags whose commits belong to `main`.
Keep one manual approval, trusted-build-system verification and origin
verification enabled for `release-signing`.

## Register the artifact configurations

In the project's **Artifact configurations**, create configurations using these
exact slugs and copy the corresponding checked-in XML:

| Slug                | XML                                                         | Required request parameters | Signed file inside the uploaded ZIP                    |
| ------------------- | ----------------------------------------------------------- | --------------------------- | ------------------------------------------------------ |
| `windows-app`       | [windows-app.xml](../.signpath/windows-app.xml)             | `version`                   | `reshiki.exe`                                          |
| `windows-installer` | [windows-installer.xml](../.signpath/windows-installer.xml) | `version`, `architecture`   | `reshiki-${version}-windows-${architecture}-setup.exe` |

Both enforce `ProductName=ReShiki` and exact `ProductVersion` and `FileVersion`
strings matching the supplied package version. For a nightly, supply the entire
`VERSION-nightly.DATE.RUN_ID.ATTEMPT` string. Numeric VERSIONINFO values remain the
three-component base version plus a zero fourth component; they do not contain
GitHub's run ID. The installer request's architecture is supplied by the workflow
and validated as `x64` or `arm64` before submission. XML parameters themselves do
not define an architecture enumeration.

Both configurations were registered and reported `VALID` during setup. Their
signing directives specify SHA-256 and preserve the Open Source account's
required signature watermark; custom signature descriptions or description URLs
are rejected by this account.

The root is `<zip-file>` because `actions/upload-artifact@v7` creates a ZIP around
the single staged executable. Upload that executable directly at the archive
root. Do not upload the already-packaged portable ZIP for the `windows-app`
request; that would add another archive level. Keep the normal ZIP behavior and
the signing action's normal decompression behavior. Each configuration signs one
explicit PE file and does not sign other files added to an upload. Required
parameters, nested paths and match counts follow SignPath's [artifact syntax
documentation](https://docs.signpath.io/artifact-configuration/syntax); metadata
restrictions follow its [artifact reference](https://docs.signpath.io/artifact-configuration/reference).

The project's initial `default` configuration signs an unrestricted PE file.
Retire or replace it with the same restrictions after registering and validating
the new configurations. Release requests must explicitly select `windows-app` or
`windows-installer`; do not leave an unrestricted configuration available for
production signing. Compare the portal XML with the repository when changing
either configuration. Use a new slug for an incompatible artifact layout, as
recommended by SignPath's [project configuration
guide](https://docs.signpath.io/projects).

## Configure GitHub credentials

Create the **`windows-signing`** GitHub environment in
[ReShiki's repository settings](https://github.com/Ameyanagi/ReShiki/settings/environments).
Add the following secret and environment variables. The two enable switches are
**repository variables**, so publication and the reusable release workflow can
read them outside the signing environment.

| Location             | Name                                  | Value                                                                 |
| -------------------- | ------------------------------------- | --------------------------------------------------------------------- |
| Environment secret   | `SIGNPATH_API_TOKEN`                  | API token for the `CI builds` submitter                               |
| Environment variable | `SIGNPATH_ORGANIZATION_ID`            | `c91a089d-e2bc-458e-8453-0d11619e584e`                                |
| Environment variable | `SIGNPATH_PROJECT_SLUG`               | `ReShiki`                                                             |
| Environment variable | `SIGNPATH_TEST_CERTIFICATE_SHA256`    | SHA-256 of the test policy's DER-encoded public certificate           |
| Environment variable | `SIGNPATH_RELEASE_CERTIFICATE_SHA256` | SHA-256 of the issued release policy's DER-encoded public certificate |
| Repository variable  | `SIGNPATH_ENABLED`                    | `false` until production validation completes                         |
| Repository variable  | `SIGNPATH_NIGHTLY_ENABLED`            | `false`; enable separately to require signed nightlies                |

In SignPath's **Users and Groups**, open the existing `CI builds` CI user and
reuse its previously saved API token, or regenerate it if it was not saved. Verify that this user is a submitter for the two intended
ReShiki policies and has no unnecessary administrator permissions. Store the
token directly in GitHub's environment-secret field; do not place it in source
files, commands, logs or chat. SignPath displays a token only when generated;
regenerating it can invalidate an existing integration. See its [CI user
documentation](https://docs.signpath.io/users#ci-users).

Download each policy's **public X.509 certificate** from SignPath to calculate its
pin. Do not use a CSR, a private-key export, or the portal's SHA-1 thumbprint. In
PowerShell 7, for a downloaded `.cer` file:

```powershell
$certificate = [System.Security.Cryptography.X509Certificates.X509Certificate2]::new((Resolve-Path '.\signpath-test.cer').Path)
$certificate.GetCertHashString([System.Security.Cryptography.HashAlgorithmName]::SHA256)
```

The method hashes the certificate's DER bytes, even when the downloaded file uses
another supported encoding. Store the resulting 64 hexadecimal characters in the
matching variable. Repeat
for the issued release certificate when it becomes available. Pin the certificate
used by the policy, and update the pin deliberately when that certificate is
rotated. CI checks returned signatures against these pins; it does not trust a
publisher name alone.

Restrict `windows-signing` deployment branches to `main` and `v*` release tags.
The workflow has its own origin gates as well. The nightly reusable call must
retain `secrets: inherit`, and signing jobs need `actions: read` and
`contents: read` permissions. The integration uses
`signpath/github-action-submit-signing-request@v3`, explicitly passes each artifact
configuration slug and downloads the completed signed artifact. See the
[SignPath action inputs](https://docs.signpath.io/trusted-build-systems/github#action-input-parameters).

## Validate with the test policy

Keep both publication switches `false`. On `main`, dispatch **Release builds**
with **Sign Windows test packages** enabled and `windows_signing_policy` set to
`test-signing`:

```sh
gh workflow run release.yml --ref main -f sign_windows=true -f windows_signing_policy=test-signing
```

This manual run does not publish a release. Repeat with `-f nightly=true` to
exercise the full nightly version string. The workflow also permits internal
test-policy dispatches on a feature branch; the signing environment must allow
that branch before such a run can access its credentials. Both x64 and ARM64 must
complete the
following sequence:

1. Check the qualified unsigned archive's checksum, source commit, version,
   architecture and expected contents, then stage its `reshiki.exe`.
2. Upload and sign the application using `windows-app`; verify the returned
   executable's version metadata, timestamped Authenticode signature and
   test-certificate pin.
3. Put that signed executable in the portable package and build Inno Setup from
   the same package contents.
4. Upload and sign the final setup executable using `windows-installer`; verify
   its metadata, signature and pin.
5. Retest the final portable package and installed application, including
   upgrades and uninstallation, and generate package checksum sidecars from the
   final bytes.

Review both architectures' SignPath request records and workflow logs. Test
signatures lack public trust. After matching the DER SHA-256 pin, the verification
helper temporarily trusts that test certificate on the disposable GitHub-hosted
Windows runner to validate Authenticode and its timestamp, and removes that trust
in its cleanup path. Do not install the test certificate on users' computers.
Do not publish
test-signed packages or describe them as trusted Windows releases. Passing this
run also does not exercise the release policy's approval and origin requirements.

Only the application and final setup executable are signed. Inno Setup's generated
uninstaller remains unsigned; signing the outer setup program does not sign it.
The installer privacy disclosure and update-check opt-out requirements must also
be reviewed with SignPath Foundation before production activation.

## Activate production signing

The inspected `Release certificate 2026` entry is still a pending CSR and has no
validity dates. A CSR is not an issued certificate and cannot supply the release
certificate pin. Wait for the Foundation's certificate-issuance process. If the
entry remains pending, contact the Foundation through the existing approval
correspondence with the organization ID, project slug and pending certificate
name; ask what remains to complete issuance. Do not generate a replacement key or
buy/upload an unrelated certificate as a workaround. SignPath explains its HSM
CSR flow in [Managing certificates](https://docs.signpath.io/managing-certificates#certificate-types).

After the certificate is issued, confirm that `release-signing` is valid, download
its public certificate and set `SIGNPATH_RELEASE_CERTIFICATE_SHA256`. Dispatch the
same manual workflow using `-f windows_signing_policy=release-signing`. For each
architecture, the approver must review and approve the application request before
the installer request can be built and submitted, then review and approve that
installer request. Check the commit, version, artifact configuration and verified
origin for every request. Keep the workflow running while completing its approvals.

Before activation, both architectures must pass public-trust verification with a
timestamp, the release-certificate pin, final package tests, install/upgrade and
uninstall checks. Record the run and request URLs in the release validation
record. Only then set repository variable `SIGNPATH_ENABLED=true`. Stable tag
publication now requires the signed Windows artifacts and fails if signing or
verification fails; it does not substitute unsigned packages.

Nightly signing remains a separate choice. Set `SIGNPATH_NIGHTLY_ENABLED=true`
only after validating a nightly-version release-signing run and arranging manual
approvals for each daily build. Leave it `false` to continue clearly labelled
unsigned Windows nightlies. Test signing is never used for public nightlies.

The final Windows ZIP and setup checksums are generated after signing and package
verification. Publication generates `SHA256SUMS` from those final packages. An
unsigned build's checksums cannot be reused because signing changes file bytes.
See the [code signing policy](code-signing-policy.md), [release process](releasing.md)
and [Foundation conditions](https://signpath.org/terms).
