// Pasteboard probe helper.
//   pb put TYPE FILE [TYPE FILE ...] [-- TYPE FILE ...]   ("--" starts a new pasteboard item)
//   pb dump OUTDIR                                         (every item, every type, bytes to files)
import AppKit

let args = Array(CommandLine.arguments.dropFirst())
let pb = NSPasteboard.general

func put(_ rest: [String]) {
    var items: [NSPasteboardItem] = []
    var cur = NSPasteboardItem()
    var i = 0
    while i < rest.count {
        if rest[i] == "--" { items.append(cur); cur = NSPasteboardItem(); i += 1; continue }
        let type = NSPasteboard.PasteboardType(rest[i])
        let data = try! Data(contentsOf: URL(fileURLWithPath: rest[i + 1]))
        cur.setData(data, forType: type)
        i += 2
    }
    items.append(cur)
    pb.clearContents()
    print(pb.writeObjects(items) ? "wrote \(items.count) item(s)" : "write failed")
}

func dump(_ dir: String) {
    try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
    let items = pb.pasteboardItems ?? []
    print("changeCount=\(pb.changeCount) items=\(items.count)")
    for (n, item) in items.enumerated() {
        for t in item.types {
            let data = item.data(forType: t) ?? Data()
            let safe = t.rawValue.replacingOccurrences(of: "/", with: "_")
            let path = "\(dir)/item\(n)__\(safe)"
            try? data.write(to: URL(fileURLWithPath: path))
            print("item\(n)\t\(t.rawValue)\t\(data.count)")
        }
    }
}

switch args.first {
case "put": put(Array(args.dropFirst()))
case "dump": dump(args[1])
default: print("usage: pb put|dump")
}
