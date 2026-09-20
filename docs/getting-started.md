# Getting started

Moruno is a desktop workspace for editable chemical drawings. Molecule processing runs locally with RDKit. The optional assistant uses your Codex connection when you send a request.

## Install

Download a package for your computer from [GitHub Releases](https://github.com/Ameyanagi/moruno/releases). If no release is listed yet, use the source instructions below. The release workflow builds these packages:

| Computer          | Package            | Open         |
| ----------------- | ------------------ | ------------ |
| Apple Silicon Mac | `macos-arm64.zip`  | `Moruno.app` |
| Intel Mac         | `macos-x64.zip`    | `Moruno.app` |
| Windows x64       | `windows-x64.zip`  | `moruno.exe` |
| Linux x64         | `linux-x64.tar.gz` | `./moruno`   |

Extract the archive before opening the app, and keep its contents together. On macOS, move the included app to Applications. Each package includes Python and RDKit; a separate Python installation is unnecessary. macOS release builds target macOS 14 or later. Linux packages are built on Ubuntu 22.04 and need a desktop session with compatible system graphics libraries. Windows packages target Windows 10/11 x64.

Tagged macOS releases require Developer ID signing and Apple notarization. Windows and Linux packages do not currently have publisher signatures. Manual workflow builds may be unsigned and are intended for testing. Packaging smoke tests check the bundled chemistry engine; they do not establish feature parity or full graphical compatibility on every operating system. Native clipboard and printing features currently have macOS-specific support.

## Draw your first molecule

1. Choose the single-bond tool, then click the canvas to begin a carbon chain.
2. Click an endpoint to extend it, or drag to choose a direction.
3. Hover an atom and press **O** or **N** to change its element.
4. Use **Templates** to preview and attach a ring or another fragment.
5. Select a molecule before using **Clean up**. Review the preview and apply it when ready.
6. Save as `.moruno` to retain the complete editable drawing.

New documents use JACS / ACS defaults: 10 pt Arial labels, 14.4 pt bonds, and 0.6 pt lines. Use Undo to reverse drawing edits. See [bond tools](bond-tools.md), [templates](template-library.md), and [keyboard shortcuts](contextual-shortcuts.md).

## Export and share

Use SVG or PDF for vector figures and PNG for an image. Editable copy and CDXML exchange preserve supported drawing objects; see [clipboard support](clipboard.md) for platform and format limits. Use [publication pages](publication-pages.md) to prepare page layouts.

## Run from source

Install Rust 1.95 or later and [uv](https://docs.astral.sh/uv/), then:

```sh
git clone https://github.com/Ameyanagi/moruno.git
cd moruno
uv sync --locked --python 3.12
cargo run --locked
```

For development checks, hooks, and documentation previews, see [development](development.md).
