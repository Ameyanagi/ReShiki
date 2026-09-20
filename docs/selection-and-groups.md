# Selection, groups and composition

Implemented on 2026-09-20 and checked through the ReShiki desktop app and CDXML exchange.

## Workflow

- Choose rectangular selection (`V`) or freeform lasso (`L`). Drag around objects to select them. Shift adds a region; Option/Alt subtracts. Shift-click toggles an object or group. Escape cancels the gesture. Invert selection is available in Properties and with Shift+Cmd/Ctrl+A.
- Select a molecule and its caption, then Group (`Cmd/Ctrl+G`). Clicking or moving any member selects or moves the entire group. Copy, paste, duplicate, rotate, resize and alignment keep it together. Grouping includes complete connected molecules, so a grouping boundary does not divide a bond.
- Groups can contain other groups. Ungroup (`Shift+Cmd/Ctrl+G`) removes one outer level. Option/Alt-click or drag edits an individual member without ungrouping. **Integral group** disables this member-selection shortcut.
- With several objects selected, composition controls appear at the top of Properties. Align left/right/top/bottom edges or horizontal/vertical centers, or distribute groups and connected components. Alignment measures visible label and drawing bounds.
- **Add frame** fits brackets, parentheses, braces, a rectangle or rounded rectangle around the selection with 6 pt padding. It groups the new frame and complete selection in one Undo step. Frames remain editable graphics; they do not define polymer chemistry.
- The palette reserves space for its slim scrollbar, keeping both icon columns visible in a short window.

Native format version 5 stores groups and their integral setting. Versions 1–4 open with no groups by default. IDs are remapped on copy/paste, nested groups survive duplication, and deletion removes stale memberships. When deletion collapses nested groups into the same membership, the integral setting is retained. Cleanup and analysis preserve group membership. Undo/Redo restores groups and keeps newly restored group members selected.

## Interchange

CDXML supports nested groups containing supported molecules, captions, forward arrows and vector graphics. Export separates disconnected molecules into chemical fragments before placing them in their group. Import retains membership and the Integral attribute. Paired bracket strokes can become separate graphics inside a persistent group after exchange. A fill/stroke curve pair with identical geometry remains one styled graphic.

This is a supported object subset, not arbitrary CDXML compatibility. Existing text, arrow, graphic and chemistry restrictions still apply. Arbitrary inherited external formatting and unsupported group children are outside the tested boundary. If later bond editing connects a group member to an atom outside its group, CDXML export reports the boundary conflict; ungroup or regroup the complete molecule before exporting. Native saves retain the drawing.

The [published CDXML group specification](https://bobhanson.github.io/IUPAC-FAIRSpec/cdx_sdk/Group.htm) informed logical nesting and the Integral attribute.

## Verification

Desktop testing used a freeform native pointer gesture to select part of aspirin, grouped the molecule with its caption, moved and duplicated that group, and aligned the copies. The native version 5 document was reopened in the standalone app. Add frame and its Undo/Redo were exercised through the inspector. The exported CDXML retained separate group selection after an external save/reopen. The fixture under `tests/fixtures/` preserves two groups, each containing one 13-atom aspirin and its caption.

Current checks: **68 Rust tests and 19 Python tests**, formatting, Clippy and standalone bundle signature verification. Tests cover nested/integral groups, complete-molecule expansion, ID remapping, deletion repair, malformed membership, bounds-based alignment, concave lasso regions, modifier gestures, individual member dragging, group/frame history, multi-fragment chemistry initialization and mixed-object CDXML round trips. Local QA files are under ignored `artifacts/selection-qa-20260920/`.

The final desktop pass reopened the saved drawing, added a rectangle frame to the second group, undid/redid it, moved it by its caption, ungrouped/undid, aligned both framed groups and saved. The screenshot is `composition-desktop.jpg` and the native file is `framed-aspirin.reshiki` in that QA directory. The standalone worker imported the externally saved fixture from `/tmp`, retaining both groups and formula `C18H16O8`; the application engine check also passed with `RESHIKI_ROOT=/nonexistent`.
