Completed the verification and saved the full [report.md](/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b-final/app/report.md), all 18 requested screenshots, supplementary captures, and 40 morph frames.

The tested feature checks passed:

- Rotation and roll preserved the **744 × 744 px** frame.
- Resizing produced **884 × 884 px**, preserved through subsequent rotation.
- The floating bar hid during drags and stayed inside the canvas when zoomed.
- Double-click reset, projection flattening, original-layout restoration, Undo, the context submenu, and Both mode behaved as expected.
- Morph captures show intermediate geometry over approximately **0.8 seconds**; capture timing cannot establish exactly 0.750 seconds.

The report documents the actual window size, zoom adjustments, exact drag coordinates, status text, and verification limits.

**Cleanup confirmed:** launched PID **29655** was terminated; installed PID **51217** remains running. No drawing was saved, and no repository files or system settings were changed.