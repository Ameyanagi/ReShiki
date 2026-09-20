# Style toolbar and editable typography

Implemented and desktop-tested on 2026-09-20. The workflow and remaining limitations are described below.

## Editing workflow

- The persistent Style row offers searchable installed font families, point size, bold/italic/underline, chemical-formula formatting, subscript/superscript, paragraph alignment, five color swatches and a custom RGB hex field.
- Choose Text and click the canvas to start typing. Click an existing label with Text, or double-click it with Select, to edit it in place. Select part of its text to format that range; with no text range selected, formatting applies to the whole draft. Done, Cmd/Ctrl+Enter, or a click outside the canvas editor applies the draft as one document Undo step. Escape cancels it. Undo/Redo inside the editor tracks text and formatting without changing the document history.
- Selecting a label reveals the text editor at the top of the inspector. Switching inspector tabs also resets the scroll position so controls do not remain hidden below an unrelated panel's previous scroll offset.
- Cmd/Ctrl+B, I and U format the selected text while the editor has focus. Font size, custom color and wrap width apply with Enter. Paragraph controls include left/center/right/justified alignment, line spacing and wrapping width in points.
- Selected atom labels accept font, size, emphasis and color. Their hydrogen counts, isotope and charge scripts remain derived from chemical data.
- The palette has an explicit **Color** scope: **All selected** (default), **Text**, or **Bonds**. All selected recolors selected atom labels, bonds, captions, arrows, graphics and attached number/stereo labels in one Undo step. Text limits the change to labels; Bonds limits it to bond strokes. Selected text ranges retain range-only formatting. The custom hex field uses the same scope.
- Click the middle of a bond to select it and both end atoms; Shift-click adds to the selection. Cmd/Ctrl+A selects the drawing, including its bonds. The selection summary includes the number of selected bonds.
- A mixed-color selection has no active palette swatch and shows an empty hex field until a color is chosen. New resets the scope to All selected and restores the JACS defaults.
- Native format version 3 introduced text defaults and UTF-8 style ranges (retained in version 4); version 1/2 documents still open with default Arial typography. Editing, copying, cleanup, recovery and Undo/Redo retain supported styles.

The canvas, selection bounds, SVG, PDF and PNG share the rich-text layout. Chemistry cleanup preserves presentation fields by stable atom IDs. The CDXML exporter receives font measurements from the Rust renderer to position baseline anchors and centered/right-aligned captions.

## Computer-use verification

Used a separate QA app so existing drawings in older Moruno windows remained open. Through actual desktop controls:

1. Entered `Pd/C, H2` and `EtOH · 25 °C` on separate lines and placed the label.
2. Enabled formula formatting; `H2` acquired a subscript.
3. Selected only `Pd/C` in the editor; applied bold and blue. The remainder stayed black and regular.
4. Saved, restarted the QA app and reopened the native document. Both the text and its formatting survived.
5. Searched for Helvetica in the font field, applied 8 pt, moved the label, centered the paragraph and used Cmd+I on only the second line.
6. Opened the exported CDXML in an external drawing application and saved it there. The resulting fixture is retained under `tests/fixtures/`;  its style runs exposed reserved-color and baseline-origin issues which now have regression tests.
7. Exported the final centered Helvetica label through Moruno's native Save dialog. Opened and magnified it in an external drawing application: blue bold `Pd/C`, subscript `2`, centered lines and italic second line were all visible.

Local screenshots and native files are under ignored `artifacts/style-qa-20260920/`. The label accompanies an example molecule for visual testing; its caption is not a proposed reaction for that molecule.

Validation: **51 Rust tests, 15 Python tests**, formatting and Clippy with warnings denied. Tests cover UTF-8 boundaries, repeated-character insertion/deletion/replacement, partial styling, Undo/Redo, formula scripts, paragraph wrapping, native compatibility, colored PNG pixels, SVG/PDF, chemistry-preserving atom typography, CDXML text-only documents and measured caption placement.

Development and standalone bundles were rebuilt. The standalone bundle's signature and chemistry check passed; its bundled worker also cleaned the saved rich-text document and exported styled CDXML from `/tmp`, independently of the checkout's worker.

## Remaining limits

- The canvas editor displays fonts, bold/italic and colors with a caret. Formula scripts, mixed sizes, underline, paragraph alignment and explicit wrapping are shown in a separate Appearance preview; the caret editor is not fully WYSIWYG for those properties. The optional inspector editor remains plain text.
- Formula formatting is a heuristic for common formulas. Use explicit superscript for ambiguous charge notation such as the `3+` in `Fe3+`.
- There is no character picker, list/tab-stop editor, arbitrary text rotation, font-outline/shadow effect, or separate document-wide font-default editor.
- Font availability and paragraph metrics can differ between applications. CDXML supports the implemented style runs and common paragraph properties; it is not a pixel-identical round trip for every external text object. Unsupported outline/shadow, rotation and incompatible paragraph settings are rejected on import. Supported graphics and groups have separate workflows; multipage CDXML remains unsupported.
- Automatic color by element and a spectrum/wheel picker remain unavailable. Existing filled graphics receive the chosen color in All selected; use their separate stroke/fill controls to change only one.

Interchange follows the published CDX/CDXML descriptions of [style runs](https://bobhanson.github.io/IUPAC-FAIRSpec/cdx_sdk/Style.htm), [color indexes](https://bobhanson.github.io/IUPAC-FAIRSpec/cdx_sdk/ColorTable.htm), [text properties](https://bobhanson.github.io/IUPAC-FAIRSpec/cdx_sdk/Text.htm) and [font face flags](https://bobhanson.github.io/IUPAC-FAIRSpec/cdx_sdk/DataType/CDXString.htm), checked against the installed application's output.

## Selection-color verification

Desktop checks on 2026-09-20 used a separate QA window: Select All plus blue colored an entire molecule; Bonds plus red left the labels blue; Text plus teal left the bonds red; Undo/Redo preserved each step. The saved drawing retains the separate colors. Regression tests cover selection scope, shared-atom boundaries, arrows, filled graphics, attached indicators, text-range formatting and atomic Undo/Redo. Strict Clippy checks retain the runtime restrictions against unsafe code and unchecked unwrap/panic paths.

## Japanese text

The interface explicitly chooses an installed Japanese sans-serif font (Hiragino Sans on macOS). Drawing styles retain their selected family and JACS/ACS defaults. When a glyph is absent from that font, shared layout resolves a sans-serif fallback and uses its actual advances for positioning and wrapping; canvas, SVG, PDF and PNG receive the same resolved text runs. This avoids half-width measurements for full-width Japanese characters.

## Canvas editing verification

Canvas drafts are separate from the document until applied. File and tool commands finish the draft first, recovery snapshots include it, and cancellation keeps unrelated edits. A changed/deleted label or switched document rejects a stale draft. Background assistant proposals wait until text editing finishes; text and assistant changes retain separate Undo steps.

Regression checks cover click-to-type, Unicode ranges and font highlighting, draft Undo/Redo, cancellation, recovery, grouped-label deletion, independent and conflicting edits, and double-click versus drag behavior. Desktop checks in the isolated `artifacts/inline-text-qa/` window exercised double-click editing, multiline typing, Japanese-label bold/color changes, Escape cancellation, atomic Undo, formula preview, Cmd+Enter after toolbar use, and native Save. The editor stays within the viewport; applying pans just enough to reveal the finished label without changing zoom. Native Japanese IME composition has not been separately verified.
