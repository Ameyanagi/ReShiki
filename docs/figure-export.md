# Exporting figures

Open **Export → Figure**, choose a format from the menu (**Vector**: **SVG**, **SVG for Office**, **PDF** and, on Windows, **EMF for Office**; **Raster**: **PNG**), then click the export button (for example **Export SVG…**) and choose a destination. File export includes the whole drawing. **Copy image** uses the selected objects, or the whole drawing when nothing is selected.

| Format                   | Output                                                                                                      |
| ------------------------ | ----------------------------------------------------------------------------------------------------------- |
| PDF                      | Vector artwork at the drawing's physical publication size.                                                  |
| SVG                      | Vector artwork for illustration and layout applications. Text becomes outlines unless you keep it editable. |
| SVG for Office           | Physical-size SVG for Word and PowerPoint. Text always becomes outlines, so labels stay in place.           |
| PNG                      | An opaque image at the chosen resolution (1200 dpi by default), with physical resolution metadata.          |
| EMF for Office (Windows) | Vector picture for Microsoft Office at the drawing's physical size. Text becomes vector outlines.           |

By default, file exports in all formats include the document’s canvas colors and background, and copied pictures use a transparent background, preserving the visible ink for slides and other documents. With **View → Dark canvas** enabled, exports use white bonds and labels on black paper; **Settings → Appearance → Interface** changes only the surrounding UI. To change either default, see [Background](#background).

Canvas zoom, rulers, grid and crosshair do not affect the exported figure. **Export pages as PDF** uses an explicitly configured publication layout; ordinary figure export crops to the artwork. See [publication pages](publication-pages.md).

## Export again

Available in Nightly builds; not in ReShiki 0.16.0.

After exporting a picture, the top of **Export** offers **Export again**, with
the filename, format and PNG resolution. Click it to replace that file with the
current full drawing, using the previous background, resolution, SVG text and
editable-drawing choices. Page exports keep their all-pages choice. The drawing's
current geometry and style determine its physical size.

The latest successful picture export replaces the previous repeat choice. Its
row belongs to the drawing that chose the destination. On the desktop, reopening
a saved drawing restores it. Saving an untitled drawing keeps that link when you
reopen it. The browser keeps the row for the current tab until the drawing is
replaced or the page reloads. Normal export still asks for a filename and starts
in the last picture-export folder.
Canceling or failing an export keeps the previous successful export. In the
browser, **Export again** starts another download with the same name; the browser
decides where it goes and whether it adds a suffix.

## Background

Copied pictures and exported files each have their own background. Choose it in **Settings → Copy & export**, or for exported files in **Export → Figure → Background**. ReShiki remembers both choices.

| Background  | Result                                                                                  |
| ----------- | --------------------------------------------------------------------------------------- |
| Transparent | No background. Slides and documents show through. The default for copies.               |
| Canvas      | The drawing as you see it, on its own canvas color. The default for exported files.     |
| White       | White paper, for journals and posters. A drawing on a dark canvas switches to dark ink. |
| Black       | Black paper, for dark slides. Black ink switches to white.                              |
| Custom      | The color chosen under **Settings → Copy & export → Your colors**.                      |

**Custom** is one color for both copied pictures and exported files. Choosing a saved color makes it the copy background and changes the color of exports set to **Custom** too.

On **White**, **Black** or **Custom**, black and white ink swap when that makes the drawing readable: dark ink on a light background, light ink on a dark one. Every other color stays as the drawing's canvas shows it: colored atoms, palette colors and custom colors. 3D models keep the drawing's color theme. **Canvas** keeps the drawing's own colors, and **Transparent** keeps the canvas ink, as copies always did.

**Settings → Copy & export** shows a **Preview** of the current drawing as copied and as exported, each on its background. The **Export** tab shows the exported picture above the **Export** button, with its size. The previews come from the same renderer as the files, with the same ink swap. A checkerboard marks transparent areas.

To copy a picture on another background once, right-click the drawing and choose **Copy image on**, then a background or one of your saved colors. A tick marks your default. **Editable drawing**, below the colors, starts at your default each time the menu opens. Click it to change it for this copy: the menu stays open, and **(not default)** shows while it differs. **Change the default…** opens **Settings → Copy & export**.

The background applies to PNG, SVG, PDF, **Export pages as PDF** and EMF for Office. On Windows, the Office preview that **Copy** adds uses the copy background too. When Office rebuilds it after you edit the embedded drawing, it uses the copy background set at that time.

## PNG resolution

Copied pictures and exported files each have their own PNG resolution: 150, 300, 600 or 1200 dpi. Copies default to 300 dpi, which keeps them small and quick to paste into slides and chats. Exported files default to 1200 dpi, which suits journal line art. Choose them in **Settings → Copy & export → PNG resolution**, or for exported files in **Export → Figure → Resolution** when **PNG** is chosen. ReShiki remembers both choices. The resolution is an app preference. It does not change the drawing or its drawing style.

The size in centimeters is the same at every resolution: a higher resolution puts more pixels into the same printed size, and the PNG records its resolution so other apps place it at that size. The **Resolution** menu shows the drawing's size in centimeters and its pixel size at each setting. In Settings, close-ups show part of the current drawing at each resolution, made by the same renderer as the exported file and shown at the same size, so you can see what a lower resolution loses. When the drawing is empty, they show a sample structure on the drawing's canvas. On PNG, the **Export** tab's preview shows the file's pixels at the chosen resolution, scaled to fit.

## Bonds in vector files

SVG, PDF and EMF draw plain bonds as stroked lines in the drawing's line width. Change the stroke width of a pasted or opened figure in Adobe Illustrator or Inkscape, and every plain bond follows, not only the inner lines of double bonds. Bonds that meet at an unlabelled carbon form one line with a mitred corner, so corners and ends look the same as on the canvas and in the PNG. Lines end flat at labels and chain ends.

Wedges, hashed wedges and bold bonds stay filled shapes, because their width varies or is set by the bold width. Where a plain bond meets one of them, a small filled corner completes the mitre. Where bonds of two colors meet, each line stops just past the corner, and the corner stays filled in its own color. Corners sharper than about 29° keep their filled tip, so those two lines do not join. Bonds between atoms that rear opacity fades stay filled too, so overlapping lines do not darken where they meet.

## Text in SVG files

SVG export draws text as outlines by default. The outlines have the same glyphs, positions and sizes as the PNG, so labels look the same in every app, including apps and computers that do not have the drawing's font. The file keeps the drawing's physical size.

Turn on **Keep text editable**, below the export button when **SVG** is chosen, to write text as `<text>` elements instead. You can then edit and search the labels in an illustration app, but each app draws them with its own copy of the font (usually Arial), and an app without that font substitutes another one, so labels can shift or, in some viewers, disappear. The option is off by default, and ReShiki remembers your choice. **SVG for Office** always draws outlines, and so does the SVG that **Copy**, **Copy image** and **Copy as → SVG** put on the clipboard, on every platform.

## Keep the drawing editable inside a picture

A PNG, SVG or single-figure PDF from ReShiki can carry the ReShiki drawing inside the file. Paste that picture into ReShiki, drop the file on the window, or open it with **File → Open**, and you get the editable drawing back instead of a picture. **Import → Paste picture** still inserts raster images instead of restoring their drawing. SVG and PDF without a ReShiki drawing cannot be imported.

- **Put the editable drawing in pictures** in **Settings → Copy & export** is the default for copies and files. It is on by default, and ReShiki remembers your choice.
- With it on, **Copy**, **Copy image** and **Copy as → PNG / SVG / PDF** put the drawing in their PNG, SVG and PDF pictures when it fits the limit, and **file export** adds it to PNG, SVG and single-figure PDF files.
- With it off, copied and exported pictures carry no drawing. On Windows, **Copy** then pastes into Word and PowerPoint as a picture, not as an embedded ReShiki drawing. Copying between ReShiki windows still keeps the drawing, because ReShiki puts its own copy of it on the clipboard. The browser editor can keep the drawing only inside the copied picture, so with the switch off, copies pasted into another ReShiki tab have no drawing either.
- To change it for one copy, right-click the drawing, choose **Copy image on**, click **Editable drawing**, then a background. **Cmd/Ctrl+C**, **Copy image** and **Copy as** always follow the default.
- **Include editable drawing**, below the export button on the **Export** tab, starts at the default each time the tab opens and applies to exports from the tab. Exports from the command palette follow the default. The line under it shows the default. When you change it, **Restore default** appears beside it to go back.
- **Export pages as PDF** never includes the editable drawing, because page edges can clip parts of the drawing. **EMF for Office** does not carry it yet.

The drawing inside a picture holds exactly what the picture shows: the selected objects for a copy of a selection, and the whole drawing for a file export. It has no file paths, user names, recent colors, import notes, page setup, recovery data or Undo history. Anyone who gets the picture can open that drawing in ReShiki, so leave the editable drawing out of figures you send to a journal or publish. In a PNG, the drawing is a text chunk after the image data. PNG files from older versions, which put it before the image data, still open as drawings.

A drawing larger than 2 MB compressed, usually one with large embedded photos, is not put in pictures. The status bar says so, and the file or clipboard picture is still written. Normal **Copy** keeps the editable drawing for ReShiki either way.

Apps that re-encode a picture remove the drawing. These include most web apps and Office's **Compress Pictures**. A PNG whose drawing comes from a newer ReShiki, or is damaged, comes in as a picture with a note that says why. An SVG or PDF with an unreadable drawing is refused with the reason.

## EMF pictures for Office on Windows

Choose **EMF for Office**, export the file, then insert it as a picture in Word, PowerPoint or Excel. Bonds, shapes and outlined text retain vector quality and the figure's physical publication size. File dimensions do not depend on the recording display; intrinsic picture bounds can round outward by up to 0.02 mm. Existing raster pictures remain raster pictures. EMF+ readers preserve their transparency; older readers using only the GDI fallback can make raster transparency opaque. The file contains the whole drawing, cropped to its artwork, including the canvas background. It does not contain an editable ReShiki document; keep the `.rsk` original separately.

EMF file export is available on Windows only and supports figures up to 40 inches in either dimension. Use PDF or SVG for larger drawings. On Windows, you can also import EMF pictures; see [pictures](pictures.md). This option does not change SVG file export or the transparent editable Office clipboard workflow.

[EMF output examples and verification](changes/emf-file-export.md).

## Large PNG drawings

Large drawings, including the complete shortcut gallery, may exceed the image memory limit at the chosen resolution. File export then uses the highest lower resolution that fits, from 600, 300, 150, 96 and 72 dpi, with a limit of 80 million pixels and 32,767 pixels on a side. It never uses a resolution above the one you chose. This changes pixel density, not the physical dimensions, drawing content or the `.rsk` file. The **Resolution** menu and the **Export** tab's preview then show the resolution used, for example _7087 × 3544 px at 600 dpi_.

The saved-file status reports the actual width, height and DPI. Use SVG or PDF if you need vector quality at any magnification. Exceptionally large drawings that exceed the limit even at 72 dpi still require a vector format. On macOS and Linux, a copied PNG keeps the copy resolution. A drawing too large for it gets no PNG, and the notice names that resolution, for example "too large for a 300 dpi PNG". On Windows, a copied PNG uses a lower resolution when it would not fit the clipboard. See [clipboard support](clipboard.md).

## Drawings with unresolved chemistry

Molecular analysis and figure rendering have different requirements. A visible aromatic ring, variable group or unfinished structure can be drawn even when the chemistry engine cannot assign a molecular identity.

Figure export refreshes computed labels when possible. If chemistry analysis fails, it exports the existing drawing and shows a review notice after saving. It does not infer a charge, replace aromatic bonds, remove objects or modify the open document. Invalid drawing geometry or references still prevent export. Chemical data exports such as SMILES and InChI retain their own validation requirements.

[Before/after evidence and desktop verification](changes/figure-export.md).
