# Prototype reference screenshot report

Completed 20 requested window-only PNG captures: 16 individual model views and 4 “The four styles, same view” strips. All final screenshots were visually reviewed against their filenames.

## Browser and capture setup

- Prototype: `prototype-artifact-89rRsm9QDX5gwanE7kokSm.html` in the parent directory. The HTML was not edited.
- Launched a separate Google Chrome instance using the exact command and throwaway profile provided in the request.
- Main process PID: **70420**. Window ID: **7128**, obtained with the provided `ev386-1b-wid` helper.
- Process identity, start time, and arguments are saved in [launched-process.txt](launched-process.txt).
- Requested launch size was 1500 × 1400 points. macOS constrained the actual window to **1500 × 1020 points**, at screen origin **(22, 30)**. Browser zoom was set to **75% before the final capture sequence**, allowing the complete paper canvas and inspector to fit.
- All final images are **3000 × 2040 pixels**. Window dimensions and browser zoom stayed fixed throughout the final sequence.
- Every final PNG was created with `screencapture -x -o -l 7128 <output-path>`. The mouse was parked in the browser toolbar, outside the model canvas, and a 0.6-second settling delay preceded each capture.

## Procedure and verification

Activated 3D-only display and waited at least 2 seconds. Selected each molecule with its title-bar chip and waited 1.1 seconds. For each molecule and look, selected Ball & stick, Stick, Wireframe, and Space-filling in order. The 16 main shots all use the top scroll position and show the full paper canvas, full molecule, and complete inspector.

After each four-style group, scrolled to the bottom and captured the complete four-tile strip, then returned to the top. Space-filling remained selected for each strip capture. All four strips share the same bottom scroll framing.

No model-canvas drag was issued. Reset view was activated during setup, before the final sequence. The final views use the opening orientation established by the molecule chip, consistently across styles and looks.

All 20 images visibly retain Hydrogens **All**, Atom labels **None**, Depth fade **35%**, Perspective **20%**, and Background **None**. MMFF94s also remains selected. Independent visual review confirmed the pressed controls, consistent framing, complete models and strip tiles, and absence of obstructing transient overlays. File verification confirmed 20 distinct PNG hashes and identical dimensions; see [capture-manifest.json](capture-manifest.json) and [capture-log.jsonl](capture-log.jsonl).

## Issues and recovery

- The native computer-use tool could not bind an exact macOS window ID. Shell-based native mouse/keyboard and accessibility helpers were therefore scoped to the recorded process/window. The input helper checked window ownership and refused global input unless PID 70420 was frontmost.
- During the initial trial batch, some style clicks did not take effect and a foreground change caused the input guard to stop. Mouse-event settings were corrected, only the target Chrome PID was explicitly reactivated, and the trial captures were replaced by a complete verified batch. No unverified trial image is included among the 20 final captures.
- The display height limitation was handled with browser zoom before evidence capture. No window resizing or zoom changes occurred between final screenshots.
- Setup images and helper files are retained under `diagnostics/`. Only files within this output directory were explicitly created, edited, or moved; Chrome used the separately authorized throwaway profile.

## Pressed controls for each capture

For main images, both the floating **3D** button and inspector **3D model** button appear selected. In strip images, the molecule chip and Show-as controls are above the visible viewport; their active states are confirmed in the corresponding preceding main capture. The table distinguishes these offscreen controls from buttons directly visible in the strip screenshot.

| Capture | Molecule | Look | Style | Show as | Scroll |
|---|---|---|---|---|---|
| [caffeine-ballstick-flat.png](caffeine-ballstick-flat.png) | Caffeine | Flat · journal | Ball & stick | 3D / 3D model | Top |
| [caffeine-stick-flat.png](caffeine-stick-flat.png) | Caffeine | Flat · journal | Stick | 3D / 3D model | Top |
| [caffeine-wire-flat.png](caffeine-wire-flat.png) | Caffeine | Flat · journal | Wireframe | 3D / 3D model | Top |
| [caffeine-space-flat.png](caffeine-space-flat.png) | Caffeine | Flat · journal | Space-filling | 3D / 3D model | Top |
| [caffeine-strip-flat.png](caffeine-strip-flat.png) | Caffeine (offscreen; confirmed before scroll) | Flat · journal | Space-filling (inspector and tile) | 3D (offscreen; confirmed before scroll) | Bottom |
| [caffeine-ballstick-glossy.png](caffeine-ballstick-glossy.png) | Caffeine | Glossy · slides | Ball & stick | 3D / 3D model | Top |
| [caffeine-stick-glossy.png](caffeine-stick-glossy.png) | Caffeine | Glossy · slides | Stick | 3D / 3D model | Top |
| [caffeine-wire-glossy.png](caffeine-wire-glossy.png) | Caffeine | Glossy · slides | Wireframe | 3D / 3D model | Top |
| [caffeine-space-glossy.png](caffeine-space-glossy.png) | Caffeine | Glossy · slides | Space-filling | 3D / 3D model | Top |
| [caffeine-strip-glossy.png](caffeine-strip-glossy.png) | Caffeine (offscreen; confirmed before scroll) | Glossy · slides | Space-filling (inspector and tile) | 3D (offscreen; confirmed before scroll) | Bottom |
| [ibuprofen-ballstick-flat.png](ibuprofen-ballstick-flat.png) | Ibuprofen | Flat · journal | Ball & stick | 3D / 3D model | Top |
| [ibuprofen-stick-flat.png](ibuprofen-stick-flat.png) | Ibuprofen | Flat · journal | Stick | 3D / 3D model | Top |
| [ibuprofen-wire-flat.png](ibuprofen-wire-flat.png) | Ibuprofen | Flat · journal | Wireframe | 3D / 3D model | Top |
| [ibuprofen-space-flat.png](ibuprofen-space-flat.png) | Ibuprofen | Flat · journal | Space-filling | 3D / 3D model | Top |
| [ibuprofen-strip-flat.png](ibuprofen-strip-flat.png) | Ibuprofen (offscreen; confirmed before scroll) | Flat · journal | Space-filling (inspector and tile) | 3D (offscreen; confirmed before scroll) | Bottom |
| [ibuprofen-ballstick-glossy.png](ibuprofen-ballstick-glossy.png) | Ibuprofen | Glossy · slides | Ball & stick | 3D / 3D model | Top |
| [ibuprofen-stick-glossy.png](ibuprofen-stick-glossy.png) | Ibuprofen | Glossy · slides | Stick | 3D / 3D model | Top |
| [ibuprofen-wire-glossy.png](ibuprofen-wire-glossy.png) | Ibuprofen | Glossy · slides | Wireframe | 3D / 3D model | Top |
| [ibuprofen-space-glossy.png](ibuprofen-space-glossy.png) | Ibuprofen | Glossy · slides | Space-filling | 3D / 3D model | Top |
| [ibuprofen-strip-glossy.png](ibuprofen-strip-glossy.png) | Ibuprofen (offscreen; confirmed before scroll) | Glossy · slides | Space-filling (inspector and tile) | 3D (offscreen; confirmed before scroll) | Bottom |

## Shutdown

Returned the page to the top after the last strip. Validated the main process arguments again and terminated only the prototype Chrome instance with **`kill 70420`**. A profile-specific `pgrep -fl` returned exit code **1** with no matches, and `pgrep -x "Google Chrome"` confirmed PID 70420 was absent. See [termination.txt](termination.txt). No other application or Chrome window was selected for automation or closed.
