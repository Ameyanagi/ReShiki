# ChemDraw clipboard round-trip check

Completed on 2026-10-20 in ChemDraw 26 (`/Applications/ChemDraw.app`). All eight candidates were pasted into separate new documents, captured on the canvas, selected with Cmd+A, copied with Cmd+C, captured with the supplied Swift clipboard script, and closed with Cmd+W → Delete without saving a ChemDraw document.

All eight labels read **Boc**, left-to-right; none reads **coB**. All G0 variants, G1-401-left, and G1-401-above have the same visible label/bond arrangement. G1-401-center and G1-401-right move the label toward NH, and their connecting bond is not visible.

## Per-candidate observations

“Right” and “left” below refer to the screenshot, not the candidate suffix. In the baseline arrangement, NH is lower-left of Boc and a short diagonal bond rises rightward toward B. B is nearest that bond; the remaining “oc” extends to the right.

| Candidate | Label appearance relative to bond | Screenshot | Problems / outcome |
| --- | --- | --- | --- |
| G0-left | Reads **Boc**. B is nearest the upper-right end of the visible diagonal bond; “oc” extends rightward. Clear separation from NH. | [shots/G0-left.png](shots/G0-left.png) | No visible label defect. Paste and copy-back succeeded. |
| G0-center | Reads **Boc**. Same baseline arrangement: B nearest the bond, “oc” extending rightward, with the connecting bond visible. | [shots/G0-center.png](shots/G0-center.png) | No visible difference from G0-left. Paste and copy-back succeeded. |
| G0-right | Reads **Boc**. Same baseline arrangement: text extends to the right from B at the bond side. | [shots/G0-right.png](shots/G0-right.png) | No visible difference from G0-left. Paste and copy-back succeeded. |
| G0-above | Reads **Boc**. Same baseline arrangement, upper-right of NH; the diagonal bond remains visible and “oc” extends rightward. | [shots/G0-above.png](shots/G0-above.png) | No visible difference from G0-left. Paste and copy-back succeeded. |
| G1-401-left | Reads **Boc**. B is nearest the clearly visible diagonal bond, with “oc” extending rightward. Matches the G0 baseline. | [shots/G1-401-left.png](shots/G1-401-left.png) | No visible label defect. Paste and copy-back succeeded. |
| G1-401-center | Reads **Boc**. The label sits farther left relative to NH than the baseline: B is almost immediately above/right of H, while “oc” still extends rightward. | [shots/G1-401-center.png](shots/G1-401-center.png) | The NH–Boc connecting bond is not visible. The text has shifted toward the bond/NH region. Paste and copy-back succeeded. |
| G1-401-right | Reads **Boc**. The label sits still farther left relative to NH: B is directly above H, while “oc” extends rightward. | [shots/G1-401-right.png](shots/G1-401-right.png) | The NH–Boc connecting bond is not visible. B nearly touches the top of H; a very narrow white gap appears present, so definite glyph overlap is not claimed. Paste and copy-back succeeded. |
| G1-401-above | Reads **Boc**. Matches the baseline: B upper-right of NH, clear diagonal connecting bond, and “oc” extending rightward. | [shots/G1-401-above.png](shots/G1-401-above.png) | No visible label defect. Paste and copy-back succeeded. |

The short separate bond segment above-left of N is visible in every candidate. Observations describe canvas rendering only; they do not assert whether a non-visible connecting bond remains in the underlying chemical data.

## Captured clipboard files

Each candidate has a directory `back/NAME/` containing `types.json` and five nonempty representations from ChemDraw Copy:

| File | Clipboard type |
| --- | --- |
| `item-0-0.pdf` | `com.adobe.pdf` |
| `item-0-1.bin` | `com.revvity.chemdraw.cdx-clipboard` |
| `item-0-2.bin` | `com.mdli.sketchfile` |
| `item-0-3.bin` | `com.mdli.molfile` |
| `item-0-4.bin` | `org.opensmiles.smiles` |

All eight PNG files and all manifest-listed files were checked for existence and nonzero size; capture file sizes match their manifests. Clipboard payloads were not decoded.

| Candidate | Capture directory | Captured CDX bytes |
| --- | --- | --- |
| G0-left | [back/G0-left/](back/G0-left/) | 2045 |
| G0-center | [back/G0-center/](back/G0-center/) | 2045 |
| G0-right | [back/G0-right/](back/G0-right/) | 2045 |
| G0-above | [back/G0-above/](back/G0-above/) | 2045 |
| G1-401-left | [back/G1-401-left/](back/G1-401-left/) | 2050 |
| G1-401-center | [back/G1-401-center/](back/G1-401-center/) | 2055 |
| G1-401-right | [back/G1-401-right/](back/G1-401-right/) | 2055 |
| G1-401-above | [back/G1-401-above/](back/G1-401-above/) | 2055 |

## Workflow notes and preservation

- Every paste produced a hidden **Change Settings** dialog. It was brought forward through the **Window** menu, and **As Copied** was chosen in all eight cases.
- Each drawing was enlarged using four **View → Magnify** actions. Selection highlighting was cleared before the screenshot. All screenshots are 1120 × 1544 pixels. The computer-use capture API returned JPEG bytes; these were converted to the requested PNG format with `sips`, without cropping, resizing, or changing the depicted content.
- The new test documents were numbered 21 through 28. The first test window later displayed the automatic title “ChemDraw Unsaved Document”; it was still discarded through Delete. No Save action was taken.
- All eight temporary test documents were closed and discarded. At the end, the Window menu contained the same three pre-existing documents recorded before testing: `Hotkeys Cheat Sheet for macOS.cdxml`, `Shortcuts Cheat Sheet for macOS.cdxml`, and `Untitled ACS Document 1996-1`.
- No pre-existing document was edited or closed, no other app was closed, and no system setting was changed. Task output files were written only under C.
- A separate read-only screenshot review confirmed the visual comparisons above.
