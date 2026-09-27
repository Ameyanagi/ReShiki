# ReShiki 0.9.1

ReShiki 0.9.1 fixes dark bands accumulating over the drawing-style and canvas-theme dropdowns on Windows.

[Release downloads](https://github.com/Ameyanagi/ReShiki/releases/tag/v0.9.1) · [Previous release](changes-0.9.md)

## Windows dropdowns

Moving the pointer between menu items could repeatedly darken the first row and the area around a dropdown. The Windows software renderer painted shadows outside the region being redrawn. The earlier popup fix covered dialogs and palettes, but the newer dropdown styling did not use it.

Dropdowns now share the same Windows shadow handling as the other popups. Borders, rounded corners, item highlighting and light/dark colors remain available. macOS and Linux retain their existing shadows.

## Regression coverage

A Windows test renders the style and theme menus in both light and dark interface modes at 100%, 125% and 200% display scaling. It performs 16 partial redraws and requires every pixel outside the damaged region to remain unchanged. The existing popup regression test also remains in the Windows CI suite.

The fix does not alter drawing styles, canvas colors, chemistry, clipboard data or document formats. Downloads remain available for Windows and Linux on x64/ARM64 and for separate Intel and Apple Silicon Macs.

Thank you to all the [contributors](contributors.md) who have helped improve ReShiki.
