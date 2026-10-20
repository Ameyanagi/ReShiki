# EV378 origin/main real-app round-trip attempt

Recorded: 2026-10-20T09:43:53.537187+09:00

**Outcome: BLOCKED at ChemDraw, for both settings. No round-trip result was obtained.** The expected unfixed-build regression was neither confirmed nor disproved. The values below distinguish observed source settings from unavailable step 6 readback.

## Exact build and driven process

- Requested source binary: `/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/bin-main/reshiki`
- Driven ReShiki PID: **47639**.
- Computer use was bound by the exact temporary app path: `/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev378/final/before/ReShiki-EV378-Before.app`.
- Process executable: `/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev378/final/before/ReShiki-EV378-Before.app/Contents/MacOS/reshiki-ev378-before`.
- The executable was a hard link to the requested binary. Both had device **16777229**, inode **13386622**, and size **258574264 bytes**.
- SHA-256: `57e3b15c7d256c59c983c5cfe10b20041b6e5fd2524ecb5d89616882192dca5a`.
- `ps` and `lsof -a -p 47639 -d txt -Fn` confirmed that PID 47639 was running the wrapper executable. Full identity evidence is in [binary-identity.json](binary-identity.json).
- No other ReShiki instance was selected, closed, or driven.

## Observed values and evidence

| Case | Source Bond attachment, observed in step 3 | Source Hydrogen position | Step 6 Bond attachment readback | Step 6 Hydrogen position readback | Outcome |
| --- | --- | --- | --- | --- | --- |
| L | Flush left | Automatic | **Not obtained** | **Not obtained** | Blocked before ChemDraw paste |
| C | Centered | Automatic | **Not obtained** | **Not obtained** | Blocked before ChemDraw paste |

For each source case, a fresh drawing tab was used, `CC` was imported through Import, cleanup was previewed and applied, and the view showed **250%**. The left carbon was selected, Enter opened its label editor, `MeO` was typed, and Enter applied it. With that atom selected, Properties → Labels & chemistry → Atom labels & numbering was opened, and the requested Bond attachment option was chosen. The whole two-atom molecule was then selected using Cmd+A and copied using Cmd+C.

### Flush left (L)

- Step 3 screenshot: [L-1-reshiki-set.png](L-1-reshiki-set.png). It visibly shows **Bond attachment (text labels): Flush left** and **Hydrogen position: Automatic**.
- Step 4 capture: [L-2-reshiki-clipboard/types.json](L-2-reshiki-clipboard/types.json), created using the specified Swift helper immediately after Cmd+C.
- Required step 5 screenshot `L-3-chemdraw.png`: **not created**, because ChemDraw could not be bound.
- Required ChemDraw copy capture directory `L-4-chemdraw-clipboard`: **not created**; no ChemDraw copy occurred.
- Required step 6 screenshot `L-5-reshiki-after.png`: **not created**; no ChemDraw-return paste or readback occurred.

### Centered (C)

- Step 3 screenshot: [C-1-reshiki-set.png](C-1-reshiki-set.png). It visibly shows **Bond attachment (text labels): Centered** and **Hydrogen position: Automatic**.
- Step 4 capture: [C-2-reshiki-clipboard/types.json](C-2-reshiki-clipboard/types.json), created using the specified Swift helper immediately after Cmd+C.
- Required step 5 screenshot `C-3-chemdraw.png`: **not created**, because ChemDraw could not be bound.
- Required ChemDraw copy capture directory `C-4-chemdraw-clipboard`: **not created**; no ChemDraw copy occurred.
- Required step 6 screenshot `C-5-reshiki-after.png`: **not created**; no ChemDraw-return paste or readback occurred.

## Clipboard validation

Both source captures advertise `dev.reshiki.drawing`, three ChemDraw CDX flavors, PDF, PNG, and SVG. In each capture, the native drawing payload contains exactly two atoms and one plain single bond: the left atom (x = −21) has the text label `MeO`, and the right atom (x = 21) is carbon. The left atom has `hydrogen_position: "auto"` in both captures and `label_attachment: "left"` for L / `"center"` for C. The three CDX flavor files are 434 bytes each. These are source clipboard observations; they do not show ChemDraw acceptance or a successful round trip.

## Blocker and deviations

1. The pre-existing ChemDraw process was PID **67529**, executable `/Applications/ChemDraw.app/Contents/MacOS/ChemDraw`. Repeated computer-use calls to bind `/Applications/ChemDraw.app` failed with **`Computer Use server error -10005: timeoutReached`** before returning any accessible windows or a usable app binding. This happened before any Cmd+N or Cmd+V was sent to ChemDraw, including retries after source preparation.
2. Process observations showed ChemDraw using approximately **97–98% CPU** during the attempts (later approximately 81%). This supports a responsiveness concern but does not establish the cause of the binding timeouts.
3. A bundle-ID binding attempt also reported ambiguity between `/Applications/ChemDraw.app` and `/Volumes/ChemDraw 26.1.0/ChemDraw.app`. The mounted copy was not driven. Exact-path retries still timed out. A documented optional `cua.computer.launch_app` entry point was unavailable (`is not a function`); it performed no launch or document operation.
4. Because no usable ChemDraw binding was obtained, its Window menu could not be inspected for hidden dialogs. No Change Settings dialog was observed or answered. No test ChemDraw document was created, pasted into, saved, or closed. Its pre-existing documents were left untouched, and its existing process remained running.
5. The sequence stopped after L step 4. While the blocker persisted, C steps 1–4 were completed separately to preserve source evidence; neither requested full 1–7 flow completed. No direct ReShiki-to-ReShiki paste was substituted for the missing ChemDraw round trip.
6. ReShiki exposed several inspector controls only visually rather than in the accessibility tree, so their values were read from the captured screenshots. One initial cleanup toolbar action returned `elementHasNoFrame`; cleanup was subsequently opened and applied through the visible controls. The existing 250% zoom was retained. A pre-existing “Recovery draft” banner was visible and was neither restored nor dismissed.
7. The user was notified of the ChemDraw blocker and asked asynchronously about restoring responsiveness or authorizing an alternate UI mechanism. No response was available before this report; no alternate UI automation mechanism was used.

## Cleanup

Cmd+Q was sent only to the exact-path ReShiki binding for PID 47639. The process exited immediately; no save/discard prompt was presented. No document Save action was performed. A subsequent `ps` check confirmed PID 47639 no longer existed. The post-quit UI observation reported `procNotFound`, consistent with the exit.

The temporary wrapper `ReShiki-EV378-Before.app` was removed after confirming process exit. The original source binary remains in place. No repository files or system settings were edited. ChemDraw PID 67529 remained running. See [processes-before.txt](processes-before.txt), [processes-after.txt](processes-after.txt), and the cleanup fields in [binary-identity.json](binary-identity.json).
