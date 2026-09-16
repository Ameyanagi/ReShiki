# Moruno

A molecular drawing workspace built with Rust and Iced, with a local Python/RDKit chemistry worker.

Version 0.2 expands everyday editing, recovery and export. It is not yet a complete professional chemistry suite. The [capability report](docs/capabilities.md) separates tested workflows, partial support, and planned features.

## Run

Requires a current Rust toolchain and [uv](https://docs.astral.sh/uv/). Tested on Apple Silicon macOS with Rust 1.95, Iced 0.14, Python 3.12, and RDKit 2026.3.6. Other operating systems have not been tested.

```sh
cd ~/dev/moruno
uv sync --locked
cargo run --locked
```

Build a macOS app bundle:

```sh
python3 scripts/build_macos_app.py
open target/debug/Moruno.app

# Includes Python and RDKit; can be moved outside the checkout
python3 scripts/build_macos_app.py --standalone
open dist/Moruno.app
```

The default development bundle uses this checkout's `engine/worker.py` and `.venv`. The `--standalone` bundle includes its chemistry worker and needs neither the checkout nor an installed Python. Add `--release` for an optimized Rust build. The bundle is signed ad hoc for local use; it is not a notarized release. `MORUNO_ROOT` overrides the development worker directory; `MORUNO_PYTHON` explicitly selects an external Python even in a standalone bundle. All molecule processing runs locally, without a chemistry service account.

## Use

- Drag with a bond tool to draw. New bonds snap to 30° angles and a consistent length; release over an existing atom to connect. Click a bond's middle to change its order or style.
- Choose an element and click an atom to replace it, or empty space to place an atom. The palette contains common elements; the symbol field accepts all 118 elements.
- Choose a 3–8 member ring, enable aromatic bonds, click a bond to fuse a ring, or click an atom to share a vertex.
- Choose an arrow style, or enter annotation text (`\n` for new lines) and place a label. Select a label and use Update selected labels to edit it.
- Copy/cut/paste and duplicate selected drawing objects. Paste SMILES, InChI, MOL or supported CDXML to insert a molecule. Templates insert into the current document.
- Scroll the left panel for rotate, flip, align, distribute, and bond-direction controls. Double-click an atom to select its molecule.
- Select/move drags atoms or objects. Drag empty space for a rectangular selection. Scroll the left panel for charge, isotope, and deletion controls.
- Check structure validates the graph and refreshes formula, mass, descriptors, implicit hydrogen labels, and canonical SMILES. Clean up regenerates 2D coordinates.
- The SMILES field and example buttons replace the drawing; Undo restores it.
- Save/open `.moruno` retains the complete editable drawing. The Open dialog also accepts MOL, SMILES (`.smi`/`.smiles`), InChI, and supported CDXML drawings.
- Export SVG or PDF for a vector drawing, PNG at 300 dpi, MOL/SMILES/InChI for molecular data, or CDXML for basic drawing interchange. MOL/SMILES/InChI cannot store annotation text or arrows.

| Shortcut | Action |
| --- | --- |
| Cmd/Ctrl+Z; Cmd/Ctrl+Shift+Z | Undo; redo |
| Cmd/Ctrl+A | Select all drawing objects |
| Cmd/Ctrl+C / X / V / D | Copy / cut / paste / duplicate |
| Cmd/Ctrl+Shift+S | Save as |
| Delete/Backspace | Delete selection |
| Cmd/Ctrl+S / O / N | Save / open / new |
| Escape | Select tool |
| Wheel | Zoom around the pointer |
| Right or middle drag | Pan |

Text fields handle their own editing shortcuts. Use the window close button for the unsaved-changes prompt; interrupted sessions can be restored from the five-second recovery snapshots. Recovery files live in the platform application-data directory (override with `MORUNO_DATA_DIR`). Restoration makes an unsaved copy.

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

See [0.2 changes](docs/changes-0.2.md) and the [ruviz integration requirements](docs/ruviz-integration.md).
