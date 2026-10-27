# Unreleased changes

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
