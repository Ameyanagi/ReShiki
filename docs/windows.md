# Windows development

This page covers Windows builds, renderers and their checks. For using
ReShiki on Windows, see the [Windows guide](https://reshiki.com/guide/windows/);
to set up a Windows build, see [Build and test](development.md).

Windows clipboard, printing and process detection are implemented in Rust and
linked into the application. No .NET runtime, C# compiler or extra native helper
is needed. The editor forbids unsafe Rust; Win32 FFI is isolated in
`native/windows`, with bounded data and ownership guards.

## Release performance and debugging

Use `cargo run --release --locked` to measure performance. Debug builds perform
substantially more work in drawing and layout. Measure idle CPU after the
drawing and chemistry finish loading.

Windows includes both Iced's WGPU renderer and its Tiny Skia software fallback.
Automatic startup uses Direct3D 12 for adapter discovery and WGPU initialization,
avoiding unnecessary initialization of OpenGL and Vulkan drivers. If no adapters
are available, or all are CPU adapters such as Microsoft's WARP, ReShiki selects
Tiny Skia directly. Otherwise Iced tries WGPU first and falls back to Tiny Skia
if initialization returns an error. An unclassified adapter is allowed to try
WGPU. A native graphics driver crash cannot be recovered by this fallback.
GPU rendering accelerates the interface and canvas; it does not move chemistry
calculations or file export onto the GPU.

To inspect the adapters and automatic startup preference in PowerShell:

```powershell
.\reshiki.exe --graphics-info | Write-Output
```

This reports adapter discovery, not successful window or device creation.
`requested_wgpu_backends` lists the APIs used for discovery and allowed when
WGPU runs; `startup_preference` includes any explicit renderer preference.
`automatic_preference` reports the choice based on the discovered adapters.
When an explicit renderer list contains no `wgpu` candidate, discovery is
skipped: `adapter_probe_performed` is false,
`automatic_preference` is null and the adapter and backend lists are empty.
For an explicit comparison, set `ICED_BACKEND` before launching the same release
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
selection; for example, `dx12`, `vulkan`, or `gl,dx12`. Existing values are
preserved. WGPU ignores unknown names; an empty value or a list containing no
recognized names enables no backends. An empty or comma-only `ICED_BACKEND`
uses automatic selection. A nonempty renderer preference bypasses both the
automatic CPU-adapter check and the Direct3D 12 default, retaining Iced's usual
backend selection unless `WGPU_BACKEND` is set. Software-only VMs cannot
establish a hardware acceleration speedup; measure on a machine that exposes a
hardware adapter to Windows as well.

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
See the [Windows VM validation](changes/windows-gpu-validation.md) for measured results
and their limits.

The idle timer correction from v0.4.0 remains in place. The original Windows
software-only choice avoided expensive editing through WARP; it was separate
from the v0.3.0 idle-redraw issue that continuously used about 89% CPU on the
earlier test VM.

The experimental `reshiki.exe --cli` commands (ReShiki 0.12.0 and later) follow
the same rule as `--graphics-info`: release builds have
no console, so their output and error messages appear only when piped or
redirected, for example `.\reshiki.exe --cli info | Write-Output` or
`.\reshiki.exe --cli convert --smiles CCO -o ethanol.mol 2> errors.txt`. Piping
also makes PowerShell wait for the command to finish, and `$LASTEXITCODE` then
holds its exit status. MCP clients connect pipes when they start
`reshiki.exe --mcp`, so the MCP server is unaffected. See
[Connect AI agents](https://reshiki.com/guide/agents/) and the
[command-line reference](agent-cli.md#windows).

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
