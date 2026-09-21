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

The clipboard uses private ReShiki data, the Windows ChemDraw Interchange Format,
PNG/SVG/PDF and a standard CF_DIB bitmap. Paste also accepts Unicode chemical
text, MDLCT and supported registered image formats. Data is decoded and memory
allocated before clearing the clipboard. External object/OLE deserialization is
never used.

Run `cargo test --workspace --locked` and
`cargo clippy --workspace --all-targets --locked -- -D warnings` on Windows.
Clipboard integration tests replace the desktop clipboard with test data.

API references: [Rust for Windows](https://github.com/microsoft/windows-rs),
[clipboard ownership](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setclipboarddata),
[print property sheet](https://learn.microsoft.com/windows/win32/api/commdlg/ns-commdlg-printdlgexw).
