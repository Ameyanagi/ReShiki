# ReShiki #386 phase 2 — local macOS UI verification

Date: 2026-10-20 (Asia/Tokyo). Result: **PASS for all requested feature checks**, with the scope and navigation limitations stated at the end. All 24 requested evidence screenshots were captured, plus a Perspective Undo screenshot and an analysis crop.

## Isolation and evidence method

Only `dev.reshiki.verification.ev386final2` / `ReShiki386F2Evidence.app` was attached through computer use. The bundle was launched exactly three times, sequentially, with `open -n` and the requested `RESHIKI_DATA_DIR` and `--open` input. Every run had exactly one process matching `apps386-final-2/ReShiki386F2Evidence.app/Contents/MacOS/reshiki`. The supplied PID-to-window helper identified the corresponding window; capture and drag wrappers rechecked this PID match before use.

| Run | Input | Data directory suffix | PID | Window ID | Result |
|---|---|---|---:|---:|---|
| A | `caffeine-ballstick-flat.rsk` | `ev386-2-final-data` | 38356 | 7402 | PASS; terminated only this PID |
| B | `ibuprofen-ballstick-flat.rsk` | `ev386-2-final-data` | 40976 | 7540 | PASS; terminated only this PID |
| C | `caffeine-stick-flat.rsk` | `ev386-2-final-data-dark` | 41441 | 7579 | PASS; terminated only this PID |

The installed app, PID **51217**, appears in the before-process list and in every after-cleanup process list. It was never attached, focused, clicked, opened, captured, or terminated. No drawings were saved, no repository files were edited, and no system settings were changed. Changes made to the test drawings were transient UI edits and were discarded by terminating only the launched PID.

Logs: [installed-process-before.txt](installed-process-before.txt), [launched-process.txt](launched-process.txt), [process-cleanup.txt](process-cleanup.txt), and [drag-commands.txt](drag-commands.txt). `process-cleanup.txt` records `pgrep` exit code 1 and empty output for the verification executable after each termination, followed by `pgrep -lf reshiki`.

Screenshots use `screencapture -x -o -l <WID> <file>`; the held-drag image uses the supplied helper's capture option. All requested PNGs are **2560 × 1704 pixels**, corresponding to the unchanged **1280 × 852 point** window. Coordinates in this report are **window points from the top-left, including the title bar**. Computer-use coordinate clicks were supplied at twice these values because its screenshot coordinate system uses image pixels. The drag helper received the point coordinates directly. Before each ordinary evidence shot, the pointer was moved clear of model and controls by a single click on blank title-bar space at **(750, 16) pt**. A13 intentionally captures while the mouse button is held. Orange crosshairs visible in some shots are the application's keyboard-drawing marker, not the mouse pointer.

## Run A — caffeine, light UI

### A01–A05: styles and both subtitles

Clicked Select / move (arrow), then the model once at **(520, 480) pt**. An earlier coordinate-mapping probe at **(260, 240) pt** hit empty canvas and did not select or edit the model. The successful click showed the selected model frame and the expanded Structure and 3D model sections. **No inspector scrolling or collapsing was needed for A01–A08:** the Structure note and the 3D model heading/subtitle, Style, Look, Hydrogens, and sliders were visible together.

The expected outcome for each row was the specified style highlighted, the corresponding model representation, and both subtitles naming the current style. Line wraps are flattened below; wording, capitalization, punctuation, and the middle dot are preserved verbatim.

| Step / file | Action and observed representation | Structure note, verbatim | 3D model subtitle, verbatim | Status bar / zoom | Result |
|---|---|---|---|---|---|
| [A01-section-ballstick.png](A01-section-ballstick.png) | Selected initial model. **Ball & stick** active; atom spheres joined by thin bonds. | `Ball & stick · Chemistry and atom IDs stay the same` | `Ball & stick` | `Document opened` / `158%` | PASS |
| [A02-style-stick.png](A02-style-stick.png) | Clicked **Stick**. Thick cylinders and small rounded joints; Ball & stick inactive. | `Stick · Chemistry and atom IDs stay the same` | `Stick` | `Drawing updated` / `158%` | PASS |
| [A03-style-wireframe.png](A03-style-wireframe.png) | Clicked **Wireframe**. Thin colored line segments; Stick inactive. | `Wireframe · Chemistry and atom IDs stay the same` | `Wireframe` | `Drawing updated` / `158%` | PASS |
| [A04-style-spacefilling.png](A04-style-spacefilling.png) | Clicked **Space-filling**. Large overlapping atom spheres; Wireframe inactive. | `Space-filling · Chemistry and atom IDs stay the same` | `Space-filling` | `Drawing updated` / `158%` | PASS |
| [A05-style-ballstick.png](A05-style-ballstick.png) | Clicked **Ball & stick**. Original opening representation/orientation restored. | `Ball & stick · Chemistry and atom IDs stay the same` | `Ball & stick` | `Drawing updated` / `158%` | PASS |

