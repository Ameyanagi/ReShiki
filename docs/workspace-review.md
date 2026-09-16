# Workspace review and redesign

Compared desktop screenshots and native menu controls on 2026-09-16. A controlled aspirin drawing was opened in the installed version 26 reference editor and used for comparison with Moruno's startup sample. No existing research document was edited.

## Observations

The reference document screenshot shows a white page on a neutral surround with little document-window chrome. Its View menu exposes separate drawing, general, style and object toolbars, independent properties windows, and organized template collections. The computer-use capture isolates the active document window; floating palettes were not included in that capture. Their menu entries establish availability, not a complete visual or behavioral audit.

Moruno's previous layout used a large brand header, four rows above the canvas, a wide text-only tool list, and one long inspector combining properties, exports and templates. Some frequently used controls required scrolling. Ring and arrow options remained visible even while using unrelated tools.

## Implemented changes

| Area | Updated behavior |
| --- | --- |
| Drawing tools | Original monochrome vector icons in a two-column palette; hover names and shortcuts; persistent active-tool indication |
| Commands | Compact file, undo/redo, import, check, cleanup, export and inspector controls |
| Tool options | Context bar shows ring size/aromaticity, arrow style, atom symbol or annotation input when relevant |
| Inspector | Separate Properties, Templates and Export tabs; can be hidden |
| Selection | Transform, charge, isotope and label editing appear for the relevant selected objects |
| Import | Drawer opens on demand; Enter inserts, while replacement is an explicit separate action |
| Canvas | White surface, restrained neutral surround, grid off by default |
| Camera | Fit uses measured canvas dimensions; manual pan/zoom survives later viewport changes |
| Keyboard | Single-key tool/element selection, import/export shortcuts and a visible shortcut reference |

The same 1154 × 768 screenshot framing showed an approximately 805 × 579 pixel canvas with the inspector open, versus roughly 741 × 411 before the redesign: about 50% more drawing area. Hiding the inspector expanded it to roughly 1034 × 579, almost twice the earlier area. These measurements are screenshot estimates, not universal window-size guarantees.

Moruno retains its own branding and icon artwork. JACS / ACS structure styling and physical export dimensions are unchanged. The white workspace is not a new printable page-layout system. Native accessibility coverage, native application menus, rich chemical labels, fragment attachment, and the broader chemistry roadmap remain separate work.

## Desktop checks

- Opened the rebuilt app and checked the default molecule and JACS indicator.
- Hid and restored the inspector, then resized the window; controls remained usable and Fit adapted.
- Started a new document, selected Ring using `R`, enabled aromaticity, placed a ring and checked `C6H6`.
- Opened Import with Cmd+I and typed `CNC`; typing stayed in the field rather than activating atom shortcuts. Enter inserted three atoms while preserving the ring.
- Moved the inserted molecule, selected Text using `T`, added `25 °C`, and saved through Cmd+Shift+S. The saved file had 9 atoms, 8 bonds, one annotation, and canonical `CNC.c1ccccc1`.
- Opened Export with Cmd+E and generated a valid vector PDF through the new inspector.
- Inspected the grouped template library and selection-specific controls.

Automated checks: 21 Rust tests, 10 Python tests, formatting and Clippy. The new regression test verifies viewport fitting without modifying the document or overwriting a manually panned camera. A final visual pass corrected heavy divider backgrounds to thin separators.
