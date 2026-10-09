# Original local naming desktop evidence

All 11 `.rsk` files are original native **Save As** outputs from the actual
macOS arm64 desktop review on 2026-10-10 Japan time (2026-10-09 UTC). No native
file or screenshot was rewritten. Application source:
`aaa6f9b791d0c256603e83ee96f0c232dfdf2c00`; signed executable SHA256:
`1c7d39e208a86f6cab7c4db9da9b48e2b28438f363cbe206f07a01b4ca6f61f1`.
Later workflow-only changes do not alter that app. This folder is controlled
chemical test data, not a third-party drawing collection.

| Original native saves | Expected state and exact raw equality |
| --- | --- |
| `ethanol-empty-before-insert.rsk`, `ethanol-insert-undo.rsk` | Both byte-identical empty drawings. One Undo removes the complete insertion. |
| `ethanol-inserted.rsk`, `ethanol-insert-redo.rsk`, `ethanol-caption-undo.rsk` | All byte-identical ethanol, three atoms/two bonds, no caption. Insertion Redo and caption Undo preserve every graph/layout field. |
| `ethanol-caption.rsk`, `ethanol-caption-redo.rsk`, `ethanol-caption-fresh-reopen.rsk` | All byte-identical. Exactly one `ethan-1-ol` annotation is added; every other native field equals inserted ethanol. |
| `r-lactic-inserted.rsk`, `r-lactic-after-rejected-name.rsk`, `r-lactic-fresh-reopen.rsk` | All byte-identical R-lactic acid, six atoms/five bonds. Optical-rotation-only rejection and fresh reopening preserve specified stereo and the full graph. |

[provenance.json](provenance.json) records all 23 original byte hashes, the 12
raw JPEG paths, capture conditions, app/kernel/source/runtime provenance,
manual scope and remaining limits. [own-compiler-fresh-proof.json](own-compiler-fresh-proof.json)
is the original receipt: all 19 own compiler artifacts across 16 packages are
`fresh:false`. The historical baseline has kernel/path/hash proof; an original
compiler correlation to the declared PR base is explicitly unverified.

Run the portable standard-library evidence verifier from the repository root:

```sh
python3 scripts/verify_chemical_naming_desktop.py
```

It checks every original hash, native version19 and label settings, four raw
equality groups, expected connectivity/neutral formula and R stereo, and the
single-caption-only delta. This verifies saved evidence; it does not launch GUI
or a naming engine. Python is a maintainer tool, not an app runtime prerequisite.

The separate [independent QA receipt](independent-desktop-qa.md) and its
[full JSON](independent-desktop-qa.json) record exact signed-app network-denied
native CLI analysis and independent RDKit graph/stereo reconstruction of every
nonempty save, ignoring cached CIP/H labels. That read-only audit did not replay
GUI actions or click Cancel. RDKit is not redistributed or used by the app.

See the [current local desktop review](../../../../docs/chemical-naming-local-visual-review.md)
for all original images, reproduction steps, zoom differences and test scope.
The files in the parent folder belong to the
[superseded HTTP prototype](../../../../docs/chemical-naming-visual-review.md).
