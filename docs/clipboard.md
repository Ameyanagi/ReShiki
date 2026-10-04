# Clipboard

On Windows, macOS and supported Linux desktop sessions, select drawing objects and use **Ctrl+C** / **Cmd+C** to copy editable structure data together with image alternatives. **Ctrl+X** / **Cmd+X** cuts only after the clipboard write succeeds. **Ctrl+V** / **Cmd+V** inserts supported drawing data as one undoable edit. The private ReShiki representation retains the full supported document model, including groups and custom presentation fields.

Use **Ctrl+Shift+C** / **Cmd+Shift+C**, or **Export → Copy image**, for a picture of the selection. With nothing selected, it copies the whole drawing. This supplies PDF, PNG and SVG; PNG starts at 1200 dpi. On Windows, large raster previews use a lower resolution to fit clipboard and picture limits, with a notice showing the actual DPI. Resolution metadata preserves the physical size; editable drawing data and vector formats keep their precision. A raster-only drawing representation also carries explicit publication-size bounds for compatible drawing applications. A pasted picture does not contain editable atoms or bonds. View aids such as grid, rulers and crosshair are excluded.

## Copy as

Right-click and choose **Copy as…** to choose one format: **PNG**, **SVG**, **PDF**, **MOL**, **SMILES**, **InChI**, **CDXML**, **CDX**, **RXN · V3000**, **Reaction SMILES**, or **ChemDoodle JSON · reaction**. The submenu says **selected objects** when there is a selection and **whole drawing** otherwise. It uses the same selection rules as ordinary Copy, retaining required abbreviation and attachment members. It does not automatically select the rest of a partially selected molecule; use **Select molecule** first when that is what you need.

PNG, SVG and PDF copy the visible figure without the canvas background. Molecular formats copy chemical data, not captions or figure layout. Select a molecule within a reaction for MOL, SMILES or InChI. RXN and reaction SMILES require one complete, explicitly defined reaction with reactants and products; a drawn arrow alone does not assign roles. The submenu explains unavailable formats. CDXML and CDX use the strict drawing exporters and do not silently substitute a picture when a feature cannot be represented.

Conversion finishes before the clipboard changes. If conversion fails, or the drawing or selection changes before it is ready, the previous clipboard stays intact. Format warnings appear in the copy receipt. Switching tabs keeps the request with its source drawing; closing that tab before preparation completes discards it. No drawing, selection or Undo history is changed by Copy as.

**ChemDoodle JSON · reaction** prepares plain text for a compatible sketcher's explicit JSON import, using the [published ChemDoodle JSON schema](https://web.chemdoodle.com/docs/chemdoodle-json-format). It preserves reactant/product membership through the arrow's atom-ID references, molecular connectivity, coordinates, charge, isotopes and attached hydrogen counts. Separate explicit H atoms remain separate. It currently requires one forward reaction, one connected molecule per participant and coefficients of one. Agents, atom maps, stereo (including stereo inferred from geometry), aromaticity, radicals, special bonds, attachment atoms and collapsed abbreviations are rejected before the clipboard changes. Captions and drawing styles are omitted with a visible receipt notice. The copied text does not contact CAS or submit a search; see the [manual SciFinder handoff](scifinder-handoff.md).

Text formats also provide ordinary clipboard text. On platforms without native clipboard support, **SVG text** copies the SVG markup; PNG, PDF and CDX remain unavailable and can be exported as files instead. Normal **Copy**, **Copy image**, and their shortcuts keep their existing purpose. The SMILES/MOL/CDXML text shortcuts use the same conversion and warning handling, and still require a selection. For a manual database-search handoff, see [SciFinder structure handoff](scifinder-handoff.md).

