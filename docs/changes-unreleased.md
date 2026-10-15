# Unreleased changes

- **Nightly candidate — Windows startup:** use Direct3D 12 consistently during automatic adapter discovery and renderer initialization, retaining explicit backend preferences and the Tiny Skia fallback. The candidate opens the editor in the Windows sandbox that reproduced the original startup failure. Installer antivirus detections remain under vendor review; this is not an antivirus-clearance claim. [Investigation and limits](release-0.12.0-windows-security.md).

- **Under review — 0.12.0 release validation:** record the tagged source, the tag runs that did not publish, the publishing run, public-download checksums and macOS signing and package checks. [Validation record](release-0.12.0-validation.md) · [PR #324](https://github.com/Ameyanagi/ReShiki/pull/324) · @Ameyanagi.

See [ReShiki 0.12.0](changes-0.12.md) for the latest release notes.
