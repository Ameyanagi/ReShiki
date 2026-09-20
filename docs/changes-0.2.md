# Moruno 0.2.0

The first public release includes editable molecular drawings, reaction schemes, and a [visual manual](https://ameyanagi.github.io/moruno/).

- Draw atoms, bonds, chains, and rings with JACS / ACS defaults.
- Attach templates through a chosen atom or bond; save reusable fragments.
- Arrange captions, bent arrows, shapes, orbitals, and pictures. Clean up only the selected structures.
- Ask the assistant for a drawing, then review it or accept edits automatically. Applied changes remain undoable.
- Save editable documents; copy structures and images; export SVG, PDF, PNG, and chemical data.

## Downloads

Install [uv](https://docs.astral.sh/uv/getting-started/installation/) first. Moruno installs its chemistry tools locally on first use.

| Platform      | Architecture                                            |
| ------------- | ------------------------------------------------------- |
| macOS 14+     | Apple Silicon; signed and notarized                     |
| Windows 10/11 | x64; unsigned                                           |
| Windows 11    | ARM64 app with an x64 chemistry worker; unsigned        |
| Linux         | x64 (Ubuntu 22.04+) and ARM64 (Ubuntu 24.04+); unsigned |

Intel Macs are not supported. Native system clipboard exchange and printing currently target macOS; Windows and Linux use file exports for these workflows.

All five packages passed extracted-archive startup, fresh chemistry setup, and offline reuse checks. Desktop workflow checks used macOS. Moruno remains under active development; review generated chemistry before using a figure.
