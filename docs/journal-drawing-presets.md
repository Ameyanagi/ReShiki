# Journal drawing presets

Choose a journal directly from the preset selector beside the color theme selector and sun/moon canvas toggle. Selecting a journal applies its dimensions and scales the drawing in one Undo step. Choose **Drawing style…** at the bottom for a preview and detailed settings. The selector contains **JACS / ACS**, **Nature**, **RSC**, **Angewandte**, **SYNLETT / SYNTHESIS**, and **Custom**. Each journal choice has a **Publisher instructions ↗** link in the panel.

These five presets use publisher recommendations or publisher downloads. The ChemDraw standard baseline and bundled variants are excluded. Publisher guidance takes precedence where written dimensions differ from older downloads.

## Publisher settings and sources

Reviewed 26 September 2026. Dimensions are publication points; values below are rounded for readability. The reusable `.reshiki-style` files in `presets/drawing/` retain source precision.

| Preset / publisher instructions | Font | Label | Bond | Line | Bold | Margin | Hash | Spacing |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| [JACS / ACS](https://pubsapp.acs.org/paragonplus/submission/general/graphics_prep.html) | Arial | 10 | 14.4 | 0.6 | 2 | 1.6 | 2.5 | 18% |
| [Nature](https://www.nature.com/nature/for-authors/formatting-guide) | Helvetica | 6 | 10.8 | 0.5953 | 1.559 | 1.191 | 1.701 | 18% |
| [RSC](https://www.rsc.org/publishing/publish-with-us/publish-a-journal-article/analytical-methods) | Arial | 7 | 12.2 | 0.5 | 1.6 | 1.25 | 1.8 | 20% |
| [Angewandte](https://onlinelibrary.wiley.com/page/journal/15213773/homepage/notice-to-authors) | Arial | 12 | 17 | 0.75 | 2.602 | 2 | 2.602 | 18% |
| [SYNLETT / SYNTHESIS](https://www.thieme.de/statics/dokumente/thieme/final/de/dokumente/zw_synthesis/Instr_SS_26_feb.pdf) | Arial | 6 | 10.12 | 0.5669 | 1.389 | 0.9071 | 1.757 | 18% |

- **ACS:** Published ACS-1996 dimensions. The ACS graphics page was last updated in 2006; it permits Helvetica on Mac and Arial on PC. ReShiki retains Arial for cross-platform compatibility and its existing default.
- **Nature:** Uses the publisher’s [ChemDraw stylesheet](https://www.nature.com/documents/nr-chemdraw-stylesheet.cds), including Helvetica 6 pt. See also the [chemical structure style guide](https://www.nature.com/documents/nr-chemical-structures-guide.pdf).
- **RSC:** Uses the explicit point values in the publisher’s author instructions, including 12.2 pt bonds and 0.5 pt lines. The 1.25 pt label margin, unspecified in the written guidance, comes from the publisher’s [structure template download](https://www.rsc.org/publishing/publish-with-us/publish-a-journal-article/article-templates).
- **Angewandte:** Uses the publisher’s [stationery download](https://wol-prod-cdn.literatumonline.com/pb-assets/assets/15213773/angew-cds.zip), a legacy ChemDraw 4.5 CDS file dated 2003 in the archive.
- **SYNLETT / SYNTHESIS:** Uses final-size dimensions illustrated in the February 2026 publisher instructions, converting centimetres with 72 / 2.54 points per centimetre. These are already final-size values: do not reduce them again to 69%. The 69% instruction refers to ChemDraw’s bundled stationery. Arial is retained because the illustrated settings do not specify a font family.

The settings cover label typography and bond dimensions. Page layout, independent caption formatting, template artwork and chemical naming conventions remain separate. Matching dimensions does not guarantee identical rendering or compliance with every journal’s submission requirements.

## Custom styles and downloaded stationery

**Custom** starts from the current draft without resetting its dimensions. Edit the name, font, sizes and advanced stroke settings; **Save style…** creates a reusable `.reshiki-style` file. **Load…** restores one into the preview. Editing a journal’s settings switches to Custom. Applying changes creates one Undo step; Cancel discards the draft.

Load publisher `.cds`, `.cdx` or `.cdxml` files directly in **Drawing style → Load…**. Modern CDX and the legacy implicit-root Angewandte CDS format are supported. Only document font and bond settings are read; embedded artwork is skipped. Invalid or incomplete settings are rejected. Native JSON is limited to 64 KB and ChemDraw stationery to 16 MB.

Enable **Scale layout with bond length** when applying a journal preset to resize existing geometry. With it disabled, existing positions remain fixed and new bonds use the chosen length.

## Canvas colors and interface appearance

The **sun/moon toggle** beside the preset and color theme selectors changes document page colors independently of its journal dimensions. The same setting is available in **Page setup**. Dark canvas displays and exports black paper with white bonds and labels. Colored vectors adapt in lightness while retaining their hue; raster pictures retain their original pixels. The canvas choice is saved in the document and can be undone.

**Publication**, **Presentation**, **Pastel**, and **Jmol** control atom colors independently of the journal preset and light/dark canvas. Presentation and Pastel derive from the [Jmol element color reference](https://jmol.sourceforge.net/jscolors/): Presentation reduces OKLCH chroma for balanced colors, while Pastel uses softer, lighter tones. Both brighten on dark canvases and retain neutral carbon/hydrogen labels. Jmol keeps the reference palette, with label brightness adjusted for readable contrast. All three use colored element-picker tiles with regular-weight symbols. Ring interiors use a separate palette adapted to each canvas, and automatic atom labels stay legible over light and dark fills.

File exports and printing carry the visible canvas colors and background. Clipboard images and editable copies always have transparent backgrounds, retaining the visible bond and label colors. Dark drawings therefore copy as white ink without a black page rectangle. No separate dark preset is needed.

In **View → Interface**, choose **Match canvas**, **Light**, or **Dark** for the surrounding interface only. This application preference is remembered between launches and never changes the canvas or output.

## Comparing actual drawings

```sh
cargo run --example journal_styles -- artifacts/journal-styles/comparison
python3 scripts/compare_journal_styles.py artifacts/journal-styles/comparison
```

The generator uses identical aspirin, caffeine and alanine coordinates for every renderer, scaled to each preset’s physical bond length. The Python script requires ChemDraw on macOS and saves actual SVG, PDF, CDXML and CDX captures from isolated temporary documents. The comparison page contains the same five publisher presets as the app. Native CDX settings are checked to 0.00002 pt (spacing to 0.000001 of bond length); CDXML rounds to 0.01 pt.

Downloaded source files and render captures live under ignored `artifacts/journal-styles/`. Proprietary stationery binaries are not committed. `presets/drawing/sources.json` records publisher links, original filenames, source hashes and extracted settings.

Drawing styles export separately as `.reshiki-style` or ChemDraw `.cds`. Color themes export as `.reshiki-theme`, containing both light and dark palettes. See [sharing styles and themes](drawing-styles.md#share-styles-and-themes) and the [theme library guide](https://github.com/Ameyanagi/ReShiki/tree/main/presets/themes).
