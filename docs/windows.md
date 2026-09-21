# Windows

Use the Windows x64 setup or portable ZIP. Windows ARM requires Windows 11 and
uses x64 emulation for the local RDKit chemistry worker. Install uv before first
use; ReShiki prepares Python and chemistry packages automatically.

## Keyboard and files

Windows uses Ctrl for application shortcuts: Ctrl+N/O/S for new/open/save,
Ctrl+Z to undo, Ctrl+Y or Ctrl+Shift+Z to redo, and Ctrl+A/C/X/V to select,
copy, cut and paste. Ctrl+Shift+G ungroups; Ctrl+G groups. Alt allows free bond
drawing. Escape leaves the current tool or cancels a draft.

Use `.reshiki` for an editable original, including pages, pictures and styles.
SVG/PDF/PNG export figures; chemical interchange formats retain their supported
subset. Windows file dialogs support paths containing spaces and Unicode.

## Clipboard

Normal Copy/Paste within ReShiki retains editable atoms, bonds, captions, arrows,
graphics, pictures and groups. Cut removes the selection only after the clipboard write
succeeds. Ctrl+Shift+C copies an image; Ctrl+Shift+V explicitly pastes a picture.

Windows Copy provides native ReShiki data, supported ChemDraw interchange data
and an editable Office object containing the complete drawing and a preview.
The editable preview uses vector paths for bonds and outlined text, preserving
smooth edges when resized and transparency over colored pages and slides.
Copy Image provides PNG, SVG, PDF and a standard bitmap. PNG preserves
transparency and resolution; the standard bitmap uses a white background.
PNG file export also uses a white background.
Imported external formats remain subject to the [clipboard limits](clipboard.md).

### Edit a drawing in Office

1. Select objects in ReShiki and use **Ctrl+C**.
2. Paste into desktop Word, PowerPoint or Excel. If necessary, use **Paste
   Special → ReShiki drawing object**.
3. Double-click the drawing to open it in ReShiki. Keep the Office document open.
4. Edit and press **Ctrl+S**. ReShiki confirms when Office accepts the update.
5. Close the ReShiki editing window and save the Office document.

The Office file contains the drawing; it does not depend on an external
`.reshiki` file. ReShiki must be installed to edit it. The installer registers
the editor, and the portable version registers its current location when you
copy. After moving a portable installation, copy once from its new location.
If Office closes or rejects an update, ReShiki reports the error and retains
a local copy. Use **Save as** to keep a separate editable original.

Use **Copy Image** for a static figure or for applications that only accept
pictures. Its SVG outlines text so Office cannot shift atom labels. Existing
pictures are not converted into editable chemistry by installing this update.
To refresh an existing embedded ReShiki object's older bitmap preview,
double-click it, press **Ctrl+S** in ReShiki, then save the Office document.

Excel adds its own white fill and outline to newly pasted OLE objects. Select
the object, press **Ctrl+1**, and choose **Colors and Lines → Fill → No fill**
to show worksheet cells through the transparent preview. Its outline is also
controlled by Excel. ReShiki's preview itself contains no background fill.

### Share with a Mac

The `.reshiki` file format works on both platforms. For a document that needs
editing on a Mac, keep that original alongside the Office document and replace
the figure after editing it. The Windows OLE double-click workflow is not
available in Mac Office. ReShiki's macOS clipboard, native printing and renderer
remain separate from the Windows implementation. See [platform boundaries](clipboard.md#office-and-macos).

## Printing

Use Page setup to choose the paper, orientation, margins and page grid, and
preview the drawing. Ctrl+P or Export → Print opens the Windows print dialog.
Choose the printer, page range, copies and driver preferences. Microsoft Print
to PDF produces a local PDF when that Windows printer is installed. Print
selection uses a separate snapshot of the selected objects.

The drawing retains publication dimensions. Text prints as vector outlines,
and pictures preserve their placement and transparency. Export PDF when you
need the application's embedded-font PDF rather than printer output. Cancelling
printing leaves the drawing, selection and undo history intact.

## Release performance and debugging

Use `cargo run --release --locked` to measure performance. Debug builds perform
substantially more work in drawing and layout. First chemistry setup may also
download dependencies. Measure idle CPU after the drawing and chemistry finish
loading, separately from that initial setup.

The idle timer correction is included in v0.4.0. Windows additionally uses
Iced's Tiny Skia renderer, avoiding GPU emulation on virtual machines and remote
desktops. Other platforms retain WGPU. On the tested Windows VM, the downloaded
v0.3.0 continuously used about 89% CPU. WGPU also became expensive during editing
with Microsoft's WARP adapter. Direct software rendering avoids that path.

Windows clipboard, printing and process detection are implemented in Rust and
linked into the application. No .NET runtime, C# compiler or extra native helper
is needed. The editor forbids unsafe Rust; Win32 FFI is isolated in
`native/windows`, with bounded data and ownership guards.

## Verification scope

Windows integration tests exercise the real clipboard, editable and image
round-trips, standard bitmap resolution, malformed data, active-process
detection and the actual Windows print renderer's physical dimensions and
placement. The full shared test suite covers drawing, chemistry, templates,
labels, reactions, graphics, pictures, page layouts, file formats, recovery and
asynchronous assistant state. Windows x64/ARM and macOS Rust checks and Python
tests passed for the vector-preview change in
[this CI run](https://github.com/Ameyanagi/ReShiki/actions/runs/35558543181).

Desktop Word, PowerPoint and Excel were checked with normal Copy/Paste,
double-click editing in ReShiki, and Ctrl+S updates accepted by Office. Saved
documents retain the native drawing and a transparent vector preview. Enlarged
figures were inspected in all three applications; Excel's separate object fill
was set to No fill for the transparency check.

The Windows x64 package was extracted outside the checkout and checked for
missing-uv guidance, first-use chemistry setup and offline reuse. Its installer
was checked for installation, in-place upgrade, chemistry and uninstallation
without removing user data. The live assistant connected and produced a
validated ethanol drawing through the user's installed Codex executable.

Local desktop checks use Windows 11 x64. ARM CI does not establish graphical
acceptance on ARM hardware. Windows 10, physical printers, other display scales and
every external application's clipboard behavior require testing on those
systems. See the [visual Windows guide](/guide/windows/) for screenshots.
