# Less crowded interface (#78)

Resolves the layout and control cleanup requested in
[issue #78](https://github.com/Ameyanagi/ReShiki/issues/78). The
[release notes](../changes-0.10.md#less-crowded-interface) describe the
changes; this record holds the screenshots and how they were made.

## Capture

Every image is application renderer output from the opt-in
`ui_layout_snapshots` test (`src/app/workspace/layout_snapshots.rs`), which
draws the real Iced widget tree with the headless renderer at the default
1280 × 820 window and the 1040 × 680 minimum. Each state sets up the same
document and tool on both sides.

- **Before:** commit `fc0884f`, the interface before the cleanup plus the
  snapshot test.
- **After:** source `c7f6853`. A fresh run matched every committed image byte
  for byte.

```sh
RESHIKI_UI_QA_DIR=/tmp/reshiki-ui cargo test --locked --bin reshiki ui_layout_snapshots -- --ignored --nocapture
```

The headless renderer does not show the native Save / Don't Save / Cancel
dialog, the file picker or Finder and Explorer drag-and-drop. The **unsaved**
state therefore shows only that no banner pushes the canvas down.

## Matched states

| State                                                | Before, 1280 × 820                                          | After, 1280 × 820                                         | 1040 × 680                                                                                                            |
| ---------------------------------------------------- | ----------------------------------------------------------- | --------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| Empty canvas, Select tool                            | ![Before](../images/ui-declutter/before/default-1280.png)   | ![After](../images/ui-declutter/after/default-1280.png)   | [Before](../images/ui-declutter/before/default-1040.png) · [After](../images/ui-declutter/after/default-1040.png)     |
| Benzene selected, inspector open                     | ![Before](../images/ui-declutter/before/molecule-1280.png)  | ![After](../images/ui-declutter/after/molecule-1280.png)  | [Before](../images/ui-declutter/before/molecule-1040.png) · [After](../images/ui-declutter/after/molecule-1040.png)   |
| Ring, arrow and rectangle selected (arrange)         | ![Before](../images/ui-declutter/before/mixed-1280.png)     | ![After](../images/ui-declutter/after/mixed-1280.png)     | [Before](../images/ui-declutter/before/mixed-1040.png) · [After](../images/ui-declutter/after/mixed-1040.png)         |
| Ring tool                                            | ![Before](../images/ui-declutter/before/ring-tool-1280.png) | ![After](../images/ui-declutter/after/ring-tool-1280.png) | [Before](../images/ui-declutter/before/ring-tool-1040.png) · [After](../images/ui-declutter/after/ring-tool-1040.png) |
| Arc selected                                         | ![Before](../images/ui-declutter/before/arc-1280.png)       | ![After](../images/ui-declutter/after/arc-1280.png)       | [Before](../images/ui-declutter/before/arc-1040.png) · [After](../images/ui-declutter/after/arc-1040.png)             |
| Import (drawer before, inspector tab after)          | ![Before](../images/ui-declutter/before/import-1280.png)    | ![After](../images/ui-declutter/after/import-1280.png)    | [Before](../images/ui-declutter/before/import-1040.png) · [After](../images/ui-declutter/after/import-1040.png)       |
| Precise transform controls                           | ![Before](../images/ui-declutter/before/transform-1280.png) | ![After](../images/ui-declutter/after/transform-1280.png) | [Before](../images/ui-declutter/before/transform-1040.png) · [After](../images/ui-declutter/after/transform-1040.png) |
| Unsaved changes (banner before, native dialog after) | ![Before](../images/ui-declutter/before/unsaved-1280.png)   | ![After](../images/ui-declutter/after/unsaved-1280.png)   | [Before](../images/ui-declutter/before/unsaved-1040.png) · [After](../images/ui-declutter/after/unsaved-1040.png)     |

## States without a before image

These states were added to the snapshot test during the cleanup, so there is
no before counterpart. Each exists at both sizes.

| State                                    | 1280 × 820                                                              | 1040 × 680                                                         |
| ---------------------------------------- | ----------------------------------------------------------------------- | ------------------------------------------------------------------ |
| Color popover                            | ![Color popover](../images/ui-declutter/after/color-popover-1280.png)   | [Image](../images/ui-declutter/after/color-popover-1040.png)       |
| Edit hues                                | ![Edit hues](../images/ui-declutter/after/edit-hues-1280.png)           | [Image](../images/ui-declutter/after/edit-hues-1040.png)           |
| Recovery offer in the status bar         | ![Recovery offer](../images/ui-declutter/after/recovery-1280.png)       | [Image](../images/ui-declutter/after/recovery-1040.png)            |
| Insert ▾ menu on the Import tab          | ![Insert menu](../images/ui-declutter/after/import-menu-1280.png)       | [Image](../images/ui-declutter/after/import-menu-1040.png)         |
| Help panel with shortcut labels          | ![Help panel](../images/ui-declutter/after/help-1280.png)               | [Image](../images/ui-declutter/after/help-1040.png)                |
| Ring tool with the Assistant tab         | ![Ring tool](../images/ui-declutter/after/ring-tool-assistant-1280.png) | [Image](../images/ui-declutter/after/ring-tool-assistant-1040.png) |
| Benzene selected with the Assistant tab  | ![Benzene](../images/ui-declutter/after/molecule-assistant-1280.png)    | [Image](../images/ui-declutter/after/molecule-assistant-1040.png)  |
| One atom selected with the Assistant tab | ![One atom](../images/ui-declutter/after/atom-assistant-1280.png)       | [Image](../images/ui-declutter/after/atom-assistant-1040.png)      |
| Chain tool with the Assistant tab        | ![Chain tool](../images/ui-declutter/after/chain-assistant-1280.png)    | [Image](../images/ui-declutter/after/chain-assistant-1040.png)     |

## Checks

- `every_context_row_fits_the_minimum_window` lays out every tool and selection
  row at 1040 × 680, with the inspector closed or open on any tab, including
  the wider Assistant tab.
- `selection_layout_and_double_click_regression` still passes: selecting keeps
  the canvas and the clicked atom in place.
- `the_color_popover_keeps_pointer_input_from_the_canvas`,
  `the_insert_menu_closes_like_the_other_menus` and
  `command_keys_never_type_into_fields_and_enter_leaves_them` send real pointer
  and key events through the widget tree.

```sh
cargo test --locked --bin reshiki layout_snapshots -- --ignored
cargo test --locked --bin reshiki selection_layout_and_double_click_regression -- --ignored
```

A native walkthrough on macOS and Windows, as the issue's release review asks,
is not part of this record.
