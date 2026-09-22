# Getting started

ReShiki is a desktop workspace for editable chemical drawings. Molecule processing runs locally. The optional assistant uses your Codex connection when you send a request.

## Install

The offline setup below applies to 0.6.0 builds. For version 0.5.0, follow its [release notes](changes-0.5.md), including the first-launch uv requirement.

Download a package for your computer from [GitHub Releases](https://github.com/Ameyanagi/ReShiki/releases). The release workflow builds these packages:

| Computer          | Package                   | Open                 |
| ----------------- | ------------------------- | -------------------- |
| Apple Silicon Mac | `macos-arm64.dmg`         | Drag to Applications |
| Windows x64       | `windows-x64-setup.exe`   | Run setup            |
| Windows ARM       | `windows-arm64-setup.exe` | Run setup            |
| Linux x64         | `linux-x64.tar.gz`        | `./reshiki`          |
| Linux ARM         | `linux-arm64.tar.gz`      | `./reshiki`          |

Drawing and chemistry tools are included and work offline from the first launch. Portable ZIP downloads are also available; extract them and keep their contents together.

Mac requires Apple Silicon and macOS 14+. Windows x64 supports Windows 10/11; Windows ARM requires Windows 11. Linux packages target Ubuntu 22.04+ on x64 and 24.04+ on ARM, with a desktop session and compatible graphics libraries.

Tagged macOS releases are signed and notarized. Windows and Linux packages are unsigned. See the [visual installation guide](/guide/install/) for screenshots and updates, or [development](development.md) to build from source.

## Draw your first molecule

1. Choose the single-bond tool, then click the canvas to begin a carbon chain.
2. Click an endpoint to extend it, or drag to choose a direction.
3. Hover an atom and press **O** or **N** to change its element.
4. Use **Templates** to preview and attach a ring or another fragment.
5. Select a molecule before using **Clean up**. Review the preview and apply it when ready.
6. Save as `.reshiki` to retain the complete editable drawing.

New documents use JACS / ACS defaults: 10 pt Arial labels, 14.4 pt bonds, and 0.6 pt lines. Use Undo to reverse drawing edits. See [bond tools](bond-tools.md), [templates](template-library.md), and [keyboard shortcuts](contextual-shortcuts.md).

## Export and share

Use SVG or PDF for vector figures and PNG for an image. Editable copy and CDXML exchange preserve supported drawing objects; see [clipboard support](clipboard.md) for platform and format limits. Use [publication pages](publication-pages.md) to prepare page layouts.

## Run from source

Follow the [development guide](development.md) to build the app and its native helper, run checks, or preview the documentation.
