# Document drawing styles

Click the style selector beside the drawing controls to switch directly between journal presets. Choose **Drawing style…** at the bottom of that menu, or **Properties → Edit drawing style…**, to open the detailed editor. New documents always start with **JACS / ACS**. Changing one document does not change the defaults for another.

Choose **JACS / ACS**, **Nature**, **RSC**, **Angewandte**, **SYNLETT / SYNTHESIS**, or **Custom**. Each journal choice follows publisher instructions or an official download and has a **Publisher instructions ↗** link in the panel. See [journal settings and publisher sources](journal-drawing-presets.md).

Direct journal selection applies the preset immediately, scales existing geometry with its bond length, and updates matching fonts and strokes in one Undo step. **Custom** opens the editor with the current settings.

Custom keeps the current draft’s dimensions so you can edit and save a reusable style. The panel previews a sample structure while the document stays unchanged. **Apply to document** commits one Undo step; **Cancel** discards the draft. Apply and Cancel remain visible while the settings scroll.

## Settings

The main controls set the label font, label size, nominal bond length, and line width. **Advanced stroke settings** exposes bold/wedge width, label clearance, hash spacing, and multiple-bond spacing as a percentage of nominal bond length. Dimensions are publication points, independent of screen zoom. [PNG file export](figure-export.md) uses up to 1200 dpi, reducing resolution for large drawings while retaining physical size.

Atom labels without individual font overrides inherit the document style. **Update matching text and strokes** also updates existing captions, atom font overrides, arrows, and graphics whose settings match the old style. Different font families, sizes, and line widths remain unchanged. Colors, bold/italic formatting, and chemical connectivity are retained.

**Scale layout with bond length** is off by default. With it off, existing positions stay fixed and the new bond length is used for subsequent drawing. With it on, object geometry scales around the drawing center by the ratio of new to old nominal bond length. This includes molecular coordinates, annotation positions, arrow paths, and graphic geometry. Paper dimensions and margins stay fixed; explicit text sizes and line widths follow the separate matching-settings option. This is a proportional layout change, not molecular cleanup.

## Reuse a style

**Save style…** saves the panel settings as a `.reshiki-style` file without changing the drawing. **Load…** opens a saved style into the preview; Apply is still required. A style file contains settings only, not molecules or page contents. Native style files are validated before use and limited to 64 KB. **Load…** also accepts ChemDraw `.cds`, `.cdx` and `.cdxml` files up to 16 MB, including older journal stationery. It extracts the document label font and bond dimensions; artwork, page layout, colors and separate caption settings are not imported.

[Journal settings and publisher sources](journal-drawing-presets.md) explain the five publisher-based choices and compare their actual drawings in ReShiki and ChemDraw. Complete stationery documents combining page layouts, objects, and styles are not imported by this panel.

## Persistence and exchange

Native document format 14 and later store drawing settings. Supported older documents open with JACS / ACS defaults. Cleanup, aromatic display changes, copy selection, recovery, and Undo/Redo retain the document style. Assistant previews use the current style.

SVG, PDF, and PNG use the same styled scene as the canvas. Supported CDXML/CDX document-level fonts and bond dimensions are written and read in physical units. Import preserves the declared physical size instead of normalizing every molecule to 14.4 pt bonds. A style's descriptive name is native metadata; editable exchange may call it **Imported style**. Pasting into an existing document uses the destination's bond style while retaining supported explicit object overrides.

Imported documents can therefore be larger than before when their source declared larger bonds. That reflects their actual publication size. Journal recommendations may cover additional requirements beyond these supported drawing settings.

## Verification

Regression checks cover native save/reopen, style-file validation, physical editable exchange, chemical identity, aromatic circle ownership, cleanup, explicit overrides, optional scaling, invalid-input rejection, stale drafts, default resets, and a single Apply/Undo/Redo transaction.

The interface was rendered offscreen at 1280 × 820 and 1040 × 680. Pointer events verified that Apply remains visible and clickable at both sizes. Snapshots are under the ignored `artifacts/document-style-qa/` directory. These checks do not replace a full native desktop acceptance pass.

## Canvas colors and interface appearance

The top-right controls separate **journal style**, **canvas color theme**, and **canvas brightness**. The sun/moon button shows the current canvas mode; click it to switch between white and black paper. These controls are also available in **Page setup**. They do not change bond dimensions, font sizes, or bold formatting.

The color theme offers:

- **Publication**: neutral black labels on light paper, white labels on dark paper.
- **Presentation**: balanced conventional element colors, including blue nitrogen and red oxygen. Carbon, hydrogen, and bonds stay neutral unless individually recolored.
- **Pastel**: softer element colors, with enough contrast to remain legible on light paper.
- **Jmol**: the [Jmol CPK palette](https://jmol.sourceforge.net/jscolors/), using the published [element color table](https://jmol.sourceforge.net/jscolors/jmol_constants.js) for H through Mt. Later elements retain neutral colors. Canvas labels adjust brightness only when necessary for at least 3:1 contrast against the paper, including white hydrogen and yellow sulfur on light canvases.

Presentation and Pastel have separate light and dark palettes. Selecting a theme replaces atom color overrides, including pasted atom colors, while preserving typography and geometry. You can assign individual colors afterward; choosing a theme again resets them. New atoms automatically follow the theme. Manually colored vector objects retain their hue while their lightness adapts to canvas brightness; raster pictures keep their original pixels. The toolbar swatches and hex field show visible canvas colors.

The periodic table and quick atom buttons use colored tile backgrounds with regular-weight, high-contrast symbols. Jmol tiles tint their backgrounds with its published element colors; canvas label brightness adjustments do not change those tile hues. A thicker outline identifies the selected element.

Canvas colors are saved with the document and participate in Undo/Redo. File exports (PNG, SVG, PDF and ChemDraw) and printing include the canvas background and visible colors. Copies always have a transparent background: Copy Image retains the visible ink in PDF/PNG/SVG, and editable ChemDraw copies retain those object colors without adding a page rectangle. Pasting between different canvas modes in ReShiki likewise retains the source ink without adding a background.

**View → Interface** controls only the surrounding interface: **Match canvas**, **Light**, or **Dark**. The preference is remembered between launches and never changes the drawing, document history, clipboard, or exports. There is no separate dark drawing preset.

Ring interiors use a separate highlight palette: **Sky, Mint, Rose, Lilac, and Sand**. Light canvases use pastel tints; dark canvases use medium tones that preserve white-bond contrast. Automatic element labels on dark filled rings brighten as needed to achieve 4.5:1 contrast against the fill. Explicit atom colors are preserved. Imported/pasted fills retain their visible RGB color. A custom hex value sets an exact visible color. Copying still leaves the area outside the drawing transparent; the colored ring interior remains part of the drawing.
