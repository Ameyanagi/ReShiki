import AppKit
import Foundation

// A single-item, multiple-representation pasteboard bridge. The editor invokes
// this process on explicit Copy/Paste; it never monitors the clipboard.
struct Representation: Codable {
    let type: String
    let data: Data
}
struct Request: Decodable {
    let operation: String
    let representations: [Representation]?
}
struct Response: Encodable {
    let representations: [Representation]
}
enum ClipboardError: Error, CustomStringConvertible {
    case message(String)
    var description: String {
        switch self { case .message(let text): return text }
    }
}
let limit = 64 * 1024 * 1024
let readable = [
    "dev.moruno.drawing", "com.revvity.chemdraw.cdx-clipboard",
    "com.perkinelmer.chemdraw.cdx-clipboard", "com.cambridgesoft.cdx",
    "com.revvity.cdx", "com.perkinelmer.cdx", "com.revvity.cdxml",
    "com.perkinelmer.cdxml", "com.cambridgesoft.cdxml",
    "chemical/x-cdxml", "com.mdli.molfile", "org.opensmiles.smiles",
    "public.utf8-plain-text", "public.png", "com.adobe.pdf", "public.svg-image"
]
func read(_ board: NSPasteboard) throws -> Response {
    guard let item = board.pasteboardItems?.first else { return Response(representations: []) }
    var result: [Representation] = []
    var size = 0
    for name in readable {
        let type = NSPasteboard.PasteboardType(name)
        guard item.types.contains(type), let data = item.data(forType: type) else { continue }
        guard data.count <= limit - size else { throw ClipboardError.message("Clipboard data exceeds the 64 MB limit") }
        size += data.count
        result.append(Representation(type: name, data: data))
        // One best editable representation suffices. Do not fetch a large image
        // or unrelated text when native structure data is available.
        if name == "dev.moruno.drawing" || name.contains("cdx") || name == "com.mdli.molfile" || name == "org.opensmiles.smiles" || name == "public.utf8-plain-text" { break }
    }
    return Response(representations: result)
}
func write(_ board: NSPasteboard, _ representations: [Representation]) throws -> Response {
    guard !representations.isEmpty, representations.count <= 20 else { throw ClipboardError.message("No supported clipboard representations") }
    let item = NSPasteboardItem()
    var total = 0
    var names = Set<String>()
    for representation in representations {
        guard readable.contains(representation.type), names.insert(representation.type).inserted,
              !representation.data.isEmpty, representation.data.count <= limit - total else {
            throw ClipboardError.message("Invalid or oversized clipboard representation")
        }
        total += representation.data.count
        guard item.setData(representation.data, forType: NSPasteboard.PasteboardType(representation.type)) else {
            throw ClipboardError.message("Could not prepare clipboard data")
        }
    }
    // Prepare all data before replacing the clipboard. Copy does not fetch
    // unrelated previous contents (which may be enormous or lazily generated).
    // The editor deletes a Cut selection only after this write succeeds.
    board.clearContents()
    guard board.writeObjects([item]) else {
        throw ClipboardError.message("Could not write clipboard data")
    }
    return Response(representations: [])
}
do {
    // Base64 JSON adds one third overhead. Bound stdin before decoding it.
    var input = Data()
    while true {
        let chunk = FileHandle.standardInput.readData(ofLength: 65536)
        if chunk.isEmpty { break }
        guard chunk.count <= limit * 2 - input.count else { throw ClipboardError.message("Clipboard request is too large") }
        input.append(chunk)
    }
    let request = try JSONDecoder().decode(Request.self, from: input)
    let response: Response
    switch request.operation {
    case "read": response = try read(NSPasteboard.general)
    case "write": response = try write(NSPasteboard.general, request.representations ?? [])
    default: throw ClipboardError.message("Unsupported clipboard operation")
    }
    let output = try JSONEncoder().encode(response)
    FileHandle.standardOutput.write(output)
} catch {
    let message = error is ClipboardError ? String(describing: error) : "Invalid clipboard request or data"
    FileHandle.standardError.write(Data(message.utf8))
    exit(1)
}
