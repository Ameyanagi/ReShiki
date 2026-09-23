// Save every representation produced by a deliberate ChemDraw Copy As action.
import AppKit
import Foundation

guard CommandLine.arguments.count == 2 else {
    throw NSError(domain: "Capture", code: 1, userInfo: [NSLocalizedDescriptionKey: "Output directory required"])
}
if CommandLine.arguments[1] == "--change-count" {
    print(NSPasteboard.general.changeCount)
    exit(0)
}
let directory = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
var results: [[String: Any]] = []
for (index, item) in (NSPasteboard.general.pasteboardItems ?? []).enumerated() {
    for (slot, type) in item.types.enumerated() {
        guard let data = item.data(forType: type), data.count <= 16 * 1024 * 1024 else { continue }
        let suffix: String
        if type.rawValue.contains("png") { suffix = "png" }
        else if type.rawValue.contains("3mf") { suffix = "3mf" }
        else if type.rawValue.contains("pdf") { suffix = "pdf" }
        else if type.rawValue.contains("tiff") { suffix = "tiff" }
        else if type.rawValue.contains("text") { suffix = "txt" }
        else { suffix = "bin" }
        let name = "item-\(index)-\(slot).\(suffix)"
        try data.write(to: directory.appendingPathComponent(name))
        results.append(["type": type.rawValue, "file": name, "bytes": data.count])
    }
}
let json = try JSONSerialization.data(withJSONObject: results, options: [.prettyPrinted, .sortedKeys])
try json.write(to: directory.appendingPathComponent("types.json"))
FileHandle.standardOutput.write(json)
