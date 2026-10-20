import AppKit
import ApplicationServices
let pid: pid_t = 70420
let app = AXUIElementCreateApplication(pid)
func attr(_ e: AXUIElement,_ key: CFString) -> AnyObject? { var v: CFTypeRef?; AXUIElementCopyAttributeValue(e,key,&v); return v }
let command=CommandLine.arguments[1]
let query=CommandLine.arguments.count>2 ? CommandLine.arguments[2] : ""
var matches:[AXUIElement]=[]
var count=0
func walk(_ e: AXUIElement,_ depth: Int=0) {
    count += 1
    if count > 10000 || depth > 80 { return }
    let role=attr(e,kAXRoleAttribute as CFString) as? String ?? ""
    let title=attr(e,kAXTitleAttribute as CFString) as? String ?? ""
    let desc=attr(e,kAXDescriptionAttribute as CFString) as? String ?? ""
    let val=attr(e,kAXValueAttribute as CFString)
    if command == "dump" && (role == "AXButton" || role == "AXRadioButton" || role == "AXCheckBox" || role == "AXWindow") { print("\(role) title=[\(title)] description=[\(desc)] value=[\(val ?? "" as NSString)]") }
    if (role == "AXButton" || role == "AXCheckBox") && (title==query || desc==query) { matches.append(e) }
    if let children=attr(e,kAXChildrenAttribute as CFString) as? [AXUIElement] { for child in children { walk(child,depth+1) } }
}
walk(app)
if command=="press" {
    guard matches.count==1 else { fatalError("Expected exactly one button matching \(query); found \(matches.count)") }
    let result=AXUIElementPerformAction(matches[0],kAXPressAction as CFString)
    print("Pressed \(query): \(result.rawValue)")
    if result != .success { exit(1) }
}

if command=="attributes" { for e in matches { var names: CFArray?; AXUIElementCopyAttributeNames(e,&names); for name in (names as? [String] ?? []) { print("\(name)=\(attr(e,name as CFString) ?? "nil" as NSString)") } } }
