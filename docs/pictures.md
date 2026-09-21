# Pictures in drawings

Use **Import → Picture…** to insert a PNG, JPEG, TIFF or WebP file. The drawing stays responsive while the picture loads. Cancel leaves the drawing unchanged. If you switch documents or edit the drawing during loading, import again when ready; a late result never replaces newer work.

On macOS, ordinary **Paste** accepts pictures when no explicit editable drawing representation takes priority. **Cmd+Shift+V**, or **Import → Paste picture**, chooses the raster representation explicitly. Clipboard reads are asynchronous and reject stale completion. PNG paste respects physical-resolution metadata.

Select a picture to move it, drag a corner to resize proportionally, or use the top handle to rotate. Existing Flip H / Flip V, grouping, alignment and distribution controls also apply. The Picture inspector provides:

- Width and height in millimetres. Press Enter to apply a dimension. **Link width and height** preserves the current proportions when entering dimensions.
- **Restore original proportions**, fitted inside the current frame.
- **Replace picture…**, retaining the center, orientation, reflection and layer while fitting the new picture inside the old frame.
- **Send to back** and **Bring to front**, with the same stacking order on the canvas and in exports.

Each insertion, replacement and transformation is undoable. Pictures are normalized to embedded PNG data, including transparency, so native files do not depend on the source image. Native Copy/Paste keeps the full frame; SVG/PDF/PNG export includes pictures alongside chemical drawing objects. The assistant can inspect pictures through its rendered canvas image.

CDXML export and editable macOS Copy also carry embedded pictures beside editable molecules, text and supported graphics. Pictures retain their physical size, position, rotation, transparency, stacking order and group membership. Reflections are encoded by reversing image pixels without resampling; the receiving editor still gets a separate picture object. Import accepts embedded PNG, TIFF, JPEG, GIF and BMP representations and normalizes them to PNG. Export placement includes pictures when calculating the page margin.

## Limits

Files are limited to 16 MB, 16 million pixels, and 8192 pixels per side. A drawing permits at most 64 MB of encoded pictures and 64 million picture pixels. File import starts at 300 dpi and fits very large pictures within 100 mm; dimensions remain editable. Animated/multipage images currently use only the first frame. Embedded pictures use 8-bit RGBA pixels; retain the source file for original bit depth and metadata. There is no crop, masking, image adjustment or PDF/SVG picture import. External vector/OLE-only pictures are rejected; when a raster alternative is present, it is imported as a picture without external application links or object metadata. The binary drawing codec has a separate 16 MB limit; large pictures can exceed it. Unsupported external picture exchange reports an error; it does not substitute an empty frame.

## Verification

Regression tests cover supported decoding formats, malformed inputs and size limits, transformations and transparency, export layering, native persistence, Undo, asynchronous cancellation/stale results, replacement, numeric dimensions and assistant image context. macOS clipboard tests use a private pasteboard. Windows integration tests use the real desktop clipboard to verify editable/image precedence and round-trips; they replace its contents with test data.

Desktop checks used an isolated document with a transparent PNG and editable ethanol: file import, physical dimensions, rotation/resize handles, front/back ordering, native save/reopen and clipboard workflows. Local QA artifacts are under `artifacts/pictures-qa/` and are not included in Git.

External picture checks opened a mixed drawing in another desktop editor, selected and moved a picture independently, saved the file, and reimported the chemistry, picture and group. Separate 37° and 90° pictures retained orientation and transparency. `tests/fixtures/picture-group-native.cdx` is an independently re-saved fixture. Rust tests compare a reflected, rotated picture's pixels and placement relative to its molecule; Python tests cover supported embedded formats, extended binary properties, fixed-point angles, grouped objects, margin placement, malformed data and image budgets. The standalone worker also imports the fixture when launched outside the checkout. Local exchange evidence is under `artifacts/picture-exchange-qa/`.
