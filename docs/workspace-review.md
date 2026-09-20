# Workspace review and redesign

Reviewed Moruno's desktop workspace on 2026-09-16 using a controlled aspirin drawing. No existing research document was edited.

## Observations

The workspace should devote most of the window to drawing, keep frequently used tools visible, and show properties relevant to the current selection.

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

## Hover hints

All hover hints use a shared 12 pt style with an opaque dark background, white text, rounded corners and consistent padding. They appear after 500 ms, wrap at 300 pixels and stay within the window. Drawing-tool hints appear to the right of the palette; command and formatting hints appear below their controls. The labeled color-scope menu has no additional hover overlay, so its open choices remain unobstructed.

Desktop verification on 2026-09-20 checked the unobstructed color-scope menu, the Keyboard shortcuts hint and the Graphic line hint in a separate QA window. Formatting, strict Clippy and signed-bundle verification passed.

## View controls and compact templates

**View** in the bottom bar opens independent **Grid**, **Rulers** and **Crosshair** switches and a unit selector: millimetres, centimetres, inches or points. The rulers use the same physical scale as publication exports (a default 42-unit bond measures 14.4 pt / 5.08 mm). Their tick spacing adapts to zoom; values track a fixed drawing origin through zoom and pan. Positive X runs right and positive Y runs down. This origin is not a printable page corner.

Rulers occupy separate top and left strips, where drawing gestures cannot create objects. The crosshair follows the pointer on the drawing surface, leaves a small gap at its centre and shows coordinates in the chosen units. Pointer markers on both rulers help alignment even with the full crosshair hidden. Leaving the canvas hides these markers and the crosshair. View choices remain available across documents in the current session, do not enter Undo history, and do not appear in exports. Screen zoom is not calibrated actual-size view.

Template results use a three-column grid of small structure icons, with names in the shared hover style. Search and filters remain above the grid; selecting an icon opens the named attachment preview and existing placement controls.

Desktop verification on 2026-09-20 checked independent switches, mm/pt ruler labels, pointer markers and coordinates at different zoom levels, hidden markers outside the canvas, and template hover names. With rulers enabled, drew a single bond, chose the pyridine icon and attached it to that bond: the saved result contained six atoms and six bonds. Undo restored the single bond; Redo restored the ring. Automated checks cover physical units, pan/zoom coordinate mapping, ruler-strip input exclusion, and unchanged document/history/export data. All **130 Rust tests**, formatting, strict Clippy and signed-bundle verification passed. Local evidence is under ignored `artifacts/template-grid-qa-20260920/`.
