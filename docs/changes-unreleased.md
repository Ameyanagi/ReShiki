# Unreleased changes

## Under review

**Copy as names unsupported text labels, points to ordinary Copy and exposes the
full refusal behind a visible status cue; narrow windows keep every footer
control visible.** @Ameyanagi in
[PR #565](https://github.com/Ameyanagi/ReShiki/pull/565), addressing
[#499](https://github.com/Ameyanagi/ReShiki/issues/499).

CDXML/CDX refusals identify the actual unsupported label instead of calling the
drawing invalid or listing absent ring curves. They suggest the platform's
ordinary Copy shortcut and native save. Strict export still refuses unsupported
content, and a failed Copy as operation leaves the clipboard unchanged. The
footer reserves its height before delayed progress appears, with a second row
below 1280 logical pixels wide.

![Native Copy as refusal names MeO and exposes the complete Command-C, native-save and unchanged-clipboard advice](images/qa-export-feedback/after-full-message.jpg)

[Matched native images, input fixture and combined footer checks](changes/qa-export-feedback.md)
record the macOS menu/hover interaction and real-renderer layout results.
Windows/Linux native interaction and a new ChemDraw round trip are not claimed.

See [ReShiki 0.13.0](changes-0.13.md) for the latest release notes.
