// Usage: swift drag.swift <pid> <x1> <y1> <x2> <y2> [shift] [capture=<png>]
// Coordinates are in points relative to the top-left of <pid>'s main window.
// Presses at (x1,y1), moves in 12 steps to (x2,y2), optionally captures the
// window while the button is still held, then releases.
import CoreGraphics
import Foundation
let a = CommandLine.arguments
let pid = Int(a[1])!
let x1 = Double(a[2])!, y1 = Double(a[3])!, x2 = Double(a[4])!, y2 = Double(a[5])!
let shift = a.contains("shift")
let capture = a.first { $0.hasPrefix("capture=") }.map { String($0.dropFirst(8)) }
let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]
guard let w = list.first(where: { ($0[kCGWindowOwnerPID as String] as? Int) == pid && ($0[kCGWindowLayer as String] as? Int) == 0 }) else { print("no window"); exit(1) }
let b = w[kCGWindowBounds as String] as! [String: Double]
let wid = w[kCGWindowNumber as String] as! Int
let ox = b["X"]!, oy = b["Y"]!
let flags: CGEventFlags = shift ? .maskShift : []
func post(_ t: CGEventType, _ x: Double, _ y: Double) {
  let e = CGEvent(mouseEventSource: nil, mouseType: t, mouseCursorPosition: CGPoint(x: ox + x, y: oy + y), mouseButton: .left)!
  e.flags = flags
  e.post(tap: .cghidEventTap)
}
if shift { let k = CGEvent(keyboardEventSource: nil, virtualKey: 56, keyDown: true)!; k.flags = .maskShift; k.post(tap: .cghidEventTap) }
post(.mouseMoved, x1, y1); usleep(80_000)
post(.leftMouseDown, x1, y1); usleep(80_000)
for i in 1...12 { let t = Double(i) / 12; post(.leftMouseDragged, x1 + (x2 - x1) * t, y1 + (y2 - y1) * t); usleep(40_000) }
usleep(300_000)
if let c = capture {
  let p = Process(); p.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture"); p.arguments = ["-x", "-o", "-l", String(wid), c]
  try! p.run(); p.waitUntilExit()
}
post(.leftMouseUp, x2, y2)
if shift { let k = CGEvent(keyboardEventSource: nil, virtualKey: 56, keyDown: false)!; k.post(tap: .cghidEventTap) }
print("ok window \(wid) origin \(ox),\(oy)")
