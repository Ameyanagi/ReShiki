# ChemDraw 26 clipboard round-trip check

Run date: 2026-10-20 (Asia/Tokyo). Application: /Applications/ChemDraw.app.

**Status:** All 15 candidates were pasted into separate new blank documents using **As Copied**, captured on the canvas, selected with Cmd+A, copied with Cmd+C, and captured using the requested Swift script. All 15 temporary documents were closed without saving. **The checked Text-menu item remains unverified for all 15 candidates because the computer-use interface did not expose checkmarks.**

## Results

“Bond-end letter” below describes what is visibly next to the drawn bond, not an atom identity inferred from the input file. The carbon is to the right. M denotes the common menu-observation limitation described below.

| Candidate | Checked Text-menu item after paste | Visible label / bond-end letter | Canvas screenshot | Problems |
|---|---|---|---|---|
| A-823-left | Not observable (M) | Reads **OMe**; **e** is nearest the visible right-hand bond. | [A-823-left.png](shots/A-823-left.png) | M; red jagged outline around label. |
| A-823-center | Not observable (M) | Reads **OMe**; **e** is nearest the visible right-hand bond. | [A-823-center.png](shots/A-823-center.png) | M; red jagged outline around label. |
| A-823-right | Not observable (M) | Reads **OMe**; **e** is nearest the visible right-hand bond. | [A-823-right.png](shots/A-823-right.png) | M; red jagged outline around label. |
| B-823-701-left | Not observable (M) | Reads **OMe**; **e** is nearest the visible right-hand bond. | [B-823-701-left.png](shots/B-823-701-left.png) | M; red jagged outline around label. |
| B-823-701-center | Not observable (M) | Reads **OMe**; **e** is nearest the visible right-hand bond. | [B-823-701-center.png](shots/B-823-701-center.png) | M; red jagged outline around label. |
| B-823-701-right | Not observable (M) | Reads **OMe**; **e** is nearest the visible right-hand bond. | [B-823-701-right.png](shots/B-823-701-right.png) | M; red jagged outline around label. |
| C-823-701-705-left | Not observable (M) | Reads **OMe**; **e** is nearest the visible right-hand bond. | [C-823-701-705-left.png](shots/C-823-701-705-left.png) | M; red jagged outline around label. |
| C-823-701-705-center | Not observable (M) | Reads **OMe**; **e** is nearest the visible right-hand bond. | [C-823-701-705-center.png](shots/C-823-701-705-center.png) | M; red jagged outline around label. |
| C-823-701-705-right | Not observable (M) | Reads **OMe**; **e** is nearest the visible right-hand bond. | [C-823-701-705-right.png](shots/C-823-701-705-right.png) | M; red jagged outline around label. |
| D-401-823-left | Not observable (M) | Reads **MeO**; no bond segment is discernible, so its bond-end letter cannot be identified visually. | [D-401-823-left.png](shots/D-401-823-left.png) | M; red jagged outline; bond not discernible. |
| D-401-823-center | Not observable (M) | Reads **MeO**; no bond segment is discernible, so its bond-end letter cannot be identified visually. | [D-401-823-center.png](shots/D-401-823-center.png) | M; red jagged outline; bond not discernible. |
| D-401-823-right | Not observable (M) | Reads **MeO—**; **O** is nearest the visible right-hand bond. | [D-401-823-right.png](shots/D-401-823-right.png) | M; no red outline. |
| E-401-823-701-left | Not observable (M) | Reads **MeO**; no bond segment is discernible, so its bond-end letter cannot be identified visually. | [E-401-823-701-left.png](shots/E-401-823-701-left.png) | M; red jagged outline; bond not discernible. |
| E-401-823-701-center | Not observable (M) | Reads **MeO**; no bond segment is discernible, so its bond-end letter cannot be identified visually. | [E-401-823-701-center.png](shots/E-401-823-701-center.png) | M; red jagged outline; bond not discernible. |
| E-401-823-701-right | Not observable (M) | Reads **MeO—**; **O** is nearest the visible right-hand bond. | [E-401-823-701-right.png](shots/E-401-823-701-right.png) | M; no red outline. |

## Menu-observation limitation (M)

After Cmd+A, the Text menu was opened for each candidate. Its accessibility tree listed Flush Left, Centered, Flush Right, and Automatic, but carried no checked/selected marker for any of them. The native computer-use screenshot method returned “Screenshot unavailable” while the menu was open. An attempted desktop screenshot did not contain the open menu and was discarded. No menu item was selected or changed to manufacture a result.

