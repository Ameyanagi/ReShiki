# Browser editor verification (feat/wasm-editor @ d5d1c239), 2026-10-21

Build: `cd /Users/shiranui/dev/ReShiki/.claude/worktrees/wasm && CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=$PWD/target scripts/build_wasm.sh` (build.log; 63 s, cached).
Serve: `cd packages/editor && PORT=63925 bun examples/serve.ts`, open http://localhost:63925/examples/.
Browser: separate Google Chrome 155 instances with throwaway profiles (chrome-profile, chrome-profile2; both killed and deleted). WebGPU (Apple metal-3) was active.

Pass 1 (Codex gpt-6-astra, report.md): page JS + CDP screenshots. Codex's computer-use could not bind to the isolated Chrome (same bundle id as the user's Chrome), so it did no mouse/keyboard input; 02-04 are identical copies of 01.
Pass 2 (Claude, scripts/input.mjs, scripts/label.mjs): trusted CDP Input.dispatchMouseEvent / dispatchKeyEvent (Chrome's input pipeline, not DOM events) and Browser.setWindowBounds. Results in input-results.json.

| File | Shows |
|---|---|
| 01-editor-rendered.png | Editor fully rendered in the page, empty canvas, "0 changes" |
| 02/03/04-*.png | Identical to 01 (Codex pass blocked; not evidence of drawing) |
| 05-caffeine-loaded.png | "Load caffeine" button -> caffeine, C8H10N4O2, 14 atoms/15 bonds, "1 changes" |
| 06-labels-closeup.png | N/O labels in bundled Liberation Sans, no tofu |
| 07-export-svg.png, export.svg | SVG export preview in footer; 3,763-byte SVG |
| 08-export-png.png, export.png | PNG export preview; 117,358-byte PNG 1303x1123, correct signature |
| 09-change-callback.png | 2nd onChange listener fired on reload ("2 changes") |
| 10-resized-small.png | #editor height 300px: editor relayouts, zoom fits (43%), tool palette scrolls |
| 11-resized-large.png | Height restored, editor fills again |
| 12-input-start.png | Pass 2 start, empty |
| 13-input-bond-dragged.png | Bond tool + mouse drag -> bond; save = "CC", "1 changes" |
| 14-input-ring-placed.png | Ring tool + click -> cyclohexane; save = "C1CCCCC1.CC", "2 changes" |
| 15-input-after-ctrl-z.png | Ctrl+Z removes ring; save = "CC", "History restored", "3 changes" |
| 16-input-after-cmd-z.png | Cmd+Z: no effect (known gap, Ctrl-only shortcuts) |
| 17-input-hover-key-o.png | Pressing "o" switched to Atom label tool, Element O (hotkey); no label yet |
| 18-window-900x650.png | Real window resize to 900x650: canvas 1800x921 backing px, relayout OK |
| 19-window-1600x1050.png | Window 1600x1050: canvas 3200x1710, relayout OK |
| 20-input-atom-label-o.png, 21-label-closeup.png | Click bond end with O tool -> "OH" label drawn in bundled font; save = "CO", "4 changes" |
| console.txt, console-events.jsonl | Only error: favicon.ico 404. No panics/exceptions in either pass |
