**ReShiki 3D structure display — local macOS UI verification**

Date: 2026-10-20 (Asia/Tokyo). Branch: `feat/3d-models-1a`.

**Result: PASS for the requested display sequence and final single-step Undo.** Caffeine imports, Structure offers 2D / 3D / Both, 3D replaces the drawing with a shaded ball-and-stick model, Both presents the two representations side by side, 2D hides the model, and one Cmd+Z restores Both. The transient Generating state was too brief to capture, so its exact wording and the MMFF94s worker cannot be confirmed from the UI evidence.

Repository: `/Users/shiranui/dev/ReShiki/.claude/worktrees/wf_9a402735-96b-2`

Executable: `/Users/shiranui/dev/ReShiki/.claude/worktrees/wf_9a402735-96b-2/target/debug/reshiki`

All UI interactions used computer use: mouse clicks, keyboard input, accessibility observations, and native screenshots. No rebuild or repository edit was performed. The working tree was clean before and after verification. No drawing was saved.

**Launch adaptation and instance isolation**

The exact bare executable was first launched directly as PID 72808. The computer-use tool could not bind to its executable path. A fallback app-name lookup inadvertently opened the installed ReShiki as PID 73776; that instance lacked the feature. These early screenshots were preserved with the `wrong-instance-` prefix and excluded from feature verdicts. That extra instance was quit via Cmd+Q. The original unbundled test process was then stopped by its tracked PID.

To make the specified executable identifiable to computer use, an artifact-only `ReShiki Verification.app` wrapper was created with a unique bundle ID, `dev.reshiki.verification.ev3861a`. Its `Contents/MacOS/reshiki` is a symlink to the exact requested binary. That executable symlink was launched directly from a shell subprocess as PID 80959; no application source, binary contents, or system settings were changed. Filesystem `samefile` verification returned true. The wrapper remains in this artifact directory for auditability. All unprefixed screenshots below belong to this verified test instance.

Binary SHA-256: `66b2f9ed1eab2645d830d8a6fc8a32640c52644fc26180becb4c8e80d1c08f4f`.

**Requested steps and evidence**

Every screenshot includes the native window title and bottom status bar. A persistent `Recovery draft` notice with `Restore` and `Dismiss` buttons appears at the left of the status bar throughout; neither button was used. The status column below records the additional operation message. The remaining bar shows `JACS / ACS`, `Publication`, `View`, zoom controls, and `Fit`.

| Step / verdict | Action and observed versus expected | Screenshot | Operation status bar text | Window title / unsaved marker | Zoom |
|---|---|---|---|---|---|
| 1 — PASS, with launch adaptation above | Launched the supplied binary through the artifact wrapper. Empty molecular drawing window, Properties panel visible, no Structure section without a selected molecule. | [01-launched.png](01-launched.png) | No operation message; `Recovery draft` notice only. | `Untitled — ReShiki`; no unsaved dot. | 100% |
| 2 — PASS | Opened toolbar Import, entered exactly `CN1C=NC2=C1C(=O)N(C(=O)N2C)C`, and pressed Cmd+Enter. Caffeine appeared selected on the canvas; input was detected as SMILES. | [02-caffeine-inserted.png](02-caffeine-inserted.png) | `Inserted structure · Drag to position · Delete or Undo to remove` | `Untitled • — ReShiki`; unsaved dot present. | 238% |
| 3 — PASS | Opened Properties, clicked Select, and pressed Cmd+A. Inspector reported `14 atoms · 15 bonds`, molecular formula `C8H10N4O2`, and an expanded Structure section above Molecular properties. 2D was active; 3D and Both inactive. | [03-structure-section-2d.png](03-structure-section-2d.png) | `Inserted structure · Drag to position · Delete or Undo to remove` | `Untitled • — ReShiki`; unsaved dot present. | 238% |
| 4 immediate capture — PASS for completed 3D; Generating UNVERIFIED | Clicked 3D and took the first available screenshot immediately. Generation had already completed: 3D was active, a shaded ball-and-stick model replaced the 2D drawing, and no 2D N/O labels remained. This file is deliberately named immediate-3d, not generating. | [04a-immediate-3d.png](04a-immediate-3d.png) | `Shown as 3D model · Undo restores the 2D drawing` | `Untitled • — ReShiki`; unsaved dot present. | 238% |
| 4 fitted view — PASS | Clicked Fit to show the whole model and its rectangular frame. Model uses gray, blue, red, and white atom spheres with shaded bonds; 3D remains active, 2D drawing hidden. No Generating message remained. | [04-3d.png](04-3d.png) | `Shown as 3D model · Undo restores the 2D drawing` | `Untitled • — ReShiki`; unsaved dot present. | 158% |
| 4 close-up — PASS | Clicked the zoom-plus control twice, changing only the viewport. All molecular geometry remains visible at the larger scale, allowing inspection of sphere shading, bond connections, and colors. | [04b-3d-closeup.png](04b-3d-closeup.png) | `Shown as 3D model · Undo restores the 2D drawing` | `Untitled • — ReShiki`; unsaved dot present. | 247% |
| 5 — PASS | Clicked Both and then Fit. Plain 2D caffeine appears on the left, with the 3D model on the right, inside a full shared selection frame. Both is active. The two representations do not overlap. | [05-both.png](05-both.png) | `Shown as 2D and 3D · Undo restores the previous view` | `Untitled • — ReShiki`; unsaved dot present. | 124% |
| 6 — PASS | Clicked 2D. Only the original 2D drawing remains, with a smaller selection box. The 3D model is hidden and 2D is active. | [06-2d.png](06-2d.png) | `Shown as 2D · Undo restores the 3D model` | `Untitled • — ReShiki`; unsaved dot present. | 124% |
| 7 — PASS | Pressed Cmd+Z exactly once after step 6. Both representations returned in their previous positions, and Both became active. Redo became available. | [07-undo.png](07-undo.png) | `History restored` | `Untitled • — ReShiki`; unsaved dot present. | 124% |

