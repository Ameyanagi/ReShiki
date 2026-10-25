# Keyboard hotspot navigation and large-gallery hover (#572, #573)

Under review: fixes for [#572](https://github.com/Ameyanagi/ReShiki/issues/572)
and [#573](https://github.com/Ameyanagi/ReShiki/issues/573), by
[@Ameyanagi](https://github.com/Ameyanagi) in
[PR #578](https://github.com/Ameyanagi/ReShiki/pull/578).

The source and renderer checks are complete. The native continuous-hover
comparison is awaiting explicit authorization for the reviewed input replay
method; it has not been executed. The measurements below establish CPU work,
not native event-to-paint latency or the final moving-pointer acceptance result.

## Sources and fixture

- Baseline production source: `3d80590a41b9d9e8efd818d23395ac51bd264ca8`.
- Fixed production source, regression tests and renderer driver:
  `2a6d007415ccd1376aae1920187841ddebfb05e2`.
- Matched PNG capture commit: `324f0df53547e4e3c3f9895c90c936e5fdd90023`.
  Repeating the renderer on the final production source above produces the same
  four published fixed images byte for byte; its 151 targets and five sampled
  hover frames also retain exact baseline parity.
- Exact issue-linked [tiled16-v26.rsk](https://gist.githubusercontent.com/Ameyanagi/0f2d480dd9a4e8cb1843ecd12660f93f/raw/tiled16-v26.rsk):
  5,796,846 bytes, SHA-256
  `71229e202a3a0ba5765af5ac4bd94f4396613237f421cb4c828925c589b553da`.
  It contains 7,728 atom records, 6,864 bonds, 256 abbreviations, 2,112
  annotations and 16 arrows. No regenerated approximation was used for the
  published measurements or captures.

The baseline worktree contains only the same opt-in profiling and renderer
drivers, with no production changes. The fixed worktree starts from the same
main commit; it does not include the other QA branches.

## Ownership and hit behavior

Wheel navigation resolves a pointer-owned hotspot at the point now under the
pointer, using the current zoom's picking radius. Arrow navigation, accepted
edits and history restoration own their target until actual pointer motion or
a click transfers it back. Scrolling still updates the cached pointer's world
coordinate while retaining a keyboard-owned target, so moving back over the
pointer's former world position is recognized as real motion. Wheel and native
pointer-motion packets carry canvas-relative screen positions until the app
consumes them. Mixed pan/zoom events and subsequent pointer motion in one
Iced event batch therefore resolve through the current camera. Programmatic
world-coordinate edits and gesture hints retain their existing behavior.

Pan and zoom preserve the document, selection, revision and Undo/Redo history.
The existing [stationary-pointer rule](../3d-keyboard-drawing.md) and the new
wheel-navigation reviewer rows document that distinction.

Profiling identified exhaustive nearest-bond work and the collapsed-label
fallback as the main hover costs. The fix checks geometric distance before
group/model visibility, indexes atom positions once per bond query, and uses
the existing conservative label reach before laying out a distant collapsed
label. Final hit boxes, strict radius comparisons, tie order and reverse group
priority are unchanged. The per-query indexes preserve the first matching ID
even for an unvalidated document with duplicate IDs.

## Matched renderer evidence

Both builds use the exact fixture, JACS / ACS, Publication colors, a light
canvas, Select, keyboard drawing on, inspector closed, and a 1600 × 968 pt
client viewport. That is the client area of the issue's 1600 × 1000 pt macOS
outer window, allowing for its 32 pt title bar. PNG output is 1× and comes
directly from the actual widget/canvas renderer, without retouching.

The canvas crosshair is enabled in both renderer runs to expose the pointer
coordinate. Fit is 4.050719%; twelve actual wheel events then reach 104% at
outer-window point (439, 241), followed by twenty pan ticks of (12, 6) logical
pixels. The wheel delta is calibrated to 104% from the measured Fit value;
the issue's macOS physical scroll-script pixels are not equated with these
Iced logical input units.

| State                                 | Baseline                                                                                 | Fixed                                                                                | Check                                                                                           |
| ------------------------------------- | ---------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------- |
| Fit, initial hover                    | ![Baseline Fit with C 28](../images/pointer-hover/before/fit-hover.png)                  | ![Fixed Fit with C 28](../images/pointer-hover/after/fit-hover.png)                  | Same camera and initial C #28 hotspot                                                           |
| After wheel zoom to 104%              | ![Baseline target after wheel zoom](../images/pointer-hover/before/after-wheel-zoom.png) | ![Fixed target after wheel zoom](../images/pointer-hover/after/after-wheel-zoom.png) | Baseline retains C #28 near the context row; fixed resolves the empty position at the crosshair |
| During wheel pan                      | ![Baseline target during wheel pan](../images/pointer-hover/before/during-wheel-pan.png) | ![Fixed target during wheel pan](../images/pointer-hover/after/during-wheel-pan.png) | Baseline target travels with the drawing; fixed target stays under the pointer                  |
| Synchronously rendered hover midpoint | ![Baseline synchronous hover midpoint](../images/pointer-hover/before/hover-075.png)     | ![Fixed synchronous hover midpoint](../images/pointer-hover/after/hover-075.png)     | Exact same target and pixels; this static parity frame is not a latency measurement             |

The 151-position ellipse has the same target identities and blank coordinates
in both builds. Its PNG frames at positions 0, 38, 75, 113 and 150 are
byte-for-byte identical. This checks hit/render preservation separately from
the ownership change and runtime performance.

## Quiet optimized profiling

Actual host: Apple M1, arm64, macOS 27.0.1 (26A434), Rust 1.99.0. This is a
different machine from the report's Apple M4. Both executables use the release
profile, three warm-ups and thirty measured samples. The saved baseline and
final fixed benchmark executables ran back-to-back after the other QA workers
held compiler, GPU and native input work. Process-state receipts confirm no
Cargo or rustc process at that boundary.

Six query points cover an atom, a collapsed group label, a bond, the reported
atom 28, nearby empty space and off-drawing empty space. The complete update
uses the baseline's world-coordinate `Edit::Hover` and the fixed native
`Edit::PointerHover`, including its current-camera conversion, through
`App::update`. Both drivers normalize the same screen-derived physical query
positions and compare hit identities. The rendering workload
also rebuilds the real widget view, reconciles its tree, lays it out, records
drawing commands and resolves mouse interaction. It does not present a native
frame or run the returned tasks. Independently measured phase medians should
not be summed.

| Workload                                         | Before median / p95 (ms) | Fixed median / p95 (ms) |
| ------------------------------------------------ | -----------------------: | ----------------------: |
| Atom label ink hit                               |          0.2625 / 0.3905 |         0.2715 / 0.3948 |
| Nearest atom, including collapsed-label fallback |        11.1809 / 11.2188 |         0.2043 / 0.2797 |
| Nearest bond                                     |        34.7414 / 35.1962 |         0.3784 / 0.3826 |
| Complete app hover update                        |        46.0600 / 46.4589 |         0.7715 / 0.9936 |
| View, layout, drawing commands and interaction   |        11.8475 / 12.2127 |       11.6824 / 11.8790 |
| Hover update through drawing commands            |        58.2897 / 58.9928 |       12.6085 / 12.9743 |

The last row is about 4.6× faster on this host. It does not establish a native
frame rate. [Machine-readable timings and executable hashes](qa-pointer-hover-timings.json)
retain the exact measurement scope, query identities and process receipts.

To reproduce, set `RESHIKI_PERF_HOVER_DRAWING` to the exact downloaded fixture
and run:

```sh
cargo test --release --bin reshiki app::performance::hover::hover_workloads --locked -- --ignored --exact --nocapture --test-threads=1
```

The default benchmark can generate a 16-copy gallery when that variable is
absent; that generated layout was not used for the reported comparison.

## Native preparation and remaining check

Both isolated QA bundles contain optimized executables and use separate
application-data directories. The baseline executable SHA-256 is
`a51637f61cec1113c885d9c1f431b8f5c662d3f61b6de8204c47139fa0fc3f1d`;
the fixed executable SHA-256 is
`9f7356cd82cdefea0adc0b8f7cfe36ed67acb8fcb44cf8e106819ece6f71e315`.

The supported native AX Zoom action yields an observed 1680 × 1020 pt outer
window, with 3360 × 2040 pixel screenshots at 2×. The exposed CUA edge/corner
drag actions did not produce a 1600 × 1000 pt window. Both native runs will use
the same observed viewport, and disclose this difference from the report.
The supported command-palette result **Zoom to 104%** sets the exact scale;
no Cmd-scroll capability is assumed for the UI automation tool.

Native horizontal CUA scroll did not change the camera. A reviewed helper has
separate fixed camera-setup and replay modes. Setup consists of fifty bounded,
unmodified pixel scrolls over about 1.6 seconds; replay is the same 151
mouse-moved events over five seconds. It accepts only the named isolated QA
executables and observed process/window/bounds, requires that exact window to
be frontmost, aborts on held buttons/modifiers or changed identity/geometry,
and never posts clicks, keys or permission requests. Default and dry-run
invocations are noninteractive. Its execution remains pending the explicit
method exception; no native moving-pointer result is claimed here yet.

## Verification

- Model suite: 803 passed, 10 existing opt-in tests ignored.
- App suite: 1,119 passed, 123 opt-in tests ignored, using the freshly built
  native naming helper.
- Keyboard suite: 27 passed, one existing renderer test ignored; this includes
  eight camera/ownership regressions and the stationary-pointer case.
  Two immutable-view input tests cover mixed wheel packets and wheel-to-motion
  packets before any app message is consumed, at nonzero offsets and 0.5×/2×.
- Independent old atom, bond and collapsed-label queries agree with the
  optimized queries across gallery labels, strict radii, ties, duplicate IDs,
  missing endpoints and hidden 3D models.
- Matched baseline/fixed actual-renderer drivers pass, including unchanged
  document/selection/history and the 151-position parity trace.
- Strict native bin/test Clippy and the mandatory three-package WebAssembly
  Clippy pass. Normal commit hooks pass.

Reusable renderer caption: **After wheel navigation, a pointer-owned keyboard
hotspot resolves under the pointer. Keyboard-selected targets remain chosen
until actual pointer motion or a click takes over.**

Reusable measured-performance caption: **On the exact 7,728-atom gallery,
optimized M1 CPU work from hover update through widget drawing commands falls
from 58.3 ms to 12.6 ms; native presentation latency is a separate check.**
