# Windows

Use the Windows x64 or ARM64 setup or portable ZIP. Windows ARM requires
Windows 11. The app and chemistry tools run natively and work offline.

## Keyboard and files

Windows uses Ctrl for application shortcuts: Ctrl+N/O/S for new/open/save,
Ctrl+W to close a tab, Ctrl+Tab or Ctrl+1–9 to switch tabs, Ctrl+Z to undo,
Ctrl+Y or Ctrl+Shift+Z to redo, and Ctrl+A/C/X/V to select, copy, cut and
paste. Ctrl+Shift+G ungroups; Ctrl+G groups. Alt allows free bond drawing and
moves without smart guides. Escape leaves the current tool or cancels a draft.

Use `.rsk` for an editable original, including pages, pictures and styles.
SVG/PDF/PNG and EMF export figures; chemical interchange formats retain their supported
subset. Windows file dialogs support paths containing spaces and Unicode.

**Export → Figure → EMF** saves a vector picture for Microsoft Office with
outlined text, the canvas background and the drawing's physical dimensions.
Keep the `.rsk` file to edit the original drawing. See [figure export](figure-export.md)
for format details and limits.

## Clipboard

Normal Copy/Paste within ReShiki retains editable atoms, bonds, captions, arrows,
graphics, pictures and groups. Cut removes the selection only after the clipboard write
succeeds. Ctrl+Shift+C copies an image; **Import → Paste picture** explicitly pastes a picture.

Windows Copy provides native ReShiki data, supported editable interchange data
and an editable Office object containing the complete drawing and a preview.
The editable preview uses vector paths for bonds and outlined text, preserving
smooth edges when resized and transparency over colored pages and slides.
Copy Image provides PNG, SVG, PDF and a standard bitmap. PNG preserves
transparency and resolution; the standard bitmap uses a white background.
PNG file export also uses a white background.
Imported external formats remain subject to the [clipboard limits](clipboard.md).

To copy an embedded ChemDraw structure from PowerPoint, select the whole object
on the slide and press **Ctrl+C**, then press **Ctrl+V** in ReShiki. Objects
containing supported ChemDraw CDX import as editable atoms and bonds. A picture
alone remains a picture; **Import → Paste picture** explicitly chooses the preview.
See the [validation and review checklist](changes/windows-chemdraw-paste-2026-10-05.md)
for this import path and its limits.

### Edit a drawing in Office

1. Select objects in ReShiki and use **Ctrl+C**.
2. Paste into desktop Word, PowerPoint or Excel. If necessary, use **Paste
   Special → ReShiki drawing object**.
3. Double-click the drawing to open it in ReShiki. Keep the Office document open.
4. Edit and press **Ctrl+S**. ReShiki confirms when Office accepts the update.
5. Close the ReShiki editing window and save the Office document.

The Office file contains the drawing; it does not depend on an external
`.rsk` file. ReShiki must be installed to edit it. The installer registers
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

The `.rsk` file format works on both platforms. For a document that needs
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
substantially more work in drawing and layout. Measure idle CPU after the
drawing and chemistry finish loading.

Windows includes both Iced's WGPU renderer and its Tiny Skia software fallback.
At startup, ReShiki enumerates WGPU adapters. If none are available, or all are
CPU adapters such as Microsoft's WARP, it selects Tiny Skia directly. Otherwise
Iced tries WGPU first and falls back to Tiny Skia if initialization fails.
An unclassified adapter is allowed to try WGPU. Other platforms retain their
existing WGPU configuration. GPU rendering accelerates the interface and canvas;
it does not move chemistry calculations or file export onto the GPU.

To inspect the adapters and automatic startup preference in PowerShell:

```powershell
.\reshiki.exe --graphics-info | Write-Output
```

This reports adapter discovery, not successful window or device creation. For
an explicit comparison, set `ICED_BACKEND` before launching the same release
build. `wgpu` forces WGPU (including WARP when that is the only adapter), while
`tiny-skia` forces software rendering. `wgpu,tiny-skia` tries both in that order.
An explicit preference bypasses the automatic CPU-adapter check.

```powershell
$env:ICED_BACKEND = 'wgpu'
.\reshiki.exe
# Close that test window before starting the comparison.
$env:ICED_BACKEND = 'tiny-skia'
.\reshiki.exe
# Restore automatic selection for subsequent launches in this shell.
Remove-Item Env:ICED_BACKEND
```

`WGPU_BACKEND` also constrains adapter discovery, matching Iced's WGPU backend
selection. Software-only VMs cannot establish a hardware acceleration speedup;
measure on a machine that exposes a hardware adapter to Windows as well.

For a repeatable desktop CPU comparison, run this from an interactive Windows
session after the release build finishes:

```powershell
.\scripts\benchmark_windows_renderer.ps1 -Executable .\target\release\reshiki.exe
```

The script opens isolated copies of the built-in shortcut drawing, samples idle
CPU, then moves the pointer and scrolls over the canvas at a fixed cadence. It
runs software and WGPU twice in opposite order plus one automatic-selection
trial. Results and desktop captures go under
`artifacts/windows-renderer-benchmark`. It temporarily controls the pointer and
foreground window, then restores them and the previous environment settings.
CPU percentages are normalized across all logical processors. This measures
CPU cost for the input workload, not achieved frame rate or input latency.
See the [Windows VM validation](windows-gpu-validation.md) for measured results
and their limits.

The idle timer correction from v0.4.0 remains in place. The original Windows
software-only choice avoided expensive editing through WARP; it was separate
from the v0.3.0 idle-redraw issue that continuously used about 89% CPU on the
earlier test VM.

Windows clipboard, printing and process detection are implemented in Rust and
linked into the application. No .NET runtime, C# compiler or extra native helper
is needed. The editor forbids unsafe Rust; Win32 FFI is isolated in
`native/windows`, with bounded data and ownership guards.

The experimental `reshiki.exe --cli` commands follow the same rule as
`--graphics-info`: release builds have no console, so their output and error
messages appear only when piped or redirected, for example
`.\reshiki.exe --cli info | Write-Output` or
`.\reshiki.exe --cli convert --smiles CCO -o ethanol.mol 2> errors.txt`. Piping
also makes PowerShell wait for the command to finish. MCP clients connect pipes
when they start `reshiki.exe --mcp`, so the MCP server is unaffected. See
[Connect AI agents](https://reshiki.com/guide/agents/).

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

The earlier worker-based Windows x64 package was extracted outside the checkout
and checked for missing-uv guidance, first-use setup and offline reuse. Its installer
was checked for installation, in-place upgrade, chemistry and uninstallation
without removing user data. The live assistant connected and produced a
validated ethanol drawing through the user's installed Codex executable.

Local desktop checks use Windows 11 x64. ARM CI does not establish graphical
acceptance on ARM hardware. Windows 10, physical printers, other display scales and
every external application's clipboard behavior require testing on those
systems. See the [visual Windows guide](/guide/windows/) for screenshots.