The final accessibility snapshot, [07-undo-accessibility.txt](07-undo-accessibility.txt), independently records 2D off, 3D off, and Both on.

**Cleanup — PASS**

After the final screenshot, sent Cmd+Q to the uniquely bound verification instance. It exited; no save prompt appeared. The three tracked processes launched during this task (72808, 73776, and 80959) were all absent afterward. Other applications were not quit or closed. See [cleanup-verification.json](cleanup-verification.json), [binary-verification.json](binary-verification.json), and [launched-process.json](launched-process.json). `app-launch.log` remained empty.

**Excluded diagnostic screenshots**

These screenshots are from the inadvertently opened installed application, not the requested build. They are retained to preserve every screenshot and are not used to pass or fail the new feature. All show `Recovery draft / Restore / Dismiss`; the additional status and title are recorded individually below.

| Screenshot | What was observed | Operation status bar text | Window title / unsaved marker | Zoom |
|---|---|---|---|---|
| [wrong-instance-01-launched.png](wrong-instance-01-launched.png) | Empty installed-app canvas. | No operation message. | `Untitled — ReShiki`; no unsaved dot. | 100% |
| [wrong-instance-02-caffeine-inserted.png](wrong-instance-02-caffeine-inserted.png) | Caffeine imported and selected. | `Inserted structure · Drag to position · Delete or Undo to remove` | `Untitled • — ReShiki`; unsaved dot present. | 238% |
| [wrong-instance-03a-properties-selected.png](wrong-instance-03a-properties-selected.png) | Properties top with 14 atoms / 15 bonds selected, no Structure section. | `Inserted structure · Drag to position · Delete or Undo to remove` | `Untitled • — ReShiki`; unsaved dot present. | 238% |
| [wrong-instance-03b-properties-scrolled.png](wrong-instance-03b-properties-scrolled.png) | Inspector scrolled to its bottom, no Structure section. | `Inserted structure · Drag to position · Delete or Undo to remove` | `Untitled • — ReShiki`; unsaved dot present. | 238% |
| [wrong-instance-03c-selection-recheck.png](wrong-instance-03c-selection-recheck.png) | Keyboard drawing disabled; molecule still selected. | `Keyboard drawing off` | `Untitled • — ReShiki`; unsaved dot present. | 238% |
| [wrong-instance-03-structure-section-2d.png](wrong-instance-03-structure-section-2d.png) | Full compact inspector after Select and Cmd+A; no Structure section. | `Keyboard drawing off` | `Untitled • — ReShiki`; unsaved dot present. | 238% |
| [wrong-instance-03d-molecule-double-click.png](wrong-instance-03d-molecule-double-click.png) | Whole-molecule double-click selection produced the same inspector. | `Keyboard drawing off` | `Untitled • — ReShiki`; unsaved dot present. | 238% |

**Not verified and visual observations**

- The optional `Generating 3D model · MMFF94s` state was not captured: the model was already rendered in the immediate post-click screenshot. No `04a-generating.png` is supplied. The specific force field and background-worker execution were not verified by the UI test; successful rendering alone does not establish those implementation details.
- The bare executable could not be controlled directly through the computer-use tool. The feature was verified with the same executable reached through an artifact-only app-bundle symlink. The root cause of bare-app visibility/binding was not established.
- The required final single Undo was verified. Undo boundaries for the earlier 2D→3D and 3D→Both switches were not separately exercised. Multiple-molecule selection behavior, rotation, export, and saving were outside this flow.
- At 247% zoom, several bond shafts and their outlines visibly extend into the front faces of atom spheres toward their centers, especially at the central fused-ring carbon and methyl carbons. This produces visible seams on the shaded spheres. Bonds remain connected; this is a rendering observation rather than a confirmed geometry error.
- The model frame extends beyond the canvas vertically in the immediate 238% view and deliberate 247% close-up. Fit at 158% shows the entire frame and model without clipping. Both at 124% also fits fully, with no representation overlap or inspector obstruction.
- The orange keyboard-drawing crosshair remains at an empty canvas position. In the fitted 3D screenshot a small part is clipped at the top canvas boundary. It is distinct from chemistry labels. No leftover 2D atom labels or 2D bonds were visible over the 3D-only model.
- After Undo, 2D retains a teal hover/focus outline beneath the pointer while Both has the active fill. Accessibility confirms only Both is active; the canvas agrees.
- Step 6 status says `Undo restores the 3D model`, even though the prior state is Both. Actual Undo correctly restores Both; the wording is less specific than the restored state.
- The Structure card provides the three requested controls, but it does not visibly show a literal `Show as` label. Its helper text reads `Ball & stick · Chemistry and atom IDs stay the same`.
- The collapsed Molecular properties card shows `Selection · C8H10N4O2` before generating 3D, then only `Selection` in the later screenshots. The card was not expanded to investigate this summary change.
- Atom colors and shading are consistent: carbon gray, nitrogen blue, oxygen red, hydrogen white. No unexpected colors, disconnected visible bonds, 2D/3D overlap, or persistent model remnant after switching to 2D was observed.
