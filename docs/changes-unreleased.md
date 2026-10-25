# Unreleased changes

Under review: [PR #579](https://github.com/Ameyanagi/ReShiki/pull/579) aligns
opening and reset 3D model faces with their drawings, keeps chemical-name
captions attached during model resizing, and shows rotation instructions before
the first drag. Proper rotations preserve chemical stereochemistry; saved views
and crystal cell orientation retain their existing behavior.

![Caffeine opens with the model facing the same direction as its drawing](images/qa-model-face-handles/head-caffeine-both.png)

![The actual browser editor keeps a caption attached during a held model resize](images/qa-model-face-handles/webkit-resize-held.png)

[Matched comparisons, inputs and native/browser checks](changes/qa-model-face-handles.md)
cover the visible changes and the chemical/native round trips.

See [ReShiki 0.13.0](changes-0.13.md) for the latest release notes.
