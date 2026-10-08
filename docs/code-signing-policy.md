# Code signing policy

ReShiki publishes free desktop applications from the
[public source repository](https://github.com/Ameyanagi/ReShiki). Official downloads
are on [GitHub Releases](https://github.com/Ameyanagi/ReShiki/releases).

## Current signing status

- **macOS:** applications and disk images are signed with Apple Developer ID,
  notarized, and stapled. This applies to current stable and nightly packages.
- **Windows:** existing published applications and setup programs are unsigned.
  SignPath Open Source Code Signing access has been granted, and ReShiki's project
  and GitHub.com trusted build system are configured. Production signing remains
  inactive while the Foundation release certificate is pending and CI setup is
  completed. See the [SignPath setup record](signpath-setup.md).
- **Linux:** packages are currently unsigned.

Verify a download against the release's `SHA256SUMS`. A checksum helps verify that
you received the published bytes; it is not a publisher signature. Each portable
package includes `build.json` with its source commit and package version.

## Windows signing policy

The following policy governs the Windows release path when production signing is
activated. A release is described as signed only after its signatures are verified;
SignPath access does not change the signing status of older downloads.

Free code signing provided by [SignPath.io](https://signpath.io), certificate by
[SignPath Foundation](https://signpath.org).

Signing covers ReShiki's own `reshiki.exe` and final Inno Setup programs for x64 and
ARM64. The portable archive contains the signed application; the setup program
embeds that same signed application and then receives its own signature. The
Inno-generated uninstaller is not signed by this integration. Signing uses
artifacts built by the public GitHub Actions release workflow from reviewed source
on `main`, with the source revision and package version recorded. We do not submit
unrelated software or upstream projects' separate binaries under ReShiki's signing
identity.

The application and setup programs have ReShiki product and version metadata.
Repository-controlled [artifact configurations](../.signpath/) restrict signing
to the expected filenames, `ReShiki` product name and matching package-version
strings, including the full nightly identifier. Requests specify these
configurations explicitly. Production publication is enabled separately from
internal test signing and has no unsigned Windows fallback once enabled.

### Team roles

| Role               | Member                                     | Responsibility                                                                                                 |
| ------------------ | ------------------------------------------ | -------------------------------------------------------------------------------------------------------------- |
| Author / committer | [@Ameyanagi](https://github.com/Ameyanagi) | Project creator and maintainer with repository write access.                                                   |
| Reviewer           | [@Ameyanagi](https://github.com/Ameyanagi) | Review contributions from people without commit access before merging.                                         |
| Signing approver   | [@Ameyanagi](https://github.com/Ameyanagi) | Review the source revision, checks, and artifacts, and manually approve each SignPath release-signing request. |

Participants in SignPath signing must use multi-factor authentication for both
GitHub and SignPath. Every `release-signing` request requires the approver's
explicit approval; a passing build alone does not approve production signing.
This includes separately approving the application and setup requests for each
architecture. The `test-signing` policy uses an untrusted test certificate and
does not require approval; those artifacts are used for internal validation and
are never published as public releases. SignPath holds the release certificate's
private key in its HSM. No private signing key is checked into ReShiki's repository
or distributed with the application.

Before enabling production signing, review the installer privacy-disclosure and update-check
opt-out requirements with SignPath Foundation.

Unsigned development builds may continue to be published with their signing
status clearly identified. A build will only be described as signed after its
signature has been verified.

## Privacy and further information

Read the [privacy policy](privacy-policy.md) for local data storage, automatic
update checks, and optional online features. ReShiki's update checks are enabled
by default; ordinary chemical drawing and calculations work offline.

See the [release and package verification process](releasing.md),
[license scope](../LICENSE), and [third-party notices](../NOTICE). SignPath's
[Foundation conditions](https://signpath.org/terms) govern eligibility and the
Windows signing process.
