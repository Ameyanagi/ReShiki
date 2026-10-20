**ReShiki WebAssembly browser verification — overall PARTIAL**

Verified on 2026-10-21 JST (2026-10-20 UTC) at `http://localhost:63925/examples/`. The full editor, caffeine rendering, serialization, both exports, load callbacks, and container-height resizing worked. Real mouse drawing and physical keyboard shortcuts could not be verified safely because the available macOS computer-use API bound to the user's original Chrome process instead of the separately launched test process.

**Isolation and method**

Launched the exact requested separate Chrome instance with `chrome-profile` inside this artifact directory, a 1400×1000 window, and remote debugging port 9333. The page viewport was 1400×913 CSS pixels with device pixel ratio 2. All page evaluations and captures targeted the demo page discovered through localhost port 9333.

The computer-use inventory and application binding were inspected before any input. Both bundle-ID and application-path bindings showed the user's original New Tab window. The browser connector likewise exposed only that original profile, and this macOS computer-use surface offered no window-ID inventory/selection API to select the isolated process. No mouse/keyboard actions were sent to either Chrome process. No original Chrome window or tab was changed or closed. The installed ReShiki application was not opened or controlled. Drawing was not simulated with DOM events or CDP input.

The authorized page-JavaScript path was used to activate the demo's Load caffeine / Export SVG / Export PNG DOM buttons (`element.click()`), inspect the public API, and change container height. These are programmatic button activations, not real mouse clicks. All PNG screenshots are genuine CDP page captures of the isolated demo; screenshot 06 is a clipped page capture. They exclude browser chrome. The repository was read only, and the existing server was neither started nor stopped. Agent-initiated network requests used localhost only.

**Results by requested step**

1. **Open and render — PASS for rendering/backend; startup duration not measured precisely.** The editor was already ready at the first explicit page inspection. Toolbar, drawing canvas, side panels, status bar, page header, and footer rendered fully in `#editor`. Initial footer: `0 changes`. The canvas measured 1400×835 CSS pixels with a 2800×1670 backing buffer. Screenshot: [01-editor-rendered.png](01-editor-rendered.png).

   WebGPU was used: `navigator.gpu` existed, `await navigator.gpu.requestAdapter()` returned an adapter (`vendor: "apple"`, `architecture: "metal-3"`), and the already-rendered canvas returned a non-null `getContext("webgpu")`. This confirms the active context, beyond merely detecting adapter availability. WebGL fallback was not exercised. The only captured page log error was a localhost `/favicon.ico` 404.

2. **Draw a bond and ring with real mouse input — PARTIAL / BLOCKED.** The relevant tools rendered, but no safe computer-use binding to the isolated instance was available. No bond or ring was drawn. Footer stayed at `0 changes`; that unchanged value does not test edit notifications. Requested screenshots [02-bond-drawn.png](02-bond-drawn.png) and [03-ring-drawn.png](03-ring-drawn.png) capture the unchanged empty canvas at the blocked steps. Their requested filenames are not evidence that a drawing action occurred.

3. **Ctrl+Z and Cmd+Z — PARTIAL / BLOCKED.** Neither shortcut was sent, and no undo behavior was observed. There was no preceding mouse-drawn structure to undo. Counter remained `0 changes`. Screenshot: [04-after-undo.png](04-after-undo.png), showing the unchanged canvas. Static documentation at `packages/editor/README.md:101` says browser shortcuts use Ctrl on macOS and Cmd does not work yet; this is source evidence only, not a runtime shortcut result.

4. **Load caffeine and inspect labels / SMILES — PARTIAL overall; API and rendering checks PASS.** Activated the demo's Load caffeine button through page JavaScript. Its handler called `editor.load("CN1C=NC2=C1C(=O)N(C(=O)N2C)C", "smiles")`. The structure appeared, and the property panel reported `C8H10N4O2`, 14 atoms, and 15 bonds. N and O labels were clearly readable, with no tofu boxes. Footer increased `0 changes` → `1 changes`. Screenshots: [05-caffeine-loaded.png](05-caffeine-loaded.png), [06-labels-closeup.png](06-labels-closeup.png). The normal visual result passed; physical header-button input was not tested.

   `await window.editor.save("smiles")` returned exactly:

   ```text
   Cn1c(=O)c2c(ncn2C)n(C)c1=O
   ```

