import AppKit
import CoreGraphics
import Foundation
let pid = 82652
let wid = 7762
let windows = CGWindowListCopyWindowInfo([.optionIncludingWindow], CGWindowID(wid)) as! [[String: Any]]
guard let w = windows.first(where: { ($0[kCGWindowNumber as String] as? Int) == wid }), (w[kCGWindowOwnerPID as String] as? Int) == pid else { fatalError("Window does not belong to verification PID") }
print("window_id=\(wid) owner_pid=\(pid) bounds=\(w[kCGWindowBounds as String]!)")
print("frontmost_pid=\(NSWorkspace.shared.frontmostApplication?.processIdentifier ?? -1)")
