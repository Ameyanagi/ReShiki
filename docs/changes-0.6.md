# ReShiki 0.6.0

ReShiki 0.6 opens on a blank canvas and includes its drawing and chemistry tools. **No Python, RDKit, or uv installation is required.** Imports, molecular properties, identifiers, stereochemistry, cleanup, and 2D layout run locally. Installers and portable packages work offline from the first launch. The optional assistant still needs a Codex connection and internet access.

## Drawing and editing

- Single, double, and triple bonds select immediately. Other bond styles have a separate palette. Straight and snaking chains have separate buttons.
- Rings, arrows, shapes, brackets, and other tools with choices reuse the selected preset on a click. Hold the button or click its corner triangle for more choices.
- Click the canvas to place a fixed-length rightward arrow, or drag to choose its size and direction. Clicking an existing arrow changes its style; repeating the same style cycles its direction or variants.
- Wavy bonds, filled half-head arrows, equilibrium arrows, and curved dashed shape borders have improved rendering.
- Drag the eraser across objects to remove everything along its path. One Undo restores the stroke.
- Align selected molecules, arrows, and groups together. Right-click opens relevant editing and arrangement commands.
- Import examples insert into the current drawing and remain selected for placement or removal.
- Properties groups controls by the selection. Molecular calculations follow the selected fragment; artwork alone has no molecular properties. Export separates figures, chemical exchange, and printing.
- Skeletal carbons can display a charge without forcing a carbon label. Shortcuts are available from a labeled toolbar button.

## Assistant schemes

Sending a request shows activity, elapsed time, and Stop immediately. Public composition summaries, actual structure counts, and editable previews appear as work becomes available. The canvas remains usable, and closing the assistant panel lets generation continue.

Schemes support aligned reaction rows, a central general reaction with surrounding examples, labeled grids, and branching reactions with one shared reactant. Layout uses measured bounds and consistent bond lengths. Compact chains retain the complete graph and can be expanded. Conventional molecular orientations are straightened before review.

The assistant reviews overview and close-up images of the exact proposed drawing, then makes a bounded set of editable corrections. Unresolved findings stay visible. Accept-all applies only a final draft that passes review; stopped requests retain completed previews. Application remains one Undo step and protects edits made while generation runs.

## Installation and compatibility

Downloads support Apple Silicon macOS, Windows x64/ARM64, and Linux x64/ARM64. Existing `.reshiki` drawings remain compatible. Keep portable package contents together so the application can find its bundled native helper. Python remains a developer build-script and optional reference-test dependency only.

See the [installation guide](/guide/install/), [assistant walkthrough](/guide/assistant/), and [release preparation record](release-0.6-validation.md).