Tracked in [#110](https://github.com/Ameyanagi/ReShiki/issues/110). Clipboard recipients choose which offered representation they accept; a successful copy does not establish that another application imported it correctly.

## Office and macOS

On Windows, normal Copy supplies an editable Office object with the full drawing and a cached preview. Double-click opens ReShiki; **Ctrl+S** updates the open Office document. Copy Image supplies a static figure with SVG text converted to outlines to preserve label placement. SVG file export continues to contain text. [Office editing steps](windows.md#edit-a-drawing-in-office).

Windows desktop Office supports ReShiki OLE objects. Copy the object back into ReShiki to recover its native drawing; Paste picture explicitly reads its preview. Copy Image remains the command for applications that only accept pictures. Normal Copy omits standalone PNG/SVG/bitmap formats because Word prefers them over the editable object; it retains the metafile presentation required by Office.

Clipboard PNG, PDF and SVG figures always have transparent backgrounds. They retain the visible ink: black bonds and labels from a light canvas, white bonds and labels from a dark canvas. The Windows editable object's EMF+ dual preview keeps bonds and outlined text as vectors. Imported pictures retain their own pixels and transparency. Open an older embedded ReShiki object and press Ctrl+S to refresh its preview, then save the Office document.

Dark editable ChemDraw copies use white bonds and labels without a background rectangle. Pasting between ReShiki drawings adds no page background; Ink and palette colors follow the receiving drawing's canvas and theme, and custom colors stay exact. **View → Interface** affects only the app controls; it never changes clipboard output.

macOS retains its existing native drawing/CDX and PDF/PNG/SVG clipboard representations. Mac Office does not provide this Windows OLE activation workflow. Keep the `.rsk` original, edit it in ReShiki and replace the Office figure. A pasted picture alone cannot restore native atoms and bonds. Mac Office round-trip behavior has not been verified by this Windows test run.

Editable external exchange uses the supported binary CDX/CDXML subset. It covers tested molecules, formal charges, isotopes, tetrahedral and double-bond stereo, bond/label colors, styled text, supported arrows, graphics, embedded raster pictures and nested groups. Some scientific symbols and orbitals become editable vector paths rather than retaining their original preset type. If the receiving editor asks which document settings to use, preserving the copied settings retains the source appearance.

Paste prefers native drawing data and explicit external chemical formats, then PNG/JPEG/TIFF/WebP pictures, then plain chemical text. A structurally valid drawing can be pasted even if chemical validation fails; its atoms, bonds and supplied charges are preserved, with a review notice and no molecular properties. Malformed or unsupported editable content still produces an error rather than silently substituting a picture. **Import → Paste picture** explicitly chooses a raster representation, including when editable formats are also present.

## Editable ligand copying

Use ordinary **Cmd/Ctrl+C** and **Cmd/Ctrl+V** in both directions. Cp and arene examples retain editable ring atoms, aromatic bond orders, perspective edges and multicenter attachment targets. Cp’s hidden −1 charge is carried by an invisible attached charge symbol; it is not removed from the chemical data. Metal charges remain as entered. Returning these drawings to ReShiki reconstructs one aromatic ellipse per ring, without extra C/H labels or a duplicate curve.

The shortcut gallery is supplied as editable CDX on the clipboard even when its high-resolution PNG would be too large. Keep the `.rsk` original for complete projection depth and ReShiki-specific editing metadata.

When a receiving editor returns R/X as query nicknames, ReShiki imports them as uninterpreted atom text with a notice. Molecular properties and query semantics are unavailable for that drawing; other unsupported query types remain rejected. NO2/N3 group definitions retain their formal charges through the tested return copy. [Before/after captures and return-paste verification](changes/shortcut-help.md#editable-cp-and-arene-round-trips).

Styled metal contacts are also retained when chemical normalization would otherwise turn their single bonds into coordinate bonds with incompatible wedge styles. That import keeps the supplied drawing, reports that coordination assignments need review, and withholds molecular properties.

### Changes made for an external copy

| Feature                                                                 | Editable CDX clipboard behavior                                                                           |
| ----------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| Cp/arene perspective, aromatic ellipse and attachment targets           | Preserved in the tested desktop round trip.                                                               |
| Hidden ±1 charge on an aromatic carbon without other marks              | Retained as an invisible attached symbol.                                                                 |
| Other hidden charge labels                                              | Shown in the external copy; charge values are retained.                                                   |
| R/X and other variable text                                             | Kept as an uninterpreted atom label; variable/query semantics are omitted.                                |
| Partial inner-ring curves                                               | Omitted; the underlying bond orders remain.                                                               |
| Perspective emphasis outside supported aromatic/Haworth cases           | Copied as plain bonds when no assigned stereocenter is affected.                                          |
| Compact groups with multicenter attachment points                       | May expand into their editable constituent atoms in the receiving editor.                                 |
| Retained 3D tilt coordinates and ReShiki-specific presentation metadata | Preserved by native `.rsk` and the private clipboard representation; not guaranteed through external CDX. |

Copy reports any applied simplifications. File export stays strict. Unsupported content beyond these conversions uses the explicitly reported picture fallback. These limits do not change the source drawing.

Ring interiors copy as ChemDraw native `ColoredMolecularArea` objects linked to
their ring bonds. The fill stays attached when atoms move, and imported fills
retain their visible RGB colors. Clipboard images still have transparent space
outside the drawing. Older ReShiki-owned filled curves remain readable when
their ownership metadata survives.

## Boundaries

- Picture paste creates one embedded object and one Undo step. Native ReShiki Copy/Paste preserves a picture’s frame, orientation and transparency. Copy Image also supplies a native raster object so normal Paste retains its publication size; PNG paste honors resolution metadata. [Picture controls and limits](pictures.md).
- External editable exchange retains pictures as separate objects alongside chemistry, with physical bounds, rotation, transparency and groups. Reflections are stored in the picture pixels without resampling. Embedded vector/OLE-only pictures remain unsupported. [Formats and limits](pictures.md).
- Binary exchange shares the documented CDXML restrictions. Query/reaction predicates, unsupported objects, polymer semantics and enhanced stereo are not supported. External text metrics can differ. Binary paragraph line spacing is rounded to whole points; native documents and image exports retain their own precision.
- If external editable export is unavailable, Copy still provides the native ReShiki drawing, available images and a sized raster CDX picture, with a visible notice. Other drawing editors receive the picture; ReShiki recovers the editable original. Image representations that fail to render are also reported.
- The binary codec accepts up to 16 MB. Native integration limits all representations to 64 MB combined. Windows raster previews are bounded to 16 million pixels and 8192 pixels per side, so decoded Office previews and Copy Image bitmaps fit the 64 MB limit; regular editable Copy does not allocate a standalone bitmap that Office would discard. macOS retains its 1200 dpi clipboard raster limit of 80 million pixels. Drawings with large embedded pictures or other encoded representations can still exceed the combined clipboard limit; file export remains available.
- Linux supports native drawings, explicit chemical formats, images and text through X11 or the focused Wayland window's standard clipboard connection. Keep ReShiki focused until Copy finishes; loss of focus or an unconfirmed write leaves a Cut's source intact. Wayland clipboard data remains available while ReShiki owns the selection; persistence after exit depends on the desktop clipboard manager. X11 uses a persistent owner process until another copy replaces it. Windows Copy Image supplies CF_DIB alongside PNG, PDF and SVG. Only ReShiki's own OLE objects are read as editable native documents; arbitrary embedded objects are not activated or imported. See the [Windows guide](windows.md).
- CDX is exposed through the clipboard and internal Rust chemistry interface; the file Open/Export UI does not yet offer it.

Internal condensed labels such as CCl₂, CF₂ and NMe retain formula typography.
When the receiving editor returns an explicit chemical group definition with
ordered connections on one attachment atom, ReShiki preserves that definition
and both outside bonds. Plain named dummy labels remain uninterpreted text;
formatting alone does not assign chemistry.

## Verification

Native desktop tests copied a 13-atom, 13-bond aspirin drawing with red bonds and teal labels into another drawing editor, then copied it back into ReShiki. The returned binary data retained the molecular identity, counts and colors. A separate image paste retained the publication dimensions instead of expanding to the PNG's pixel dimensions. The standalone worker was launched outside the checkout.

A seven-atom, six-bond aromatic drawing that fails chemical validation was also copied into ReShiki, back into the source editor, and returned through the native clipboard as editable objects. Both binary captures are regression fixtures. Variable labels now copy as editable uninterpreted atom text; query semantics are not assigned.

A ChemDraw 26 test copied a caffeine drawing with two filled rings into ChemDraw, moved a ring atom, and copied it back. Both fills remained bond-linked native areas with the same visible colors, with no loose curves or pictures. The ChemDraw-created ring and return clipboard captures are regression fixtures in `tests/fixtures/ring-fills/`.

Automated tests cover independent native clipboard data, supported exchange fixtures, Unicode font runs, large property lengths, truncation, duplicate identifiers, invalid base64, query rejection, stale asynchronous completion and failed Cut. The raster wrapper is checked for image-only contents, unchanged PNG bytes and physical bounds. Local desktop artifacts are under `artifacts/clipboard-qa-20260920/` and are ignored by Git. These examples establish the tested subset, not universal external-document compatibility.

The macOS adapter maps the portable `chemical/x-cdxml` representation to
ChemDraw's `com.revvity.cdxml`, `com.perkinelmer.cdxml` and
`com.cambridgesoft.cdxml` types, retaining the exact UTF-8 text fallback.
[`NSPasteboardItem` requires a UTI](<https://developer.apple.com/documentation/appkit/nspasteboarditem/setdata(_:fortype:)>),
so a MIME string containing `/` cannot be passed directly. These aliases are
declared by ChemDraw 26's bundle metadata. Unique-pasteboard regressions check
the exact native types and bytes for picture and chemical format packets,
including ChemDoodle reaction text, and unchanged prior contents after invalid,
conflicting or oversized writes. The available Copy as options are listed above.
The 64 MB combined limit includes the expanded native aliases.

Linux transport tests additionally cover multi-format selection publication, a 2 MiB X11 incremental image transfer, replacement and one-shot owners, rejected oversized transfer headers, stalled recipients, and atomic rejection of invalid writes. These are isolated Xvfb and process/pipe checks; they do not establish native GUI or compositor-specific Wayland compatibility. [Transport design and test commands](../native/linux/README.md).
