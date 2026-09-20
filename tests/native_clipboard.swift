import AppKit
import Foundation

@main
enum ClipboardTests {
    static func main() throws {
        let board = NSPasteboard.withUniqueName()
        defer { board.releaseGlobally() }
        let native = Representation(type: "dev.moruno.drawing", data: Data("native".utf8))
        let png = Representation(type: "public.png", data: Data("png".utf8))
        let text = Representation(type: "public.utf8-plain-text", data: Data("https://example.invalid/picture".utf8))
        func check(_ condition: Bool, _ message: String) throws {
            if !condition { throw ClipboardError.message(message) }
        }
        _ = try write(board, [text, png, native])
        var result = try read(board).representations
        try check(result.count == 1 && result.first?.type == native.type, "Editable data must take priority")
        result = try read(board, imageOnly: true).representations
        try check(result.count == 1 && result.first?.data == png.data, "Explicit picture paste must bypass editable data")
        _ = try write(board, [text, png])
        result = try read(board).representations
        try check(result.first?.type == png.type, "Pictures must take priority over accompanying URL text")
        for type in pictureTypes {
            let image = Representation(type: type, data: Data(type.utf8))
            _ = try write(board, [image])
            result = try read(board).representations
            try check(result.count == 1 && result.first?.type == type && result.first?.data == image.data, "Picture format lost: \(type)")
        }
        _ = try write(board, [native])
        result = try read(board, imageOnly: true).representations
        try check(result.isEmpty, "Picture paste must not substitute molecular data")
        do {
            _ = try write(board, [png, png])
            throw ClipboardError.message("Duplicate representations were accepted")
        } catch {
            try check(try read(board).representations.first?.data == native.data, "Invalid write replaced the old clipboard")
        }
        print("Private pasteboard format, priority, and failed-write checks passed")
    }
}