All five show **Flat · journal**, **All** hydrogens, Depth fade **35%**, Perspective **20%**. The selection summary is `14 atoms · 15 bonds`; Structure is **3D** and the 2D layout control remains **Original drawing**. No unexpected atom/bond-count change was observed when changing representation.

### A06–A10: look, hydrogens, perspective, background

| Step / file | Action | Observed versus expected | Active controls / status | Result |
|---|---|---|---|---|
| [A06-look-glossy.png](A06-look-glossy.png) | Clicked **Glossy · slides**. | More concentrated bright highlights on atom spheres and softer-looking outlines than Flat; expected visual look change present. Then clicked **Flat · journal** again before A07. | Ball & stick; Glossy; All. `Drawing updated`, `158%`. | PASS |
| [A07-hydrogens-polar.png](A07-hydrogens-polar.png) | With Flat restored, clicked **Polar**. | All white hydrogen atoms and their bonds disappeared. No polar H remained, as expected for this caffeine input. Heavy atoms remained; the visible model refitted/enlarged within its frame. | Ball & stick; Flat; **Polar** on, All/None off. `Drawing updated`, `158%`. | PASS |
| [A08-hydrogens-none.png](A08-hydrogens-none.png) | Clicked **None**. | Same hydrogen-free model as Polar, with None now active. Then restored **All**; white H atoms reappeared. | Ball & stick; Flat; **None** on, All/Polar off. `Drawing updated`, `158%`. | PASS |
| [A09-perspective-100.png](A09-perspective-100.png) | Drag helper: **(1060, 784) → (1265, 784) pt**, no Shift. Endpoint overshoots the slider's right end (~1252 pt). | Perspective visibly reads **100%** and its knob is at the far right. Projection/depth scaling visibly changed. | Ball & stick; Flat; All; Depth fade **35%**; Perspective **100%**. `Drawing updated`, `158%`. | PASS |
| [A09b-perspective-undo-20.png](A09b-perspective-undo-20.png) | Pressed **Cmd+Z exactly once** after the drag. | Perspective returned to **20%** and the opening projection returned. | Depth fade **35%**, Perspective **20%**; other settings retained. `History restored`, `158%`. | PASS |
| [A10-soft-card.png](A10-soft-card.png) | Scrolled Properties to the lower 3D settings, then clicked **Soft card**. | A pale rounded card appeared behind the model, within its frame. Then clicked **None**, which removed the card before rotation. | Ball & stick; Flat; All; **Soft card** on, None off in A10; Depth fade **35%**, Perspective **20%**. `Drawing updated`, `158%`. | PASS |

Both subtitles remained `Ball & stick · Chemistry and atom IDs stay the same` / `Ball & stick` throughout A06–A09b. The Structure note was scrolled offscreen in A10; the visible 3D model subtitle was `Ball & stick`.

Inspector navigation before A10: computer-use scroll **down 0.55 pages** at **(1210, 725) pt** overshot into Bond appearance; **up 0.25 pages**, then **up 0.10 pages**, at **(1200, 450) pt** brought the complete 3D model section, including Background and Reset view, into view. No section was collapsed. An attempted pixel-distance scroll was rejected by the computer-use tool because macOS accepts pages; it produced no UI change. From A10 through A14 the complete 3D model section was visible, while the Structure style note was above the viewport.

### A11–A14: rotate, reset, held drag

| Step / file | Action | Observed versus expected | Settings and status | Result |
|---|---|---|---|---|
| [A11-rotated.png](A11-rotated.png) | After Background None, drag helper **(500, 480) → (620, 530) pt**, no Shift: +120 x / +50 y. | Model rotated to a nearly edge-on view inside the frame. Expected 3D rotation occurred without moving the frame. | Ball & stick; Flat; All; None background; Depth fade **35%**, Perspective **20%**. `Drawing updated`, `158%`. | PASS |
| [A12-reset-view.png](A12-reset-view.png) | Clicked **Reset view**. | Opening orientation matches A05 visually. No additional visible model setting changed. The unsaved marker cleared because the displayed model settings/orientation again matched the input. The keyboard-position marker remained at its latest location; it was not restored to its earlier A05 location. | Ball & stick; Flat; All; None background; Depth fade **35%**, Perspective **20%**; same frame and `158%` zoom. `Drawing updated`. | PASS |
| [A13-stick-rotate-mid-drag.png](A13-stick-rotate-mid-drag.png) | Clicked **Stick**. Drag helper **(550, 450) → (450, 510) pt**, no Shift: −100 x / +60 y, with `capture=.../A13-stick-rotate-mid-drag.png`. | Supplied helper captured the held-button state before release. Stick rendering remains intact and rotated; no pale C–C midpoint seam is visible at this normal zoom. The canvas hint reads `Drag to rotate · Shift-drag to roll · Double-click to reset`. | **Stick**, Flat, All, None background, **35%** / **20%**. Subtitle `Stick`. Status `Drawing updated`, `158%`. | PASS |
| [A14-stick-rotate-released.png](A14-stick-rotate-released.png) | Let the helper release, then moved pointer to blank title bar. | Rotated Stick appearance persists after release, with no observed geometry jump or new pale seam. Held-drag hint is gone. Clicked **Reset view** afterward to restore the opening orientation while keeping Stick active. | Stick, Flat, All, None background, **35%** / **20%**. Subtitle `Stick`. `Drawing updated`, `158%`. | PASS |

