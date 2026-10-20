# Screenshot measurements

Measured fresh PNG bytes with macOS `/usr/bin/swift` and AppKit `NSBitmapImageRep`; no live application or UI was accessed. All inputs are 2560 × 1704 pixels. Coordinates below use a top-left origin, with integer coordinates identifying pixel samples. Thus a two-pixel stroke occupying columns 664–665 has sample-center coordinate 664.5. To express the same center relative to the image's outer pixel edge, add 0.5 to each coordinate. Dimensions are unaffected. Logical dimensions at 2× are half the listed pixel dimensions.

The corrected `02-3d-shown.png` was read afresh at 122% zoom (SHA-256 `258174208f4b7d339e90db3b2788f3d89cee24c8e61adf2d4be125edc0746e3c`). The older 238% screenshot cached in an earlier image view was not used for these measurements.

| Screenshot(s) | Left / top stroke center | Right / bottom stroke center | Frame width × height | Frame center |
| --- | --- | --- | --- | --- |
| 02, 03, 04, 05b, 06, 07 | (664.5, 603.5) | (1408.5, 1347.5) | **744 × 744 px** / 372 × 372 pt | (1036.5, 975.5) |
| 08, 09 | (664.5, 603.5) | (1548.5, 1487.5) | **884 × 884 px** / 442 × 442 pt | (1106.5, 1045.5) |
| 05, zoomed | Visible left x=287.5 | Visible right x=1695.5 | Width **1408 px**; horizontal edges are outside the visible canvas | Horizontal center x=991.5 |

The exact solid selection-stroke RGB in decoded bitmap bytes is **(62, 133, 117)**. For the 744-pixel square, long straight strokes occupy columns **664–665 and 1408–1409**, and rows **603–604 and 1347–1348**. For the resized square, the right stroke occupies columns **1548–1549**, and the bottom occupies rows **1487–1488**. Handles and the rotation stem were excluded by measuring long straight runs. There is **zero measured frame displacement or size change** from 02 through ordinary rotation, released rotation, Shift-roll, and reset (03/04/06/07). Resizing moves the right and bottom edges by **+140 px**, retaining the left/top edges. Rotation after resizing (08→09) again changes neither frame position nor dimensions.

The visible white canvas occupies pixel columns **208–1959** and rows **364–1619**, or the half-open rectangle **[208,1960) × [364,1620)**, measuring **1752 × 1256 px**. Inspector divider pixels begin at x=1960; status-bar divider pixels begin at y=1620.

| Screenshot | Floating bar body raster envelope, inclusive | Size | Canvas margins: left / right / top / bottom |
| --- | --- | --- | --- |
| 02 and 04 | x≈760–1313, y≈446–516 | ≈554 × 71 px | ≈552 / 646 / 82 / 1103 px |
| 05, zoomed and clamped | x≈715–1268, y≈1543–1612 | ≈554 × 70 px | ≈507 / 691 / 1179 / **7 px** |

These envelopes include the hard rounded border and its antialiased edge, excluding the diffuse drop shadow. The border's exact darkest constant color is RGB **(214,219,223)**: its extremal pixels are x=761/1312 and y=447/515 in 02/04, and x=716/1267 and y=1543/1612 in 05. Antialiasing makes the outer body envelope uncertain by approximately one pixel; its nominal height is about 70 px. The zoomed body remains fully above the status bar, and its shadow extends into the seven-pixel gap, still ending before the divider. It also remains well left of the inspector. The 02 and 04 bar crops **[750,1330) × [440,535)** are pixel-identical.

In 03, **all 39,334 pixels** of the former bar body rectangle **[760,1314) × [446,517)** are white: the bar is absent during the drag. A mint hint strip occupies x=226–1941, y=1552–1601 (**1716 × 50 px**), displaying “Drag to rotate · Shift-drag to roll · Double-click to reset”. The bar returns in 04.

Reset restores the opening 3D orientation to raster precision. Comparing 02 and 07 inside **[674,1399) × [614,1338)**, while excluding the two orange keyboard-crosshair rectangles **[700,766) × [676,743)** and **[1240,1303) × [1035,1098)**, leaves 516,509 pixels. Only **222** differ; their maximum RGB-channel difference is **2/255**, with total absolute channel difference 315. The same comparison contains 41,305 nonwhite model pixels. These tiny color-rounding differences show no spatial discrepancy in the molecular geometry. Toolbar/status differences lie outside this comparison.

The additional original-layout check also passes. Fitting the circular selection markers for all **14 heavy-atom centers** in `01-selected-2d.png` and `14-original-restored.png` gives the uniform transform:

```text
x_restored = 0.5118824532 * x_original + 528.8222674
y_restored = 0.5118824532 * y_original + 483.9398452
```

No rotation or shear was needed. The center-fit residual is **0.129 px RMS**, **0.378 px maximum**, despite the zoom/translation change. Representative original→restored centers are top methyl **(786.535,586.174)→(931.486,783.925)**, left O **(533.460,895.546)→(801.847,942.404)**, and bottom O **(1069.227,1333.853)→(1076.216,1166.757)**. Marker centers were estimated by algebraically fitting circles to teal stroke pixels within 24 px of each approximate atom position; black bonds/labels occlude parts of some rings, so subpixel residuals should not be interpreted as exact atom-coordinate telemetry. This strongly supports restoration of the original 2D arrangement.

`frame-measurements.json` stores the measured bounds, comparison statistics, and all 14 center pairs. To reproduce the exact selection-stroke raster scan from the scratchpad directory, run:

```sh
swift - <<'SWIFT'
import AppKit
let names = ["02-3d-shown.png", "03-rotate-mid-drag.png",
 "04-rotate-released.png", "05-zoomed-bar-clamped.png",
 "05b-zoom-restored.png", "06-shift-roll.png", "07-double-click-reset.png",
 "08-resized.png", "09-resized-then-rotated.png"]
for name in names {
 let url = URL(fileURLWithPath: "ev386-1b-final/app/" + name)
 let im = NSBitmapImageRep(data: try! Data(contentsOf: url))!
 precondition(im.bitsPerSample == 8 && im.samplesPerPixel == 4)
 let d = im.bitmapData!, stride = im.bytesPerRow
 var columns = [Int](repeating: 0, count: im.pixelsWide)
 var rows = [Int](repeating: 0, count: im.pixelsHigh)
 for y in 364..<1620 { for x in 208..<1960 {
  let k = y * stride + x * 4
  if d[k] == 62 && d[k+1] == 133 && d[k+2] == 117 {
   columns[x] += 1; rows[y] += 1
  }
 }}
 print(name, "long columns:", columns.indices.filter { columns[$0] > 600 },
       "long rows:", rows.indices.filter { rows[$0] > 600 })
}
SWIFT
```

The evidence establishes the geometry visible in these captures. A screenshot pair cannot establish every intermediate event, native atom identity, or application-state persistence beyond the visible result.
