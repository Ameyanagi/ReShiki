# Moruno

A molecular drawing workspace built with Rust and Iced, with a local Python/RDKit chemistry worker.

This is a working first implementation, not a complete professional chemistry suite. The [capability report](docs/capabilities.md) separates tested workflows, partial support, and planned features.

## Run

Requires a current Rust toolchain and [uv](https://docs.astral.sh/uv/). Tested on Apple Silicon macOS with Rust 1.95, Iced 0.14, Python 3.12, and RDKit 2026.3.6. Other operating systems have not been tested.

```sh
cd ~/dev/moruno
uv sync --locked
cargo run --locked
```

Build a development macOS app bundle:

```sh
python3 scripts/build_macos_app.py
open target/debug/Moruno.app
```

The development bundle uses this checkout's `engine/worker.py` and `.venv`; it is not a standalone distributable. `MORUNO_ROOT` overrides the worker directory and `MORUNO_PYTHON` overrides the Python executable. All molecule processing runs locally, without a chemistry service account.

## Use

- Drag with a bond tool to draw. New bonds snap to 30° angles and a consistent length; release over an existing atom to connect. Click a bond's middle to change its order or style.
- Choose an element and click an atom to replace it, or empty space to place an atom. The palette contains common elements; imported molecules can contain others.
- Place a six-membered ring, draw an arrow, or enter annotation text and place a label.
- Select/move drags atoms or objects. Drag empty space for a rectangular selection. Scroll the left panel for charge, isotope, and deletion controls.
- Check structure validates the graph and refreshes formula, mass, descriptors, implicit hydrogen labels, and canonical SMILES. Clean up regenerates 2D coordinates.
- The SMILES field and example buttons replace the drawing; Undo restores it.
- Save/open `.moruno` retains the complete editable drawing. The Open dialog also accepts MOL, SMILES (`.smi`/`.smiles`), and molecular CDXML.
- Export SVG for a vector drawing, MOL/SMILES/InChI for molecular data, or CDXML for basic drawing interchange. MOL/SMILES/InChI cannot store annotation text or arrows.

| Shortcut | Action |
| --- | --- |
| Cmd/Ctrl+Z; Cmd/Ctrl+Shift+Z | Undo; redo |
| Cmd/Ctrl+A | Select all drawing objects |
| Delete/Backspace | Delete selection |
| Cmd/Ctrl+S / O / N | Save / open / new |
| Escape | Select tool |
| Wheel | Zoom around the pointer |
| Right or middle drag | Pan |

Text fields handle their own editing shortcuts. Use the window close button for the unsaved-changes prompt; force quit and OS-level termination do not provide crash recovery.

## Verification

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
uv run --locked python -m unittest discover -s tests -p 'test_*.py'
cargo run --locked -- --engine-check
```

Tests cover molecular identity, tetrahedral and E/Z stereo, stable atom IDs, cleanup, error recovery, file exchange, history, rapid pointer events, and stale asynchronous results. Desktop tests also exercised freehand drawing, validation, cleanup, movement, annotations, save/reopen, undo/redo, and cross-application XML exchange. See the [test record](docs/capabilities.md#desktop-test-record).

## Design

The Rust document and drawing scene are independent of RDKit. Chemistry requests pass through a versioned protocol and `ChemistryEngine` interface so a Rust backend can replace Python in stages. See [architecture](docs/architecture.md), [framework research](docs/framework-research.md), and [roadmap](docs/roadmap.md).