Exact helper invocations are preserved in [drag-commands.txt](drag-commands.txt). No Shift modifier was used in any helper invocation.

### A15–A16: 400% seam inspection

For A15, after Reset view, clicked empty canvas at **(900, 300) pt** to deselect. A single click, and later a double-click, on the status-bar percentage did not open an entry field. Five status-bar plus clicks at **(1212, 831) pt** increased zoom multiplicatively (observed checkpoints **158% → 198% → 387% → 484%**), so those controls did not directly produce 400%.

Used the application's Commands palette for an exact value: **Shift+;** to type `:`, typed **`zoom 400`**, confirmed the selected result **`Zoom to 400%`**, then pressed **Enter**. Status bar visibly read **400%**. A read-only source lookup by an independent worker located this existing UI command; the zoom itself was applied and verified through keyboard/UI, not through file modification.

Panned on the canvas using scroll at **(850, 300) pt**: **down 0.03 pages**, then **up 0.05 pages**, then **left 0.02 pages**. Vertical scrolling positioned the target bond in the middle region of the canvas; the left-scroll call produced no visible horizontal change. Moved pointer to **(750, 16) pt**, explicitly waited **1 second**, then captured A15.

| File | Observation | Status / selection | Result |
|---|---|---|---|
| [A15-zoom-400-stick.png](A15-zoom-400-stick.png) | Examined the grey C–C bond shared by the six-membered ring and the adjacent five-membered ring. Atom-center endpoints are approximately **(485, 428.5)** and **(532.5, 569) pt**. The inspected two-half midpoint is approximately **(509, 499) pt**, i.e. **(1018, 998) image pixels**. **No pale transverse line, gap, or abrupt brightness step is visible there.** The pale band runs along the cylinder and is ordinary longitudinal shading. | **400%**, model deselected, `Select an object to edit its properties.` in Properties. Status `Drawing updated`. | PASS |
| [A16-zoom-400-ballstick.png](A16-zoom-400-ballstick.png) | Selected model at **(509, 499) pt**, clicked **Ball & stick**, then deselected as described below and returned to **400%**. Spheres and outlined narrow bonds appear as expected. The corresponding grey C–C midpoint is visually continuous without a pale transverse seam. | **400%**, model deselected, Properties shows `Select an object to edit its properties.`; status `Drawing updated`. | PASS |

An independent worker reviewed A15 at original resolution and agreed that the midpoint is continuous. Its unmodified direct analysis crop is [A15-cc-midpoint-analysis-crop.png](A15-cc-midpoint-analysis-crop.png); the primary screenshot remains unchanged.

A16 deselection workaround: at 400% the model's selection frame extended beyond the visible canvas. Clicking visually empty canvas at **(925, 200) pt** and pressing Escape left the selection active. Therefore, used Commands → **`zoom 100`**, confirmed **100%**, clicked empty canvas at **(900, 300) pt** outside the smaller frame, and then used Commands → **`zoom 400`** again. Confirmed the selection controls were gone, the model remained Ball & stick, and zoom was **400%** before capture. The same model orientation and central bond position were retained. Waited another **1 second** before A16. No geometry was edited by this navigation.

Terminated **PID 38356 only** with `kill 38356`. The verification executable no longer matched `pgrep`; PID 51217 still appeared. **PASS.**

## Run B — ibuprofen, light UI

Launched only after A's cleanup confirmation. Clicked Select / move, then model once at **(510, 490) pt**. Both section subtitles were visible together without scrolling or collapsing. The selection summary stayed `15 atoms · 15 bonds`; initial view zoom was **121%**.

Expected: each style's representation and both current-style subtitles; Glossy highlights; Polar leaving only the O–H hydrogen.

