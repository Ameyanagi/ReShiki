import Cocoa
import CoreGraphics

let args = CommandLine.arguments
if args.count != 5 { fputs("usage: cmd-scroll.swift PID windowX windowY pixelDelta\n", stderr); exit(2) }
guard let pid = Int32(args[1]), pid == 74366,
      let x = Double(args[2]), let y = Double(args[3]), let delta = Int32(args[4]),
      NSWorkspace.shared.frontmostApplication?.processIdentifier == pid,
      NSWorkspace.shared.frontmostApplication?.bundleIdentifier == "dev.reshiki.verification.ev3862p2" else {
    fputs("Refusing: expected verification PID is not frontmost\n", stderr); exit(3)
}
let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String:Any]] ?? []
guard let window = windows.first(where: { ($0[kCGWindowNumber as String] as? Int) == 7185 && ($0[kCGWindowOwnerPID as String] as? Int) == Int(pid) }),
      let b = window[kCGWindowBounds as String] as? [String:Double],
      let bx = b["X"], let by = b["Y"], let width = b["Width"], let height = b["Height"],
      x >= 0, y >= 0, x < width, y < height else { exit(4) }
let point = CGPoint(x: bx+x, y: by+y)
if let top = windows.first(where: { w in
    if (w[kCGWindowName as String] as? String) == "Cursor" { return false }
    guard let d = w[kCGWindowBounds as String] as? [String:Double], let wx = d["X"], let wy = d["Y"], let ww = d["Width"], let wh = d["Height"] else { return false }
    return CGRect(x:wx,y:wy,width:ww,height:wh).contains(point)
}), (top[kCGWindowOwnerPID as String] as? Int) != Int(pid) {
    fputs("Refusing: scroll point is obstructed by another PID\n", stderr); exit(5)
}
let source = CGEventSource(stateID: .hidSystemState)
CGWarpMouseCursorPosition(point)
let move = CGEvent(mouseEventSource: source, mouseType: .mouseMoved, mouseCursorPosition: point, mouseButton: .left)!
move.post(tap: .cghidEventTap)
let down = CGEvent(keyboardEventSource: source, virtualKey: 55, keyDown: true)!
down.flags = .maskCommand
down.post(tap: .cghidEventTap)
usleep(100000)
let scroll = CGEvent(scrollWheelEvent2Source: source, units: .pixel, wheelCount: 1, wheel1: delta, wheel2: 0, wheel3: 0)!
scroll.flags = .maskCommand
scroll.location = point
scroll.post(tap: .cghidEventTap)
usleep(100000)
let up = CGEvent(keyboardEventSource: source, virtualKey: 55, keyDown: false)!
up.flags = []
up.post(tap: .cghidEventTap)
print("Cmd+scroll PID=\(pid) WID=7185 relative=(\(x),\(y)) pixelDelta=\(delta)")
