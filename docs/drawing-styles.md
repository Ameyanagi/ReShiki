# Document drawing styles

Click the style name beside the drawing controls, or choose **Properties → Edit drawing style…**. New documents always start with **JACS / ACS**. Changing one document does not change the defaults for another.

Choose **JACS / ACS**, **Presentation**, or edit a custom style. The panel previews a sample structure while the document stays unchanged. **Apply to document** commits one Undo step; **Cancel** discards the draft. Apply and Cancel remain visible while the settings scroll.

## Settings

The main controls set the label font, label size, nominal bond length, and line width. **Advanced stroke settings** exposes bold/wedge width, label clearance, hash spacing, and multiple-bond spacing as a percentage of nominal bond length. Dimensions are publication points, independent of screen zoom. PNG output remains 1200 dpi.

Atom labels without individual font overrides inherit the document style. **Update matching text and strokes** also updates existing captions, atom font overrides, arrows, and graphics whose settings match the old style. Different font families, sizes, and line widths remain unchanged. Colors, bold/italic formatting, and chemical connectivity are retained.

**Scale layout with bond length** is off by default. With it off, existing positions stay fixed and the new bond length is used for subsequent drawing. With it on, object geometry scales around the drawing center by the ratio of new to old nominal bond length. This includes molecular coordinates, annotation positions, arrow paths, and graphic geometry. Paper dimensions and margins stay fixed; explicit text sizes and line widths follow the separate matching-settings option. This is a proportional layout change, not molecular cleanup.

## Reuse a style

**Save style…** saves the panel settings as a `.reshiki-style` file without changing the drawing. **Load…** opens a saved style into the preview; Apply is still required. A style file contains settings only, not molecules or page contents. Files are validated before use and limited to 64 KB.

This supports reusable drawing settings. Complete stationery documents combining page layouts, objects, and styles are not yet implemented.

## Persistence and exchange

Native document format 14 and later store drawing settings. Supported older documents open with JACS / ACS defaults. Cleanup, aromatic display changes, copy selection, recovery, and Undo/Redo retain the document style. Assistant previews use the current style.

SVG, PDF, and PNG use the same styled scene as the canvas. Supported CDXML/CDX document-level fonts and bond dimensions are written and read in physical units. Import preserves the declared physical size instead of normalizing every molecule to 14.4 pt bonds. A style's descriptive name is native metadata; editable exchange may call it **Imported style**. Pasting into an existing document uses the destination's bond style while retaining supported explicit object overrides.

Imported documents can therefore be larger than before when their source declared larger bonds. That reflects their actual publication size. Style names are descriptive: the Presentation preset is a general larger drawing preset, not a claim of compliance with a journal specification.

## Verification

Regression checks cover native save/reopen, style-file validation, physical editable exchange, chemical identity, aromatic circle ownership, cleanup, explicit overrides, optional scaling, invalid-input rejection, stale drafts, default resets, and a single Apply/Undo/Redo transaction.

The interface was rendered offscreen at 1280 × 820 and 1040 × 680. Pointer events verified that Apply remains visible and clickable at both sizes. Snapshots are under the ignored `artifacts/document-style-qa/` directory. These checks do not replace a full native desktop acceptance pass.