| Step / file | Action and observed result | Structure note, verbatim | 3D model subtitle, verbatim | Active Look / Hydrogens; status | Result |
|---|---|---|---|---|---|
| [B01-ballstick.png](B01-ballstick.png) | Selected initial **Ball & stick** model. Atom spheres/thin bonds visible. | `Ball & stick · Chemistry and atom IDs stay the same` | `Ball & stick` | **Flat · journal / All**; `Document opened`; `121%` | PASS |
| [B02-stick.png](B02-stick.png) | Clicked **Stick**. Thick cylinders with small rounded joints. | `Stick · Chemistry and atom IDs stay the same` | `Stick` | **Flat · journal / All**; `Drawing updated`; `121%` | PASS |
| [B03-wireframe.png](B03-wireframe.png) | Clicked **Wireframe**. Thin colored segments. | `Wireframe · Chemistry and atom IDs stay the same` | `Wireframe` | **Flat · journal / All**; `Drawing updated`; `121%` | PASS |
| [B04-spacefilling.png](B04-spacefilling.png) | Clicked **Space-filling**. Overlapping large atom spheres. | `Space-filling · Chemistry and atom IDs stay the same` | `Space-filling` | **Flat · journal / All**; `Drawing updated`; `121%` | PASS |
| [B05-glossy.png](B05-glossy.png) | Clicked **Ball & stick**, then **Glossy · slides**. Concentrated bright sphere highlights appear. | `Ball & stick · Chemistry and atom IDs stay the same` | `Ball & stick` | **Glossy · slides / All**; `Drawing updated`; `121%` | PASS |
| [B06-hydrogens-polar.png](B06-hydrogens-polar.png) | Clicked **Polar**. Exactly one white hydrogen is visible, attached to the lower red oxygen in the carboxyl group; carbon-bound H atoms disappear. Remaining O–H hydrogen is around **(635, 606) pt**. Heavy atoms remain and model refits slightly. | `Ball & stick · Chemistry and atom IDs stay the same` | `Ball & stick` | **Glossy · slides / Polar**; All and None inactive; `Drawing updated`; `121%` | PASS |

Depth fade remained **35%**, Perspective **20%**, and Structure **3D** / **Original drawing** throughout. No drag-helper action was needed for Run B.

Terminated **PID 40976 only** with `kill 40976`. The verification executable no longer matched `pgrep`; PID 51217 still appeared. **PASS.**

## Run C — caffeine Stick, dark UI

Launched with the supplied dark data directory, without changing appearance or system settings. The title bar, toolbar, sidebars, status bar, and Properties panels are visibly **dark**. The drawing canvas and its floating Show as control remain white/light; the requested dark application UI is nevertheless active.

| Step / file | Action | Observed versus expected | Text / active controls / status | Result |
|---|---|---|---|---|
| [C01-dark-stick-selected.png](C01-dark-stick-selected.png) | Clicked Select / move, then model once at **(520, 480) pt**. | Selected frame is visible; 3D model section is visible in dark Properties. Stick model renders normally on the white drawing canvas. | Structure note: `Stick · Chemistry and atom IDs stay the same`. 3D model subtitle: `Stick`. **Stick**, **Flat · journal**, **All** active; Depth fade **35%**, Perspective **20%**. `Document opened`, `158%`. | PASS |
| [C02-dark-stick-deselected.png](C02-dark-stick-deselected.png) | Clicked empty canvas at **(900, 300) pt**, then blank title bar. | Selection frame/floating Show as control disappeared. Stick model remains visible and unchanged; dark application panels remain dark. | Properties: `Select an object to edit its properties.`; no selected-model subtitle shown. `Document opened`, `158%`. | PASS |

No scroll, collapse, zoom change, or drag-helper action was needed in Run C.

Terminated **PID 41441 only** with `kill 41441`. The verification executable no longer matched `pgrep`; PID 51217 still appeared. **PASS.**

## Anything not verified / limits

- **No requested feature check remained blocked or unverified.** All 24 named evidence images exist, and the supplied sequence A → B → C and per-run cleanup were completed.
- The seam finding is a visual result for the specified 400% opening view and the additional normal-zoom rotation captures. It is not a proof for every bond, camera angle, display scale, or GPU. No pale midpoint line was observed in the required C–C target.
- Reset view was exercised through the **Reset view button**; the optional double-click alternative was not separately tested. All settings already restored to their requested values remained visually unchanged during Reset. Reset behavior with deliberately nondefault Look/Hydrogens/Background/slider values was outside this sequence.
- The Depth fade control was observed at **35%** but was not dragged, because the requested steps did not ask for a Depth fade change.
- The Structure note was above the scroll viewport during A10–A14; all style-transition subtitle checks A01–A05 and B01–B06 were performed with both subtitles visible. The full 3D model section was visible during A10–A14.
- The exact 400% requirement was met through the Commands palette. The status-bar percentage did not offer direct editing in the attempted clicks, and plus-button steps alone did not land on 400%. The A16 deselection workaround is documented above rather than treated as an unobserved successful click.
- Verification was limited to the UI and captured evidence. Save/reopen persistence was intentionally not tested because drawings were not to be saved.
