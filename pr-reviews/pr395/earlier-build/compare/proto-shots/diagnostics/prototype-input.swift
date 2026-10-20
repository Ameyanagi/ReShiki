import AppKit
import CoreGraphics
import ApplicationServices
let pid: pid_t = 70420
let wid: CGWindowID = 7128
let rows = CGWindowListCopyWindowInfo([.optionIncludingWindow], wid) as! [[String: Any]]
guard let row = rows.first, (row[kCGWindowOwnerPID as String] as? Int) == Int(pid), let bounds = row[kCGWindowBounds as String] as? [String: CGFloat] else { fatalError("Target window no longer belongs to expected PID") }
let origin = CGPoint(x: bounds["X"]!, y: bounds["Y"]!)
func point(_ x: Double,_ y: Double) -> CGPoint { CGPoint(x: origin.x+x, y: origin.y+y) }
func post(_ event: CGEvent?) {
    guard NSWorkspace.shared.frontmostApplication?.processIdentifier == pid else { fatalError("Expected Chrome PID is not frontmost; refusing input") }
    event!.setIntegerValueField(.mouseEventClickState,value:1)
    if event!.type == .leftMouseDown || event!.type == .leftMouseUp || event!.type == .mouseMoved { event!.flags = [] }
    event!.post(tap: .cghidEventTap); usleep(150000)
}
let args = CommandLine.arguments
switch args[1] {
case "info": print("PID=\(pid) WID=\(wid) bounds=\(bounds) frontmost=\(NSWorkspace.shared.frontmostApplication?.processIdentifier ?? 0)")
case "focus":
    guard let app=NSRunningApplication(processIdentifier:pid) else { fatalError("Target process exited") }
    app.activate(options:[.activateIgnoringOtherApps]); usleep(250000)
case "click":
    let p = point(Double(args[2])!, Double(args[3])!)
    post(CGEvent(mouseEventSource:nil,mouseType:.mouseMoved,mouseCursorPosition:p,mouseButton:.left))
    post(CGEvent(mouseEventSource:nil,mouseType:.leftMouseDown,mouseCursorPosition:p,mouseButton:.left))
    post(CGEvent(mouseEventSource:nil,mouseType:.leftMouseUp,mouseCursorPosition:p,mouseButton:.left))
case "park":
    let p = point(1445, 70)
    CGWarpMouseCursorPosition(p)
    post(CGEvent(mouseEventSource:nil,mouseType:.mouseMoved,mouseCursorPosition:p,mouseButton:.left))
case "key":
    let code = CGKeyCode(UInt16(args[2])!)
    let flags: CGEventFlags = args.count > 3 && args[3] == "cmd" ? .maskCommand : []
    let down = CGEvent(keyboardEventSource:nil,virtualKey:code,keyDown:true)!
    let up = CGEvent(keyboardEventSource:nil,virtualKey:code,keyDown:false)!
    down.flags = flags; up.flags = flags
    post(down); post(up)
case "scroll":
    let p = point(1425, 800)
    let event = CGEvent(scrollWheelEvent2Source:nil,units:.pixel,wheelCount:1,wheel1:Int32(args[2])!,wheel2:0,wheel3:0)!
    event.location = p
    post(event)
default: fatalError("Unknown command")
}
