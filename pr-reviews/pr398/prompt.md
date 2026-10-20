You are running local browser UI verification for Claude. Use computer use (screen control, mouse, keyboard, screenshots).

Repository (read-only, do NOT edit any files in it): /Users/shiranui/dev/ReShiki/.claude/worktrees/wasm
Artifact directory (write screenshots + report here): /private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev-wasm
Demo server is ALREADY running: http://localhost:63925/examples/  (do not start/stop it)

App: the ReShiki chemical drawing editor compiled to WebAssembly (iced + wgpu), mounted into the #editor div of a demo page. The page header has buttons "Load caffeine", "Save as text" (with a format select), "Export SVG", "Export PNG". The footer shows "N changes" (incremented by the editor's onChange callback), an image preview of exports, and a text output. `window.editor` is the JS API object (load, save, exportImage, onChange, destroy).

BROWSER ISOLATION (mandatory):
- The user has their own Google Chrome open. NEVER interact with, close, or kill the user's Chrome windows or processes.
- Launch a SEPARATE Chrome instance with its own profile:
  open -na "/Applications/Google Chrome.app" --args --user-data-dir=/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev-wasm/chrome-profile --no-first-run --no-default-browser-check --new-window --window-size=1400,1000 --remote-debugging-port=9333 http://localhost:63925/examples/
- Only interact with that window. You may use the DevTools protocol on port 9333 (e.g. via a small script with bun or python websockets, or Chrome's own DevTools window inside THAT instance) to run JS in the page and capture console output, but do the drawing steps with real mouse input via computer use.
- At the end, quit ONLY that instance: pkill -f "user-data-dir=/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev-wasm/chrome-profile" . Do not touch other processes. Do not open the user's installed ReShiki app.

Flow (screenshot at each step into the artifact dir, names given):
1. Open the demo page; wait up to ~60 s for the editor (wasm is ~23 MB). Expect: the full desktop-like editor UI (toolbar, canvas) renders inside the page between header and footer. Screenshot 01-editor-rendered.png. Also capture the browser console output (any errors/warnings) and note whether it used WebGPU or WebGL (console or `!!navigator.gpu && await navigator.gpu.requestAdapter()`).
2. Draw with the mouse: pick the bond tool (or default drawing tool) and click/drag on the canvas to draw a bond; then pick a ring tool (e.g. benzene/cyclohexane) and click on the canvas to place a ring. Expect: bond and ring appear; footer "N changes" increases. Screenshots 02-bond-drawn.png, 03-ring-drawn.png.
3. Undo: click on the canvas area to focus it, press Ctrl+Z (Ctrl, not Cmd - the browser build uses Ctrl even on macOS). Also try Cmd+Z once and note what happens. Expect: the last drawing step is undone. Screenshot 04-after-undo.png.
4. Load caffeine through the JS API from the demo page: click the "Load caffeine" header button (which calls editor.load("CN1C=NC2=C1C(=O)N(C(=O)N2C)C","smiles")). Additionally run in page console: `await window.editor.save("smiles")` and record the result. Expect: caffeine structure shown with N and O atom labels rendered legibly (bundled Liberation Sans font, not tofu boxes). Screenshot 05-caffeine-loaded.png, plus a zoomed/cropped close-up of labels 06-labels-closeup.png.
5. Export SVG: click "Export SVG". Expect: a preview image of caffeine appears in footer. Screenshot 07-export-svg.png. Also via console: `(await window.editor.exportImage("svg")).slice(0,200)` and length; save the full SVG text to export.svg in the artifact dir if you can (e.g. through the DevTools protocol).
6. Export PNG: click "Export PNG". Expect: PNG preview appears. Screenshot 08-export-png.png. Via console: check the result is a Uint8Array starting with PNG signature bytes 137 80 78 71 and its length; save it to export.png if you can.
7. Change callback: record the "N changes" counter before and after steps 2, 3, 4 (it should increase after each edit/load). Also register a second callback via console: `let n=0; const off = window.editor.onChange(()=>n++);` then make one more edit with the mouse (e.g. draw another bond) and check n > 0. Screenshot 09-change-callback.png.
8. Resize the container: resize the browser window substantially smaller (e.g. 900x650) and then larger, and/or via console set `document.getElementById('editor').style.height='300px'` then back to ''. Expect: the editor re-lays out to fill the new size with no blank/stretched/blurred canvas. Screenshots 10-resized-small.png, 11-resized-large.png.
9. Final console log dump (errors/panics) into console.txt in the artifact dir.

Constraints: no system settings changes, no closing other apps, no edits to the repository, no network beyond localhost. If a step cannot be driven, say so and screenshot what you see.

Write report.md into the artifact dir: for each step, what you did, observed vs expected, screenshot filename(s), PASS/FAIL/PARTIAL. Include the WebGPU/WebGL finding, the console errors, the save("smiles") output, the export sizes. End with anything you could not verify and why. Make sure the separate Chrome instance is killed before you finish.
