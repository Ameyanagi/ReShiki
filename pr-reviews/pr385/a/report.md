# ChemDraw MeO atom-label justification verification

Created and saved using ChemDraw 26.1.0.6801 at `/Applications/ChemDraw.app` on 2026-10-20. All artifacts are in this directory. Drawing, formatting, saving, reopening for screenshots, and closing were performed through computer use. The native ChemDraw files were not edited or synthesized outside ChemDraw.

## Menu paths and procedure

- Created a new blank document with **File > New from Stationery > New Document**. The initial File menu's default new-document command was “New ACS Document 1996”; the New Document stationery was chosen explicitly. The new window was initially “Untitled Document-4”.
- Used the **Solid Bond** tool (`x`) and one canvas click to create the single bond. Used the **Text** tool (`t`) at the left endpoint and entered `MeO`. Preliminary drawing attempts were cleared before the final single-bond structure was made.
- Selected the structure containing its sole atom label with **Edit > Select All** / Cmd+A. Applied **Text > Flush Left**, **Text > Centered**, **Text > Flush Right**, and **Text > Automatic**. These are direct Text menu items, without an intervening Justification submenu.
- The exact alignment choices are **Flush Left**, **Centered**, **Flush Right**, **Justified**, **Stacked Above**, and **Automatic**. **Justified** was disabled for this atom label.
- Saved through **File > Save As…** (also invoked with Cmd+Option+Shift+S). The format names are exactly **ChemDraw XML** for `.cdxml` and **ChemDraw** for `.cdx`. XML used “Line feed (Mac OS X and UNIX default)”. Separate filenames were established before changing each later variant so earlier saved variants were preserved.
- Menu-bar screenshot capture was unavailable in the computer-use tool. The required menu screenshots were successfully captured using the equivalent atom-label context menu: **right-click the label > Alignment**. Each screenshot shows a checkmark beside the saved setting. The blue hover highlight on Flush Left in the Centered/Flush Right screenshots is not the active setting; the checkmark identifies it.

## Saved files and visible results

The bond slopes upward to the right. Its left endpoint remains fixed at document coordinates `(190, 158)`; its right endpoint remains `(215.97, 143)`. Approximate screenshot coordinates below refer to the 1120 × 1544 pixel captures, where the left atom is near `(400, 400)`.

| Setting | Native files | Visible label placement | Drawing screenshot | Menu screenshot |
|---|---|---|---|---|
| Flush Left | `meo-flush-left.cdxml`, `meo-flush-left.cdx` | Reads **MeO**. Text begins near the left atom and extends mainly rightward, approximately x=393–434. A red chemical-warning outline is visible. | `a-01-flush-left.png` | `a-01-flush-left-menu.png` |
| Centered | `meo-centered.cdxml`, `meo-centered.cdx` | Reads **MeO**. Text straddles the atom position, approximately x=380–421. A red chemical-warning outline is visible. | `a-02-centered.png` | `a-02-centered-menu.png` |
| Flush Right | `meo-flush-right.cdxml`, `meo-flush-right.cdx` | Reads **MeO**. Text extends mainly to the left, approximately x=366–407, with O nearest the bond. No red warning outline is visible. | `a-03-flush-right.png` | `a-03-flush-right-menu.png` |
| Automatic | `meo-auto.cdxml` | ChemDraw visibly rearranges the label to **OMe**, approximately x=363–405. A red chemical-warning outline is visible. Stored CDXML text is still `MeO`. | `a-04-auto.png` | Not requested; menu text recorded in `a-04-auto-menu.txt`. |

All seven image files are actual PNG files. The computer-use API supplied JPEG-encoded captures, which were converted to PNG without cropping, annotation, resizing, or content changes. The context submenus extend beyond the captured document window on the right, so part of “Stacked Above” is clipped; all three requested setting names and their checkmarks are visible.

## Native-file verification and limitation

All four XML files parse successfully and preserve the same outer fragment (`15`), atom IDs (`14` and `16`, with AtomID values `1` and `2`), atom positions, and bond (`17`, B=`14`, E=`16`). All four store exactly `MeO` in the same text style (font 60, size 10, color 0, face 96). The three binary CDX files are nonempty and have the `VjCD0100` native signature.

| Setting | Atom LabelDisplay | Text LabelJustification | Text Justification | Text p |
|---|---|---|---|---|
| Flush Left | `Left` | `Left` | omitted | `185.83 161.64` |
| Centered | `Center` | `Center` | `Center` | `190 161.64` |
| Flush Right | `Right` | `Right` | `Right` | `193.89 161.64` |
| Automatic | omitted | `Right` | `Right` | `192.78 161.64` |

**The files are not byte-for-byte or XML-attribute-only justification variants.** Only the label justification was deliberately changed between variants, but ChemDraw also recalculated bounding boxes, text origins, and chemical interpretation. In Flush Right, the labeled node becomes `NodeType="Fragment"`, its invalid-valence warning disappears, and ChemDraw adds a nested methoxy fragment with carbon, oxygen, an external connection point, and two internal bonds. Flush Left, Centered, and Automatic retain `NodeType="Unspecified"` and `Warning="An atom in this label has an invalid valence."` All retain `NeedsClean="yes"`. Thus the UI-only procedure cannot claim that the saved files differ solely in one justification attribute. These native effects were preserved for verification.

Document Name attributes also vary, and the Right/Automatic XML contain temporary-save suffixes internally despite their correct filesystem names. `native-comparison.json` records the parsed atom, bond, and text attributes for inspection. The `a-01-flush-left-menu.txt`, `a-02-centered-menu.txt`, `a-03-flush-right-menu.txt`, and `a-04-auto-menu.txt` files contain additional Text-menu accessibility evidence.

## Cleanup

The created test document and all generated-file windows reopened for screenshot verification were closed with Cmd+W. An unintended temporary duplicate produced while testing a Save As shortcut was discarded immediately and was never used as a deliverable. ChemDraw remains running. Its three pre-existing documents remain open: “Hotkeys Cheat Sheet for macOS.cdxml”, “Shortcuts Cheat Sheet for macOS.cdxml”, and “Untitled ACS Document 1996-1”; none was opened for editing, modified, saved, or closed by this task. `remaining-documents.txt` records the final Window menu. No other apps were closed, no system settings were changed, and no repository files were edited.
