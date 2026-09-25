# Clipboard

On Windows and macOS, select drawing objects and use **Ctrl+C** / **Cmd+C** to copy editable structure data together with image alternatives. **Ctrl+X** / **Cmd+X** cuts only after the clipboard write succeeds. **Ctrl+V** / **Cmd+V** inserts supported drawing data as one undoable edit. The private ReShiki representation retains the full supported document model, including groups and custom presentation fields.

Use **Ctrl+Shift+C** / **Cmd+Shift+C**, or **Export → Copy image**, for a picture of the selection. With nothing selected, it copies the whole drawing. This supplies PDF, 1200 dpi PNG and SVG; a raster-only drawing representation also carries explicit publication-size bounds for compatible drawing applications. A pasted picture does not contain editable atoms or bonds. View aids such as grid, rulers and crosshair are excluded.

On Windows, normal Copy supplies an editable Office object with the full drawing and a cached preview. Double-click opens ReShiki; **Ctrl+S** updates the open Office document. Copy Image supplies a static figure with SVG text converted to outlines to preserve label placement. SVG file export continues to contain text. [Office editing steps](windows.md#edit-a-drawing-in-office).

## Office and macOS

Windows desktop Office supports ReShiki OLE objects. Copy the object back into ReShiki to recover its native drawing; Paste picture explicitly reads its preview. Copy Image remains the command for applications that only accept pictures. Normal Copy omits standalone PNG/SVG/bitmap formats because Word prefers them over the editable object; it retains the metafile presentation required by Office.

Clipboard PNG figures have a transparent background on Windows and macOS. The Windows editable object's EMF+ dual preview keeps bonds and outlined text as vectors, preserving smooth edges when enlarged. It also preserves transparency, including partially transparent imported pictures. The standard Windows bitmap fallback and PNG file export have a white background. Open an older embedded ReShiki object and press Ctrl+S to refresh its preview, then save the Office document.

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

## Boundaries

- Picture paste creates one embedded object and one Undo step. Native ReShiki Copy/Paste preserves a picture’s frame, orientation and transparency. Copy Image also supplies a native raster object so normal Paste retains its publication size; PNG paste honors resolution metadata. [Picture controls and limits](pictures.md).
- External editable exchange retains pictures as separate objects alongside chemistry, with physical bounds, rotation, transparency and groups. Reflections are stored in the picture pixels without resampling. Embedded vector/OLE-only pictures remain unsupported. [Formats and limits](pictures.md).
- Binary exchange shares the documented CDXML restrictions. Query/reaction predicates, unsupported objects, polymer semantics and enhanced stereo are not supported. External text metrics can differ. Binary paragraph line spacing is rounded to whole points; native documents and image exports retain their own precision.
- If external editable export is unavailable, Copy still provides the native ReShiki drawing, available images and a sized raster CDX picture, with a visible notice. Other drawing editors receive the picture; ReShiki recovers the editable original. Image representations that fail to render are also reported.
- The binary codec accepts up to 16 MB. Native integration limits all representations to 64 MB combined and the rendered raster to 80 million pixels. Large drawings can exceed the combined clipboard limit before reaching the raster limit; file export remains available.
- Linux uses the earlier text clipboard path; picture file import is available in the app. Windows Copy Image supplies CF_DIB alongside PNG, PDF and SVG. Only ReShiki's own OLE objects are read as editable native documents; arbitrary embedded objects are not activated or imported. See the [Windows guide](windows.md).
- CDX is exposed through the clipboard and internal Rust chemistry interface; the file Open/Export UI does not yet offer it.

Internal condensed labels such as CCl₂, CF₂ and NMe retain formula typography.
When the receiving editor returns an explicit chemical group definition with
ordered connections on one attachment atom, ReShiki preserves that definition
and both outside bonds. Plain named dummy labels remain uninterpreted text;
formatting alone does not assign chemistry.

## Verification

Native desktop tests copied a 13-atom, 13-bond aspirin drawing with red bonds and teal labels into another drawing editor, then copied it back into ReShiki. The returned binary data retained the molecular identity, counts and colors. A separate image paste retained the publication dimensions instead of expanding to the PNG's pixel dimensions. The standalone worker was launched outside the checkout.

A seven-atom, six-bond aromatic drawing that fails chemical validation was also copied into ReShiki, back into the source editor, and returned through the native clipboard as editable objects. Both binary captures are regression fixtures. Variable labels now copy as editable uninterpreted atom text; query semantics are not assigned.

Automated tests cover independent native clipboard data, supported exchange fixtures, Unicode font runs, large property lengths, truncation, duplicate identifiers, invalid base64, query rejection, stale asynchronous completion and failed Cut. The raster wrapper is checked for image-only contents, unchanged PNG bytes and physical bounds. Local desktop artifacts are under `artifacts/clipboard-qa-20260920/` and are ignored by Git. These examples establish the tested subset, not universal external-document compatibility.
