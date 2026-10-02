# Unreleased changes

See [ReShiki 0.10.0](changes-0.10.md) for the latest release notes.

- **Under review — Handle shortcuts for precise transforms:** double-click a
  rotation, corner or edge handle to open the matching numeric field, ready to
  type. Opening a field leaves the drawing and Undo history unchanged.
  [Details](selection-transforms.md) · @Ameyanagi.

- **Under review — Stable selection rotation:** repeated rotations no longer
  shift asymmetric molecules such as Pyrrole. Keyboard, numeric and handle
  rotations share a stable center, including mixed drawing selections.
  [Details](selection-transforms.md) · [Issue #96](https://github.com/Ameyanagi/ReShiki/issues/96)
  · @Ameyanagi.
