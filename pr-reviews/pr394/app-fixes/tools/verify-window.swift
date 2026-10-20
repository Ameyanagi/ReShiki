import AppKit
import CoreGraphics
let pid = Int(CommandLine.arguments[1])!
let expected = Int(CommandLine.arguments[2])!
let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]
guard let w = list.first(where: { ($0[kCGWindowNumber as String] as? Int) == expected && ($0[kCGWindowOwnerPID as String] as? Int) == pid }) else { fatalError("Expected PID/window missing") }
let b = w[kCGWindowBounds as String] as! [String: Double]
if CommandLine.arguments.contains("activate") {
 guard let target = NSRunningApplication(processIdentifier: pid_t(pid)), target.bundleIdentifier == "dev.reshiki.verification.ev386fix1b" else { fatalError("Unexpected target bundle") }
 target.activate(options: [.activateIgnoringOtherApps])
 for _ in 0..<20 {
  if NSWorkspace.shared.frontmostApplication?.processIdentifier == pid_t(pid) { break }
  RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.05))
 }
}
let front = NSWorkspace.shared.frontmostApplication!
print("pid=\(pid) window=\(expected) rect=\(b) frontmost_pid=\(front.processIdentifier) frontmost_bundle=\(front.bundleIdentifier ?? "unknown")")
if CommandLine.arguments.contains("park") {
 guard front.processIdentifier == pid else { fatalError("Target is not frontmost") }
 let p = CGPoint(x: b["X"]! + 960, y: b["Y"]! + 60)
 CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: p, mouseButton: .left)!.post(tap: .cghidEventTap)
 RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.2))
}
