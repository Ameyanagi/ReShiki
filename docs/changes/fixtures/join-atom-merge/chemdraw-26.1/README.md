# ChemDraw 26.1 Join reference

This controlled, three-selected-atom fixture records one observed result in
Windows ChemDraw Prime 26.1.0.6327 on 2026-10-10. It supports the placement and
identity checks below; it does not establish a universal ChemDraw Join algorithm
or claim that a ReShiki build has passed native acceptance.

## Original files and actions

`join-three-arms-valid-input-v2.cdxml` is an authored test input, not native
ChemDraw output. It has six atoms in three separate two-atom fragments. Selected
nodes are N1, C2 and O3; their unselected terminal carbons are 11, 12 and 13.
Its bond IDs 201–203 avoid a duplicate fragment/bond ID error in an earlier
private input; the invalid earlier input is not included.

ROOT, the sole native GUI operator, reports these actions:

1. Open the controlled input; select N first, then add C and O with Shift-click.
2. Choose Object → Join; use one toolbar Undo, then one Redo.
3. Save As the native AFTER CDXML, close that controlled document, and reopen
   the saved file in the same running ChemDraw process.
4. Reopen the unchanged authored original and Save As the separate native
   BEFORE CDXML without changing its graph.

**The native BEFORE file was saved subsequently from the reopened unchanged
original**, not before the original Join operation. Its six atom elements and
coordinates and three bond endpoints/orders match the authored input.

Raw captures retain their original bytes, 2556 × 1712 dimensions and ordinary
window chrome, including the private-network RDP title, taskbar and clock:

- [Selected N/C/O before Join](../../../../images/join-atom-merge/windows-join-three-selected-v2.jpg)
- [One Undo](../../../../images/join-atom-merge/windows-join-one-undo-v2.jpg)
- [Redo](../../../../images/join-atom-merge/windows-join-redo-v2.jpg)
- [Saved AFTER document reopened](../../../../images/join-atom-merge/windows-join-fresh-document-reopened-v2.jpg)

The snapshots support ROOT's observations; the XML itself does not prove action
order, first-selection order, Undo/Redo or document reopen. No fresh-process
reopen is claimed. The screenshots show a 100% view, not a physical-size check.

## Measured result

The native BEFORE text `BoundingBox` union for selected N1/C2/O3 is
`[194.94, 173.68, 305.06, 275.67]`, with midpoint `(250, 224.675)`.
Native AFTER places survivor N1 at `(250, 224.68)`: a 0.005-coordinate-unit
rounding difference. In this fixture, that result is consistent with centering
the selected visible label-ink bounds and excludes the arithmetic node mean
`(250, 210)`. The selected atom-position bounds center `(250, 225)` also differs.
Unselected terminal arms are excluded from this measured center interpretation.
Use each renderer's own label metrics; these native font offsets are not a
portable placement constant.

The saved graph has four atoms and three single bonds. N1 retains its element,
node ID, red color4 and Arial 14-point label runs. Terminal node IDs 11/12/13
retain positions `(100,180)`, `(400,180)` and `(250,370)` and their label/style
state. Bond IDs 201/202/203 retain all attributes except their necessary endpoint
remapping to `1–11`, `1–12`, `1–13`.

## Native save normalization and limits

- Arial font ID1/UTF-8 becomes ID3/ISO-8859-1 on native save; native BEFORE and
  AFTER label runs agree. Authored red `(0.85,0.1,0.1)` becomes
  `(0.8471,0.0980,0.0980)`, approximately 8-bit `(216,25,25)`.
- Native saves omit default carbon element and single-bond order attributes.
  They compute hidden implicit H counts N1/C2/O3 = 2/3/1 before Join, and N1 = 0
  after its three single bonds.
- Terminal auxiliary `AtomID` values change 4→3 and 6→4 for nodes 12 and 13;
  stable node `id` values do not change. N1 `Z` changes 2→13 as draw order,
  not evidence of 3D depth.
- Three fragments consolidate into fragment101. Native saves add AS/BS,
  NeedsClean and document/page/style metadata and resolve text coordinates and
  bounds. N1's text bounds translate with its displacement. Native output is
  not byte-identical to authored input.

This native comparison does not test four-endpoint fusion, hidden or multiline
labels, stereo, atom maps, groups, reactions or 3D behavior. Save/reopen confirms
this controlled document only. ReShiki acceptance evidence will be recorded
separately after the native application check.

`evidence-inventory.json` pins all seven unchanged originals and this authored
summary. Private About/license captures, unrelated documents, remote command
output, accessibility dumps and machine-path receipts are excluded.
