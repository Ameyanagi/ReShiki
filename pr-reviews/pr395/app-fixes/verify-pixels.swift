import AppKit
import Foundation
let names = ["P01-perspective-20.png", "P02-perspective-50.png", "P03-perspective-100.png", "P04-back-to-20.png", "P05-perspective-100-rotated.png"]
var out: [[String: Any]] = []
var baselineMask: Set<Int> = []
for name in names {
    let rep = NSBitmapImageRep(data: try Data(contentsOf: URL(fileURLWithPath: name)))!
    guard rep.bitsPerSample == 8, rep.samplesPerPixel == 4, !rep.isPlanar, let p = rep.bitmapData else { fatalError("Unexpected pixel layout") }
    var count = 0
    var rows: [[Int]] = []
    var cols: [[Int]] = []
    var mask: Set<Int> = []
    func match(_ x: Int, _ y: Int) -> Bool {
        let i = y * rep.bytesPerRow + x * 4
        return p[i] == 62 && p[i+1] == 133 && p[i+2] == 117
    }
    for y in 364..<1620 {
        var runStart = -1
        for x in 208...1960 {
            let hit = x < 1960 && match(x, y)
            if hit { count += 1; mask.insert(y * rep.pixelsWide + x); if runStart == -1 { runStart = x } }
            else if runStart >= 0 { if x-runStart >= 300 { rows.append([y,runStart,x-1,x-runStart]) }; runStart = -1 }
        }
    }
    for x in 208..<1960 {
        var runStart = -1
        for y in 364...1620 {
            let hit = y < 1620 && match(x,y)
            if hit { if runStart == -1 { runStart = y } }
            else if runStart >= 0 { if y-runStart >= 300 { cols.append([x,runStart,y-1,y-runStart]) }; runStart = -1 }
        }
    }
    var bx0 = 9999, by0 = 9999, bx1 = -1, by1 = -1
    for y in 1470..<1620 {
        for x in 700..<1300 {
            let i = y * rep.bytesPerRow + x * 4
            if p[i] == 214 && p[i+1] == 219 && p[i+2] == 223 {
                bx0 = min(bx0,x); by0 = min(by0,y); bx1 = max(bx1,x); by1 = max(by1,y)
            }
        }
    }
    if name.hasPrefix("P01") { baselineMask = mask }
    out.append(["file": name,"width":rep.pixelsWide,"height":rep.pixelsHigh,"exactColorCanvasCount":count,"horizontalRuns_y_x0_x1_length":rows,"verticalRuns_x_y0_y1_length":cols,"barBorderCoreBoundsInclusive":[bx0,by0,bx1,by1],"frameColorMaskDifferenceVersusP01":baselineMask.symmetricDifference(mask).count])
}
let data = try JSONSerialization.data(withJSONObject: out, options: [.prettyPrinted, .sortedKeys])
try data.write(to: URL(fileURLWithPath: "independent-pixel-check.json"))
print(String(data:data,encoding:.utf8)!)
