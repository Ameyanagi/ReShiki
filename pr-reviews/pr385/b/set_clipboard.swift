// Put a CDX (and CDXML) drawing on the general pasteboard under the same
// types ReShiki writes on Copy. Usage: swift set_clipboard.swift file.cdx file.cdxml
import AppKit
import Foundation

let cdx = try Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[1]))
let cdxml = try Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[2]))
let item = NSPasteboardItem()
for type in ["com.revvity.chemdraw.cdx-clipboard", "com.perkinelmer.chemdraw.cdx-clipboard", "com.cambridgesoft.cdx"] {
    item.setData(cdx, forType: NSPasteboard.PasteboardType(type))
}
for type in ["com.revvity.cdxml", "com.perkinelmer.cdxml", "com.cambridgesoft.cdxml"] {
    item.setData(cdxml, forType: NSPasteboard.PasteboardType(type))
}
NSPasteboard.general.clearContents()
print(NSPasteboard.general.writeObjects([item]))
print(NSPasteboard.general.changeCount)
