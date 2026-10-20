Verification completed. The [report](</private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b/app/report.md>) records every step, exact drag coordinates, status text, window title, and verdict. All 13 evidence screenshots and 40 morph frames are saved in the artifact directory.

- **Core flow passed:** morph, rotation, Shift-roll, reset, resize, flattening, and single-Undo restoration.
- **One visual defect:** the floating bar’s lower border is clipped by the footer after the first rotation, visible in [05-rotate-released.png](</private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b/app/05-rotate-released.png>).
- **Cleanup confirmed:** test PID **46018** was terminated with `kill`; installed-app PID **51217** remains running.

The report notes limits on exact animation timing, individual rotation Undo boundaries, and hover-only submenu behavior.