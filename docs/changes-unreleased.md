# Unreleased changes

## Fixes

Space-filling models keep hydrogens facing the viewer visible where overlapping
atoms meet. The front cap no longer leaves a hole through its carbon.
@Ameyanagi, [PR #535](https://github.com/Ameyanagi/ReShiki/pull/535), merged.
[Matched renderer review](changes/qa-513.md).

| Before                                                                                 | After                                                                             |
| -------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| ![A front-facing hydrogen leaves a hole through carbon](images/qa-513/before-dark.png) | ![The visible hydrogen cap fills the carbon cutout](images/qa-513/after-dark.png) |

PDF export keeps 3D model and crystal shading aligned at publication size.
@Ameyanagi, [PR #536](https://github.com/Ameyanagi/ReShiki/pull/536), under review.
[Matched export review and reproduction](changes/qa-522.md).

![Matched PDF exports before and after the fix: Flat and Glossy molecular models and a sodium chloride unit cell retain their shading](images/qa-522/pdf-gradient-before-after.png)

Crystal files passed to the CLI now produce a clear unsupported-format message
instead of a SMILES parse error. Rejection happens before input is read or output
is created. @Ameyanagi, [PR #537](https://github.com/Ameyanagi/ReShiki/pull/537),
under review. [Command examples and validation](changes/qa-490.md).

Under review: @Ameyanagi — long polystyrene and alanine chains import with
separated rings and chemical labels while retaining molecular identity and
stereochemistry in [PR #539](https://github.com/Ameyanagi/ReShiki/pull/539).
[Matched renderer comparisons, inputs, and bounded fallback behavior](changes/long-chain-layout.md).
Import latency is tracked separately in [#508](https://github.com/Ameyanagi/ReShiki/issues/508).

![Poly-L-alanine with separated labels and retained stereochemical wedges](images/long-chain-layout/stereo-alanine200-after-detail.png)

Under review: @Ameyanagi — drag the visible frame edge to move a selected 3D
model while keeping its view and selection in
[PR #540](https://github.com/Ameyanagi/ReShiki/pull/540).
[Matched native desktop evidence and editable fixture](changes/qa-514.md).

![Methanol moves while retaining its selected frame and view](images/qa-514/after-card.png)

Under review: @Ameyanagi in [PR #541](https://github.com/Ameyanagi/ReShiki/pull/541).

Safari pastes clipboard text into captions and Import fields without turning Command-V into a typed v.

[Evidence, comparisons, and reusable release material](changes/qa-521.md).

See [ReShiki 0.13.0](changes-0.13.md) for the latest release notes.
