# Roadmap

This page lists the larger open directions after ReShiki 0.15.0, taken from
the [open GitHub issues](https://github.com/Ameyanagi/ReShiki/issues). It is not
a release commitment. See [feature status](feature-status.md) for what works
today and the [latest release notes](changes-0.15.md) for what changed.

JACS / ACS stays the default. New features keep physical drawing sizes,
complete molecular graphs and editable originals.

- **Browser editor:** make the WebAssembly build a full editor
  ([#436](https://github.com/Ameyanagi/ReShiki/issues/436)), with heavy work off
  the main thread, NMR and names
  ([#430](https://github.com/Ameyanagi/ReShiki/issues/430)), an embedding API
  ([#431](https://github.com/Ameyanagi/ReShiki/issues/431)) and a faster demo
  page ([#433](https://github.com/Ameyanagi/ReShiki/issues/433)).
- **Office and other apps:** reopen ReShiki pictures as editable drawings in
  Office and other apps, and paste several items into one tab
  ([#462](https://github.com/Ameyanagi/ReShiki/issues/462)); finish the remaining
  Wayland, Windows focus and Office checks
  ([#1118](https://github.com/Ameyanagi/ReShiki/issues/1118)).
- **Chemistry files:** open computational chemistry input and output files
  ([#418](https://github.com/Ameyanagi/ReShiki/issues/418)) and crystal and
  periodic structures with their unit cell
  ([#417](https://github.com/Ameyanagi/ReShiki/issues/417)); import more inputs
  with a warning instead of refusing them
  ([#1138](https://github.com/Ameyanagi/ReShiki/issues/1138)).
- **3D and layout:** 3D model objects beside structures
  ([#386](https://github.com/Ameyanagi/ReShiki/issues/386)), relaxed 3D
  optimization for metal complexes
  ([#371](https://github.com/Ameyanagi/ReShiki/issues/371)) and a CoordGen
  layout for macrocycles in **Clean up**
  ([#370](https://github.com/Ameyanagi/ReShiki/issues/370)).
- **Distribution:** signed Windows builds through SignPath
  ([#489](https://github.com/Ameyanagi/ReShiki/issues/489)), a Homebrew cask
  ([#368](https://github.com/Ameyanagi/ReShiki/issues/368)) and a translated
  interface ([#408](https://github.com/Ameyanagi/ReShiki/issues/408)).

Bugs are tracked as issues with the
[`bug` label](https://github.com/Ameyanagi/ReShiki/issues?q=is%3Aopen+label%3Abug).
