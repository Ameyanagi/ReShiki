# Windows window fitting (#108)

The Windows editor now fits its initial decorated window inside the selected
monitor's work area. It keeps the normal DPI scale and preferred 1280 × 820
logical client size when those dimensions fit. On smaller work areas, it lowers
both the initial size and the 1040 × 680 logical minimum as needed.

The hidden HWND supplies its actual physical client/frame bounds, work area and
per-window DPI. The fit reserves an eight-logical-pixel margin, changes size and
minimum constraints through Iced, and places the outer frame using physical
screen coordinates. Accessibility installation and initial visibility wait for
the fit. A DPI change fits the current window without resetting it to the
preferred startup size. Negative monitor origins and side taskbars are covered.

## Reproduction and evidence

Base: `51fa0991`. The unchanged source requests 1280 × 820 logical pixels.
At the issue's reported 200% scale, that is a 2560 × 1640 physical client on a
2256 × 1504 screen, already too large before adding window decorations. This
source-derived witness confirms the size defect; it is not a desktop screenshot
or a measurement of the reporter's taskbar/frame dimensions.

The regression test also models a 2256 × 1408 work area with 32 × 78 physical
decoration pixels at 192 DPI. The fitted logical client is 1096 × 649 and the
minimum is 1040 × 649. These frame/taskbar values are test inputs, not measured
values from the issue.

The unchanged Windows executable is retained separately from the candidate:
SHA-256 `135779960a15c17bac51e00c614600422531a30b90480d6cb1ba8de5b795e51d`.
Both use Rust/Cargo 1.99.0, the x64 MSVC toolchain, no incremental compilation,
and dev-profile debug information disabled. Baseline `--graphics-info` reports
Microsoft Basic Render Driver/DX12, a CPU device and automatic `tiny-skia`.
Candidate SHA-256:
`b38cac810f799fca1b2ae4e841d4d042c7f01709ef888fbe3feac9d8fbf195ed`.

## Validation

Completed on Windows 11 build 26200:

- `cargo +1.99.0 build --locked --bin reshiki`: passed.
- `cargo +1.99.0 test --locked --bin reshiki window_fit:: -- --nocapture`:
  six tests passed. Coverage includes the reported 200% overflow, roomy and small
  work areas, 100–250% DPI including fractional scales, decorated outer bounds,
  lowered minimums, negative origins, side taskbars, DPI changes, and rejection
  of invalid measurements.
- `cargo +1.99.0 clippy --locked --bin reshiki -- -D warnings` and
  `cargo +1.99.0 clippy --locked -p reshiki-windows --lib -- -D warnings`:
  passed.
- Formatting and diff whitespace checks passed.

**Pending acceptance:** actual matched Windows BEFORE/AFTER screenshots, native
interactive-session measurements, and resize/menu/drawing interaction. The RDP
connection currently awaits its certificate trust decision. No desktop result
is claimed by the source witness or policy tests. This visible fix still needs
the evidence required by [visual-review.md](../visual-review.md).

Capture a fresh, unmaximized empty editor for each executable at the same
resolution, scale, taskbar configuration, renderer settings and framing. First
use 200% DPI where the original window overflows; also check a small logical
work area such as a 1024 × 768 desktop and a roomy 100% desktop. Record the actual
work area rather than assuming the full monitor is available. Close one build
before starting the other and use separate clean data directories.

Run this read-only probe in the same interactive session as the editor:

```powershell
powershell -NoProfile -File scripts/probe_windows_window.ps1 -EditorProcessId <PID> -SourceCommit <COMMIT>
```

Retain its executable hash, DPI, physical outer/client/work bounds and
`FitsWorkArea` result alongside each screenshot. The fixed outer frame must
fit the work area, including decorations; all window controls must be reachable.
Resize down to the lowered minimum, open menus and perform a drawing operation.
When another DPI is available, change DPI or move to that monitor and recheck
containment, UI scale, accessibility and resizing. A same-DPI work-area change
alone is not currently a fit trigger.

Release-note caption: “The Windows editor now fits smaller and high-DPI desktop
work areas while preserving the normal UI scale.”

Image alt text: “The same Windows desktop before and after fitting the ReShiki
window: the fixed window and its controls stay inside the available work area.”
Image links are pending the actual captures.