5. **Export SVG — PARTIAL overall; export and preview checks PASS.** Activated the Export SVG button through page JavaScript. A visible caffeine preview appeared in the footer; its image had `alt="SVG export"`, `complete=true`, and natural size 104×90. Independently called `window.editor.exportImage("svg")` and saved its full string to [export.svg](export.svg). It contained **3,763 characters / 3,763 UTF-8 bytes**. Footer remained `1 changes`. Screenshot: [07-export-svg.png](07-export-svg.png). Physical button clicking was not tested.

   First 200 characters:

   ```xml
   <svg xmlns="http://www.w3.org/2000/svg" viewBox="-120.96748 -90.49945 227.92627 196.50627" width="78.14615pt" height="67.37357pt">
   <rect x="-120.96748" y="-90.49945" width="227.92627" height="196.5062
   ```

6. **Export PNG — PARTIAL overall; export and preview checks PASS.** Activated the Export PNG button through page JavaScript. The footer preview updated to `alt="PNG export"`, `complete=true`, with natural dimensions **1303×1123**. The API returned a genuine `Uint8Array` (`instanceof Uint8Array === true`) with **117,358 bytes**, saved to [export.png](export.png). First eight bytes: `137 80 78 71 13 10 26 10`, the PNG signature. Footer remained `1 changes`. Screenshot: [08-export-png.png](08-export-png.png). Physical button clicking was not tested.

7. **Change callbacks — PARTIAL.** Registered the requested second listener with `let n=0; const off=window.editor.onChange(()=>n++);`. Immediately afterward, `n=0` and the footer was `1 changes`. Because mouse editing was blocked, performed a supplemental public-API reload of caffeine. After that load, `n=1` and the footer was `2 changes`, confirming both callbacks fired for a load. Screenshot: [09-change-callback.png](09-change-callback.png). The listener's `n` value is recorded in [api-results.jsonl](api-results.jsonl), not drawn onto the screenshot. Called `off()` afterward. The callback behavior after a real mouse edit or undo remains unverified.

   | Point | Footer counter | Interpretation |
   | --- | ---: | --- |
   | Initial editor / before step 2 | 0 | Baseline |
   | Blocked bond and ring steps | 0 | No input performed |
   | Blocked Ctrl/Cmd undo step | 0 | No input performed |
   | Before caffeine load | 0 | Baseline for load |
   | After caffeine load | 1 | Load callback fired |
   | After SVG and PNG exports | 1 | Exports did not change document counter |
   | Before supplemental callback load | 1 | Second listener `n=0` |
   | After supplemental callback load | 2 | Second listener `n=1` |
   | After resize and restore | 2 | No extra document change callback |

8. **Resize container smaller and restore — PASS for the permitted container-height alternative.** Set `document.getElementById('editor').style.height='300px'`, then restored it to `''`. Small: editor and canvas both 1400×300 CSS pixels, backing buffer 2800×600. Restored: both 1400×728.5 CSS pixels, backing buffer 2800×1457. DPR remained 2. The molecule automatically fitted the reduced drawing area, remained visible, and returned to its larger rendering. No blank canvas, stretching, or visible blur was observed. At 300 px height the tool/sidebar regions were clipped or scrollable; the large white area below the editor was outside the 300 px container, not an unrendered portion of its canvas. Screenshots: [10-resized-small.png](10-resized-small.png), [11-resized-large.png](11-resized-large.png). Browser width was not changed.

9. **Final console dump — PASS.** Saved readable output to [console.txt](console.txt) and raw CDP events to [console-events.jsonl](console-events.jsonl). Captured one page/network error: `Failed to load resource: the server responded with a status of 404 (Not Found)` for `http://localhost:63925/favicon.ico`. No application panic, runtime exception event, or warning was captured. Runtime and Log domains were enabled after initial page startup, requesting buffered messages; monitoring continued until isolated-instance shutdown.

   One diagnostic evaluation initially returned `SyntaxError: Unexpected identifier 'window'` because the harness had not enabled CDP's top-level-await `replMode`. This was a verifier expression error, not an app failure. Enabling `replMode` and repeating the serialization query succeeded. The error and corrected result are retained in `api-results.jsonl` and explained in `console.txt`.

**Cleanup — PASS.** Ran the exact profile-specific `pkill -f "user-data-dir=…/ev-wasm/chrome-profile"` requested by the user. A subsequent process check found no Chrome process using that profile; localhost port 9333 was closed. Evidence: [cleanup.json](cleanup.json). The user's other Chrome instance and the demo server were not targeted.

**Unverified and why:** Real mouse bond drawing, ring placement, canvas focus, Ctrl+Z, Cmd+Z, callbacks after mouse edits/undo, and physical clicks on header buttons remain unverified because the computer-use tool could not safely select the isolated Chrome process. Requested screenshots 02–04 document the blocked states. The exact time-to-first-render was not instrumented. WebGL fallback and browser-width resizing were not tested; the active WebGPU path and the explicitly permitted container-height resize were tested. No application failure was observed in the checks that could be completed, but this run is not a completed mouse/keyboard UI verification.
