# Unreleased changes

## Fixes

Artemisinin imports with separated peroxide oxygen labels and can be cleaned up
without changing its stereochemistry. Whole-molecule cleanup refusals now say
that the drawing is unchanged instead of asking for an already complete selection.
@Ameyanagi, [PR #538](https://github.com/Ameyanagi/ReShiki/pull/538), under review.
[Matched renderer review](changes/artemisinin-depiction.md).

| Before                                                                                          | After                                                                                        |
| ----------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| ![Artemisinin with overlapping peroxide oxygen labels](images/artemisinin-depiction/before.png) | ![Artemisinin with separated peroxide oxygen labels](images/artemisinin-depiction/after.png) |

See [ReShiki 0.13.0](changes-0.13.md) for the latest release notes.
