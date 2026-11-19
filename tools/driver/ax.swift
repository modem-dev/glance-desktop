// Reads and presses an app's accessibility tree, so a check can find controls and read
// state through the same interface VoiceOver uses. Posts no input events.
//
//   ax tree APP                  one JSON object per element: role, title, description,
//                                value, frame in global points, depth
//   ax menus APP                 every menu bar item: "Menu > Item"
//   ax menu APP MENU ITEM        presses that menu item (AXPress)
//   ax dock-menu APP ITEM        opens APP's Dock menu (AXShowMenu) and presses ITEM in it
import AppKit
import ApplicationServices

setvbuf(stdout, nil, _IOLBF, 0)

func fail(_ reason: String) -> Never {
    FileHandle.standardError.write((reason + "\n").data(using: .utf8)!)
    exit(1)
}
func attribute(_ element: AXUIElement, _ name: String) -> AnyObject? {
    var value: AnyObject?
    return AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success
        ? value : nil
}
func string(_ element: AXUIElement, _ name: String) -> String? {
    switch attribute(element, name) {
    case let text as String: return text
    case let number as NSNumber: return number.stringValue
    default: return nil
    }
}
func children(_ element: AXUIElement) -> [AXUIElement] {
    attribute(element, kAXChildrenAttribute) as? [AXUIElement] ?? []
}
func frame(_ element: AXUIElement) -> [Double]? {
    guard let position = attribute(element, kAXPositionAttribute),
        let size = attribute(element, kAXSizeAttribute)
    else { return nil }
    var point = CGPoint.zero
    var extent = CGSize.zero
    AXValueGetValue(position as! AXValue, .cgPoint, &point)
    AXValueGetValue(size as! AXValue, .cgSize, &extent)
    return [point.x, point.y, extent.width, extent.height].map(Double.init)
}

let arguments = Array(CommandLine.arguments.dropFirst())
guard arguments.count >= 2 else { fail("usage: ax tree|menus|menu|dock-menu APP [MENU] [ITEM]") }
guard AXIsProcessTrusted() else { fail("refusing: this process has no Accessibility permission") }
guard
    let pid = NSWorkspace.shared.runningApplications.first(where: {
        $0.localizedName == arguments[1]
    })?.processIdentifier
else { fail("\(arguments[1]) is not running") }
let app = AXUIElementCreateApplication(pid)

func menuItems() -> [(String, String, AXUIElement)] {
    guard let bar = attribute(app, kAXMenuBarAttribute) else { fail("no menu bar") }
    var found: [(String, String, AXUIElement)] = []
    for menu in children(bar as! AXUIElement) {
        let menuTitle = string(menu, kAXTitleAttribute) ?? "?"
        for list in children(menu) {
            for item in children(list) {
                let title = string(item, kAXTitleAttribute) ?? ""
                if !title.isEmpty { found.append((menuTitle, title, item)) }
            }
        }
    }
    return found
}

switch arguments[0] {
case "tree":
    func walk(_ element: AXUIElement, _ depth: Int) {
        var row: [String: Any] = ["depth": depth]
        for (key, name) in [
            ("role", kAXRoleAttribute), ("title", kAXTitleAttribute),
            ("description", kAXDescriptionAttribute), ("value", kAXValueAttribute),
            ("identifier", kAXIdentifierAttribute),
        ] {
            if let value = string(element, name), !value.isEmpty { row[key] = value }
        }
        if let frame = frame(element) { row["frame"] = frame }
        let data = try! JSONSerialization.data(withJSONObject: row, options: [.sortedKeys])
        print(String(decoding: data, as: UTF8.self))
        for child in children(element) { walk(child, depth + 1) }
    }
    for window in attribute(app, kAXWindowsAttribute) as? [AXUIElement] ?? [] { walk(window, 0) }
case "menus":
    for (menu, item, _) in menuItems() { print("\(menu) > \(item)") }
case "menu":
    guard arguments.count == 4 else { fail("usage: ax menu APP MENU ITEM") }
    guard let (_, _, item) = menuItems().first(where: { $0.0 == arguments[2] && $0.1 == arguments[3] })
    else { fail("no menu item \(arguments[2]) > \(arguments[3])") }
    let result = AXUIElementPerformAction(item, kAXPressAction as CFString)
    guard result == .success else { fail("pressing \(arguments[3]) gave \(result.rawValue)") }
case "dock-menu":
    guard arguments.count == 3 else { fail("usage: ax dock-menu APP ITEM") }
    guard
        let dockPid = NSRunningApplication.runningApplications(
            withBundleIdentifier: "com.apple.dock"
        ).first?.processIdentifier
    else { fail("the Dock is not running") }
    let dock = AXUIElementCreateApplication(dockPid)
    guard
        let icon = children(dock).flatMap(children).first(where: {
            string($0, kAXTitleAttribute) == arguments[1]
        })
    else { fail("no Dock icon named \(arguments[1])") }
    let shown = AXUIElementPerformAction(icon, kAXShowMenuAction as CFString)
    guard shown == .success else { fail("showing the Dock menu gave \(shown.rawValue)") }
    // The menu is the icon's child once it shows; 50 × 0.1 s is far past any open seen.
    func find(_ element: AXUIElement) -> AXUIElement? {
        if string(element, kAXRoleAttribute) == kAXMenuItemRole,
            string(element, kAXTitleAttribute) == arguments[2]
        {
            return element
        }
        return children(element).lazy.compactMap(find).first
    }
    for _ in 0..<50 {
        if let item = find(icon) {
            let result = AXUIElementPerformAction(item, kAXPressAction as CFString)
            guard result == .success else { fail("pressing \(arguments[2]) gave \(result.rawValue)") }
            exit(0)
        }
        Thread.sleep(forTimeInterval: 0.1)
    }
    AXUIElementPerformAction(icon, kAXCancelAction as CFString)
    fail("no item \(arguments[2]) in \(arguments[1])'s Dock menu")
default:
    fail("unknown command \(arguments[0])")
}
