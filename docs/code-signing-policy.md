# Code signing policy

ReShiki publishes free desktop applications from the
[public source repository](https://github.com/Ameyanagi/ReShiki). Official downloads
are on [GitHub Releases](https://github.com/Ameyanagi/ReShiki/releases).

## Current signing status

- **macOS:** applications and disk images are signed with Apple Developer ID,
  notarized, and stapled. This applies to current stable and nightly packages.
- **Windows:** applications and setup programs are currently unsigned. We are
  preparing an application to SignPath Foundation; approval and Windows signing
  have not been granted or configured.
- **Linux:** packages are currently unsigned.

Verify a download against the release's `SHA256SUMS`. A checksum helps verify that
you received the published bytes; it is not a publisher signature. Each portable
package includes `build.json` with its source commit and package version.

## Proposed Windows signing policy

The following policy describes the intended process **if SignPath Foundation
accepts ReShiki**. It does not claim that existing Windows downloads are signed.

The acknowledgement after approval will be:

> Free code signing provided by [SignPath.io](https://signpath.io), certificate by
> [SignPath Foundation](https://signpath.org).

We intend to sign ReShiki's own Windows application executable and setup programs
for x64 and ARM64. Signing will use artifacts built by the public GitHub Actions
release workflow from reviewed source on `main`, with the source revision and
package version recorded. We will not submit unrelated software or upstream
projects' separate binaries under ReShiki's signing identity.

Before enabling this process, the Windows executable's product and version
metadata must be added, and SignPath's artifact restrictions must enforce the
ReShiki product name and matching build versions. Existing setup programs already
have product and version metadata. This application-preparation change does not
enable a signing integration or grant access to the repository.

### Team roles

| Role               | Member                                     | Responsibility                                                                                         |
| ------------------ | ------------------------------------------ | ------------------------------------------------------------------------------------------------------ |
| Author / committer | [@Ameyanagi](https://github.com/Ameyanagi) | Project creator and maintainer with repository write access.                                           |
| Reviewer           | [@Ameyanagi](https://github.com/Ameyanagi) | Review contributions from people without commit access before merging.                                 |
| Signing approver   | [@Ameyanagi](https://github.com/Ameyanagi) | Review the source revision, checks, and artifacts, and manually approve each SignPath signing request. |

Participants in SignPath signing must use multi-factor authentication for both
GitHub and SignPath. Every SignPath signing request will require the approver's
explicit approval; a passing build alone will not approve signing. SignPath will
hold the certificate's private key. No private signing key will be checked into
ReShiki's repository or distributed with the application.

Before enabling signing, review the installer privacy-disclosure and update-check
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
proposed Windows signing process.
