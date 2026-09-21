# Windows integration

This Rust crate uses Microsoft's `windows` bindings to the Windows APIs. It is
linked into ReShiki: no C# compiler, .NET runtime, or helper executable is needed.
The editor keeps `#![forbid(unsafe_code)]`. The FFI required by Win32 is isolated
here, with safe entry points and ownership guards for clipboard allocations,
windows, GDI+ objects, printer settings and device contexts.

Clipboard work and print dialogs run on blocking worker threads, away from the
editor event loop. Printing uses a validated, immutable vector snapshot made
from the same SVG/font layout as PDF export. Glyphs are vector paths. Raster
pictures retain their transforms and alpha. Cancelling leaves the drawing and
undo history unchanged. Page setup in the editor provides the drawing preview;
the Windows dialog selects the printer, pages, copies and driver preferences.

Normal Copy exposes an editable Office OLE object with the native drawing and a
transparent EMF+ dual preview. Bonds and glyphs remain vector paths, including
the GDI fallback used by OLE's presentation cache. Device physical resolution is
used explicitly so remote desktop DPI does not change publication dimensions.
Only imported raster pictures remain raster. Copy Image supplies PNG/SVG/PDF and
a standard CF_DIB bitmap; standalone image formats are omitted from normal Copy
so Word selects the editable object. Paste also accepts Unicode chemical text,
MDLCT, supported images and ChemDraw interchange data.

The same executable runs a separate COM STA server for Office. Double-click
opens a private working document; Ctrl+S validates and renders it, updates the
container, and acknowledges only the bytes accepted by Office. Saved OLE storage
contains native JSON, a PNG fallback and vector preview, so Office can display it
without launching ReShiki. Older PNG-only objects remain readable. Paste reads
only this application's CLSID and bounded streams; foreign OLE objects are not
activated or deserialized.

Run `cargo test --workspace --locked` and
`cargo clippy --workspace --all-targets --locked -- -D warnings` on Windows.
Clipboard integration tests replace the desktop clipboard with test data.

API references: [Rust for Windows](https://github.com/microsoft/windows-rs),
[clipboard ownership](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setclipboarddata),
[print property sheet](https://learn.microsoft.com/windows/win32/api/commdlg/ns-commdlg-printdlgexw).
