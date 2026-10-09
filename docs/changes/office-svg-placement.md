# Export an SVG picture for Office

Choose **Figure → SVG · Office picture** when inserting an exported figure into Word or PowerPoint. The file has explicit physical dimensions and converts text to vector outlines before export, using the same placement treatment as the existing Windows clipboard picture. This avoids relying on Office's interpretation of SVG text baselines or the receiving computer's fonts.

Drawing objects remain vectors, but outlined text cannot be edited as text in Office or another illustration program. Embedded pictures keep their existing image representation. Keep the native ReShiki drawing for changes. The ordinary **SVG** choice remains available with editable text. Both file choices retain the canvas background and figure padding; the clipboard picture remains transparent.

The exporter resolves the scene through the existing SVG parser, then writes physical point dimensions with the matching CSS-pixel viewBox. It leaves ordinary SVG bytes unchanged. The Save dialog uses `.svg` for either choice; the internal `svg-office` choice is a rendering mode, not a filename extension.

The renderer regression compares ordinary and outlined artwork at three times the SVG resolution, with light and dark canvas backgrounds and with clipboard transparency. It checks physical dimensions, no remaining text objects, pixel agreement, and font-independent re-import. The focused export and figure-snapshot tests passed, as did locked workspace all-targets/all-features Clippy and checking. The final export-button label correction passed a live-widget accessibility assertion, formatting, focused production Clippy and a fresh production build. Actual Mac Word and PowerPoint insertion from the first preserved candidate retained complete O and SiMe₃ labels and correct bond endpoints after save/reopen, at the same intrinsic dimensions as the ordinary SVG control. The final label-only candidate still needs its native export comparison and menu/completion captures; Windows Office desktop acceptance is unverified.

For review, export **SVG** and **SVG · Office picture** from the same drawing at the same style and insert both in actual Word and PowerPoint without resizing. Preserve the two SVG files, the Office documents, exact native dimensions, and matched screenshots showing labels, bond endpoints, and placement after save/reopen. Compare each Office size with that SVG's own width and height, including figure padding.

Caption: **Export an SVG picture with outlined text and physical dimensions for placement in Office.**