The actual menu trees are preserved as **shots/NAME-menu.txt**. These support that the menu was inspected, but do not establish the checked item. No valid **NAME-menu.png** is included. Serialized CDX properties below are supplemental evidence and must not be reported as observed menu checkmarks.

The first paste initially appeared blank because its Change Settings dialog was not the exposed window. The dialog was located through the Window menu and **As Copied** was chosen. The same explicit As Copied choice was made for all subsequent candidates. The first canvas screenshot was replaced after the structure became visible.

## Supplemental clipboard evidence

Each **back/NAME/** contains the unmodified output of the requested capture script, including **types.json** and all five representations: PDF, com.revvity.chemdraw.cdx-clipboard, MDL sketchfile, MDL molfile, and SMILES. All 75 payload files are nonempty, and every size matches its manifest.

The returned CDX payloads were independently decoded with the repository’s local reference.engine.cdx_exchange.from_cdx decoder. All 15 decode successfully. Derived XML is in **decoded/NAME.cdxml** and the machine-readable summary is [roundtrip-properties.json](roundtrip-properties.json). “Absent” means not explicitly serialized; it is not an assumed default.

| Candidate | Text LabelJustification | Text Justification | Text LabelAlignment | Node LabelDisplay | Returned SMILES |
|---|---|---|---|---|---|
| A-823-left | Right | Right | Right | Absent | `[MeO]C` |
| A-823-center | Right | Right | Right | Absent | `[MeO]C` |
| A-823-right | Right | Right | Right | Absent | `[MeO]C` |
| B-823-701-left | Right | Right | Right | Absent | `[MeO]C` |
| B-823-701-center | Right | Right | Right | Absent | `[MeO]C` |
| B-823-701-right | Right | Right | Right | Absent | `[MeO]C` |
| C-823-701-705-left | Right | Right | Right | Absent | `[MeO]C` |
| C-823-701-705-center | Right | Right | Right | Absent | `[MeO]C` |
| C-823-701-705-right | Right | Right | Right | Absent | `[MeO]C` |
| D-401-823-left | Left | Absent | Absent | Left | `[MeO]C` |
| D-401-823-center | Center | Center | Absent | Center | `[MeO]C` |
| D-401-823-right | Right | Right | Absent | Right | `COC` |
| E-401-823-701-left | Left | Absent | Absent | Left | `[MeO]C` |
| E-401-823-701-center | Center | Center | Absent | Center | `[MeO]C` |
| E-401-823-701-right | Right | Right | Absent | Right | `COC` |

The stored CDX label text remains **MeO** in all 15 cases even when the canvas reads **OMe**. Each SMILES clipboard payload ends with a NUL byte, omitted from the table.

**D-401-823-right and E-401-823-701-right are the two candidates that visibly render MeO with O next to a visible bond and also return SMILES COC.** Their returned MeO node has NodeType=Fragment and contains a nested carbon–oxygen–external-connection-point structure. Their MOL representation contains 3 atoms and 2 bonds with a MeO SUP group. The other 13 returned CDX structures retain an Unspecified MeO pseudoatom linked to carbon and return [MeO]C. This is a difference in chemical interpretation as well as display.

D/E-center additionally have literal alias text M^eO in their MOL payloads, despite CDX text MeO.

## Document handling and evidence

Only the 15 newly created test documents (initially Untitled Document-6 through Untitled Document-20) were closed. ChemDraw’s discard action appeared as **Delete**, with accessibility ID DontSaveButton, and was used instead of Save. Some temporary windows automatically acquired the suggested title “ChemDraw Unsaved Document” before being discarded; no Save command was used.

The pre-existing documents **Hotkeys Cheat Sheet for macOS.cdxml**, **Shortcuts Cheat Sheet for macOS.cdxml**, and **Untitled ACS Document 1996-1** remained listed in the final Window menu; see [final-windows.txt](final-windows.txt). They were not edited or closed. No other applications were closed, and no system settings were changed.

Canvas screenshots show the selection highlight for most candidates. D-left and D-center were also inspected with the selection cleared, and their final screenshots show that state; their red outline and lack of a discernible bond persisted. Every candidate was selected again when its Text menu and copied structure were examined.

The remaining unresolved measurement is the actual Text-menu checkmark. It requires a UI observation method that exposes the menu’s checkmark state.
