// Independent AppKit PNG decoder and exact-pixel measurement for Run 2.
// Usage: swift verify-pixels-run2.swift > independent-pixel-check-run2.json
import AppKit
import Foundation

let names = ["Q01-perspective-20-zoomed-out.png", "Q02-perspective-50-zoomed-out.png", "Q03-perspective-100-zoomed-out.png", "Q04-perspective-100-rotated-zoomed-out.png", "Q05-back-to-20-zoomed-out.png"]
let minRun = 100
var measurements: [[String: Any]] = []
var masks: [String: Set<Int>] = [:]
var straightMasks: [String: Set<Int>] = [:]

for name in names {
    guard let rep = NSBitmapImageRep(data: try Data(contentsOf: URL(fileURLWithPath: name))),
          rep.bitsPerSample == 8, rep.samplesPerPixel == 4, !rep.isPlanar,
          let p = rep.bitmapData else { fatalError("Unexpected pixel layout: \(name)") }
    let width = rep.pixelsWide
    func matches(_ x: Int, _ y: Int, _ color: [UInt8]) -> Bool {
        let i = y * rep.bytesPerRow + x * 4
        return p[i] == color[0] && p[i+1] == color[1] && p[i+2] == color[2]
    }
    let green: [UInt8] = [62, 133, 117]
    let gray: [UInt8] = [214, 219, 223]
    var frameMask = Set<Int>()
    var straightMask = Set<Int>()
    var horizontal: [[Int]] = []
    var vertical: [[Int]] = []
    var grayHorizontal: [[Int]] = []

    for y in 364...1619 {
        var start = -1
        var grayStart = -1
        for x in 208...1960 {
            let frameHit = x < 1960 && matches(x, y, green)
            if frameHit {
                frameMask.insert(y * width + x)
                if start < 0 { start = x }
            } else if start >= 0 {
                if x-start >= minRun {
                    horizontal.append([y, start, x-1, x-start])
                    for xx in start..<x { straightMask.insert(y * width + xx) }
                }
                start = -1
            }
            let grayHit = x < 1960 && matches(x, y, gray)
            if grayHit {
                if grayStart < 0 { grayStart = x }
            } else if grayStart >= 0 {
                if x-grayStart >= minRun { grayHorizontal.append([y, grayStart, x-1, x-grayStart]) }
                grayStart = -1
            }
        }
    }
    for x in 208...1959 {
        var start = -1
        for y in 364...1620 {
            let hit = y < 1620 && matches(x, y, green)
            if hit {
                if start < 0 { start = y }
            } else if start >= 0 {
                if y-start >= minRun {
                    vertical.append([x, start, y-1, y-start])
                    for yy in start..<y { straightMask.insert(yy * width + x) }
                }
                start = -1
            }
        }
    }
    guard let left = vertical.map({ $0[0] }).min(), let right = vertical.map({ $0[0] }).max(),
          let top = horizontal.map({ $0[0] }).min(), let bottom = horizontal.map({ $0[0] }).max() else { fatalError("Incomplete frame") }

    var bestPair: ([[Int]], Int)? = nil
    for a in grayHorizontal {
        for b in grayHorizontal {
            let overlap = min(a[2], b[2]) - max(a[1], b[1]) + 1
            if (48...100).contains(b[0]-a[0]) && overlap >= 300 && abs(a[1]-b[1]) <= 8 && abs(a[2]-b[2]) <= 8 {
                if bestPair == nil || overlap > bestPair!.1 { bestPair = ([a,b], overlap) }
            }
        }
    }
    guard let pair = bestPair?.0 else { fatalError("No bar border") }
    var bx0 = 9999, by0 = 9999, bx1 = -1, by1 = -1, borderCount = 0
    let scanX0 = max(208, min(pair[0][1], pair[1][1])-32)
    let scanX1 = min(1959, max(pair[0][2], pair[1][2])+32)
    for y in max(364,pair[0][0]-4)...min(1619,pair[1][0]+4) {
        for x in scanX0...scanX1 where matches(x,y,gray) {
            bx0 = min(bx0,x); by0 = min(by0,y); bx1 = max(bx1,x); by1 = max(by1,y)
            borderCount += 1
        }
    }
    let placement = by1 < top ? "above" : by0 > bottom ? "below" : "vertical overlap"
    let gap = placement == "above" ? top-by1-1 : by0-bottom-1
    let id = String(name.prefix(3))
    masks[id] = frameMask
    straightMasks[id] = straightMask
    measurements.append([
        "file": name, "size": [width,rep.pixelsHigh],
        "exactColorCanvasCount": frameMask.count,
        "horizontalRuns_y_x0_x1_length": horizontal,
        "verticalRuns_x_y0_y1_length": vertical,
        "exactColorLongStraightRunCount": straightMask.count,
        "frameCoreEnvelopeInclusive": [left,top,right,bottom],
        "frameCoreEnvelopeSize": [right-left+1,bottom-top+1],
        "barBorderCoreBoundsInclusive": [bx0,by0,bx1,by1],
        "barBorderExactColorCount": borderCount,
        "barPlacement": placement,
        "barFrameCorePixelRowSeparation": gap,
        "barCanvasBottomClearance": 1619-by1
    ])
}
var comparisons: [String: Any] = [:]
for (a,b) in [("Q03","Q04"),("Q01","Q05")] {
    comparisons["\(a)_versus_\(b)"] = [
        "exactColorCanvasMaskSymmetricDifferencePixels": masks[a]!.symmetricDifference(masks[b]!).count,
        "longStraightRunMaskSymmetricDifferencePixels": straightMasks[a]!.symmetricDifference(straightMasks[b]!).count
    ]
}
let output: [String: Any] = ["decoder": "NSBitmapImageRep via AppKit; independent of Python stdlib decoder", "conventions": "0-based original screenshot pixels, inclusive bounds, exact RGB masks, long-run minimum 100 px", "measurements": measurements, "comparisons": comparisons]
let data = try JSONSerialization.data(withJSONObject: output, options: [.prettyPrinted,.sortedKeys])
print(String(data:data,encoding:.utf8)!)
