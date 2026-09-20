import AppKit
import Foundation

@main
struct PrintMain {
    static func main() {
        do {
            var input=Data()
            while true {
                let chunk=FileHandle.standardInput.readData(ofLength:8192)
                if chunk.isEmpty { break }
                guard chunk.count <= 65536-input.count else { throw PrintError.message("Print request is too large") }
                input.append(chunk)
            }
            let request=try JSONDecoder().decode(PrintRequest.self,from:input)
            let document=try loadSnapshot(request)
            let app=NSApplication.shared
            app.setActivationPolicy(.accessory)
            app.finishLaunching()
            let operation=try printOperation(document:document,title:request.title)
            app.activate(ignoringOtherApps:true)
            // false means cancellation or failure; never report it as successful output.
            let result=PrintResponse(completed:operation.run())
            FileHandle.standardOutput.write(try JSONEncoder().encode(result))
        } catch {
            let message=error is PrintError ? String(describing:error) : "Could not open the print snapshot"
            FileHandle.standardError.write(Data(message.utf8))
            exit(1)
        }
    }
}
