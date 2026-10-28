# Unreleased changes

**Python package — under review in [PR #611](https://github.com/Ameyanagi/ReShiki/pull/611).** Draw, convert and analyze structures
from scripts, Jupyter, pandas and Excel with the same headless Rust engine as the
editor. Immutable objects support chained cleanup and styles; parallel batches
report individual row errors. [Issue #608](https://github.com/Ameyanagi/ReShiki/issues/608),
[Python guide](python.md). PyPI publishing, six-platform release wheels, EMF and
editable Office pictures are follow-ups.

Under review: @Ameyanagi — browser canvas touch drawing, pinch zoom, two-finger
pan and long-press context menus, plus a canvas label and status announcements
for screen readers. [Issue #435](https://github.com/Ameyanagi/ReShiki/issues/435),
[PR #610](https://github.com/Ameyanagi/ReShiki/pull/610).

Under review: @Ameyanagi — Browser Save updates the opened file in Chromium,
and Save As offers a file name and location. Download fallbacks keep the edited
marker; opening the same file handle returns to its tab. Remaining browser file
work for [issue #428](https://github.com/Ameyanagi/ReShiki/issues/428), [PR #612](https://github.com/Ameyanagi/ReShiki/pull/612).

Under review: @Ameyanagi — imported drawings show their source filename in tabs
and the window title while Save still asks for a native drawing destination
([issue #601](https://github.com/Ameyanagi/ReShiki/issues/601), [PR #606](https://github.com/Ameyanagi/ReShiki/pull/606)).

Under review: @Ameyanagi — Inline reaction-condition editors open away from the
arrow, keeping the shaft and neighbouring structure visible where space permits.
[Issue #602](https://github.com/Ameyanagi/ReShiki/issues/602); [PR #607](https://github.com/Ameyanagi/ReShiki/pull/607).

Under review: @Ameyanagi — Linux desktop launchers can associate ReShiki windows
with their launcher and icon in [PR #598](https://github.com/Ameyanagi/ReShiki/pull/598).
Reported by @fusion809 in [issue #597](https://github.com/Ameyanagi/ReShiki/issues/597).
[Metadata checks and limits](changes/linux-window-identity.md).

**Adjustable reaction-label spacing — under review.** Set attached conditions to
an exact gap in the current ruler unit, or drag perpendicular to the arrow while
keeping the label attached. Selecting several labels applies one shared gap in
one Undo step. Visible gap dimensions help align labels and stay out of exports.
Custom gaps persist in native `.rsk` version 31; Auto-only drawings keep version 30.
@Ameyanagi — [PR #603](https://github.com/Ameyanagi/ReShiki/pull/603),
[user guide](arrows.md#adjust-label-spacing) and
[native QA evidence](changes/arrow-label-spacing.md).

![Two attached reaction-condition labels share a 4.0 mm gap, with on-canvas dimensions and a matching Properties value.](images/arrow-label-spacing/equal-gaps.png)

The [0.13.1 release](https://github.com/Ameyanagi/ReShiki/releases/tag/v0.13.1)
was published on 26 October 2026. It includes the October dependency and CI runtime
security updates merged in [PR #586](https://github.com/Ameyanagi/ReShiki/pull/586),
tracked in [issue #585](https://github.com/Ameyanagi/ReShiki/issues/585).
@Ameyanagi — [security review and remaining scoped findings](changes/security-dependency-audit.md).

Merged for 0.13.1: @Ameyanagi —
[reaction reference coverage](https://github.com/Ameyanagi/ReShiki/pull/589),
[large-selection Commands](https://github.com/Ameyanagi/ReShiki/pull/590),
[portable synthetic-paste checks](https://github.com/Ameyanagi/ReShiki/pull/591)
and [large-selection PDF feedback](https://github.com/Ameyanagi/ReShiki/pull/592).
The versioned notes retain immutable QA links and qualifications.

See [ReShiki 0.13.1](changes-0.13.1.md) for the published release notes and
[release validation](release-0.13.1-validation.md) for package and web verification.

Under review: [PR #599](https://github.com/Ameyanagi/ReShiki/pull/599) adds a
general PubChem name lookup with confirmation for each query. Review its source
and editable preview before inserting; local naming remains offline. Implemented
by @Ameyanagi for [issue #596](https://github.com/Ameyanagi/ReShiki/issues/596),
reported by @fusion809. [Native and browser examples](changes/online-name-lookup.md).
