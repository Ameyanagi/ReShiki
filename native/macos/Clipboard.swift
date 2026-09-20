import AppKit
import Foundation

@main
enum ClipboardRunner {
    static func main() {
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
            case "read_picture": response = try read(NSPasteboard.general, imageOnly: true)
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
    }
}
