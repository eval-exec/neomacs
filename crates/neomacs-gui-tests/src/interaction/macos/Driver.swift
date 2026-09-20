// Native desktop helper. Rust owns contracts; this process owns macOS permissions.
import AppKit
import ApplicationServices
import ScreenCaptureKit
import Darwin

struct DriverFailure: Error { let message: String }
func fail(_ message: String) -> DriverFailure { DriverFailure(message: message) }
func rect(_ r: CGRect) -> [String: Double] {
    ["x": r.minX, "y": r.minY, "width": r.width, "height": r.height]
}

@MainActor
final class Desktop {
    var pid: pid_t?
    var heldKeys = Set<CGKeyCode>()
    var heldButtons = Set<Int>()
    var generations: [CGWindowID: UInt64] = [:]
    var generation: UInt64 = 0
    let source = CGEventSource(stateID: .hidSystemState)
    let keyCodes: [String: CGKeyCode] = ["escape":53, "enter":36, "down":125,
        "up":126, "left":123, "right":124, "home":115, "end":119, "x":7]

    func release() {
        for key in heldKeys { CGEvent(keyboardEventSource: source, virtualKey: key, keyDown: false)?.post(tap: .cghidEventTap) }
        heldKeys.removeAll()
        let point = CGEvent(source: nil)?.location ?? .zero
        for button in heldButtons {
            CGEvent(mouseEventSource: source, mouseType: button == 0 ? .leftMouseUp : .rightMouseUp,
                mouseCursorPosition: point, mouseButton: button == 0 ? .left : .right)?.post(tap: .cghidEventTap)
        }
        heldButtons.removeAll()
        pid = nil
        generations.removeAll()
    }
    func preflight() -> [String: Bool] {
        ["accessibility": AXIsProcessTrusted(), "post_events": CGPreflightPostEventAccess(),
         "screen_capture": CGPreflightScreenCaptureAccess(), "desktop": !NSScreen.screens.isEmpty]
    }
    func target() throws -> NSRunningApplication {
        guard let pid, let app = NSRunningApplication(processIdentifier: pid), !app.isTerminated else { throw fail("target process exited or is not attached") }
        return app
    }
    func content() async throws -> (SCShareableContent, [SCWindow], SCDisplay) {
        _ = try target()
        let content = try await SCShareableContent.excludingDesktopWindows(true, onScreenWindowsOnly: true)
        let windows = content.windows.filter { $0.owningApplication?.processID == pid && $0.isOnScreen }
        guard let root = windows.first(where: { $0.title == "NEOMACS-MENU-REPRO" }) else { throw fail("target editor window is not visible") }
        guard let display = content.displays.max(by: {
            let a = $0.frame.intersection(root.frame); let b = $1.frame.intersection(root.frame)
            return (a.isNull ? 0 : a.width*a.height) < (b.isNull ? 0 : b.width*b.height)
        }) else { throw fail("no capture display") }
        return (content, windows, display)
    }
    func observe() async throws -> [String: Any] {
        let (_, windows, display) = try await content()
        let active = Set(windows.map(\.windowID))
        generations = generations.filter { active.contains($0.key) }
        for window in windows where generations[window.windowID] == nil {
            generation += 1; generations[window.windowID] = generation
        }
        let screen = NSScreen.screens.first { ($0.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)?.uint32Value == display.displayID }
        // AppKit has a bottom-left origin; Quartz uses top-left global points.
        let primaryHeight = NSScreen.screens.first?.frame.height ?? display.frame.height
        let visible = screen?.visibleFrame ?? .zero
        let usable = CGRect(x: visible.minX, y: primaryHeight-visible.maxY, width: visible.width, height: visible.height)
        return ["display":rect(display.frame), "usable":rect(usable), "windows": windows.map { window in
            ["id":["native":UInt64(window.windowID), "generation":generations[window.windowID]!],
             "title":window.title ?? "", "bounds":rect(window.frame)] as [String: Any]
        }]
    }
    func input(_ action: [String: Any]) throws {
        let app = try target()
        guard app.isActive else { throw fail("target lost desktop focus") }
        guard let kind = action["kind"] as? String else { throw fail("missing input kind") }
        switch kind {
        case "move":
            guard let p = action["point"] as? [String: Double], let x=p["x"], let y=p["y"], x.isFinite, y.isFinite else { throw fail("invalid point") }
            let type: CGEventType = heldButtons.contains(0) ? .leftMouseDragged : heldButtons.contains(1) ? .rightMouseDragged : .mouseMoved
            CGEvent(mouseEventSource: source, mouseType: type, mouseCursorPosition: CGPoint(x:x,y:y), mouseButton: .left)?.post(tap: .cghidEventTap)
        case "button":
            guard let button = action["button"] as? String, ["left","right"].contains(button), let down = action["down"] as? Bool else { throw fail("invalid button") }
            let right = button == "right", index = right ? 1 : 0
            if down { heldButtons.insert(index) } else { heldButtons.remove(index) }
            let type: CGEventType = right ? (down ? .rightMouseDown : .rightMouseUp) : (down ? .leftMouseDown : .leftMouseUp)
            let event = CGEvent(mouseEventSource: source, mouseType: type,
                mouseCursorPosition: CGEvent(source:nil)?.location ?? .zero, mouseButton: right ? .right : .left)
            event?.setIntegerValueField(.mouseEventClickState, value: 1)
            event?.post(tap: .cghidEventTap)
        case "key":
            guard let name = action["key"] as? String, let code=keyCodes[name], let down=action["down"] as? Bool else { throw fail("invalid key") }
            if down { heldKeys.insert(code) } else { heldKeys.remove(code) }
            CGEvent(keyboardEventSource: source, virtualKey: code, keyDown: down)?.post(tap: .cghidEventTap)
        case "scroll":
            guard let lines = action["lines"] as? Int32, abs(Int64(lines)) <= 100 else { throw fail("invalid scroll count") }
            CGEvent(scrollWheelEvent2Source: source, units: .line, wheelCount: 1, wheel1: lines, wheel2: 0, wheel3: 0)?.post(tap: .cghidEventTap)
        default: throw fail("unknown input action")
        }
    }
    func handle(_ request: [String: Any]) async throws -> Any {
        switch request["op"] as? String {
        case "preflight": return preflight()
        case "attach":
            guard preflight().values.allSatisfy({ $0 }), let requested = request["pid"] as? Int32 else { throw fail("desktop permissions or pid missing") }
            pid = requested
            let app = try target()
            guard app.executableURL?.lastPathComponent == "neomacs" else { throw fail("target is not Neomacs") }
            app.activate(options: [])
            for _ in 0..<50 {
                if app.isActive { return true }
                try await Task.sleep(nanoseconds: 20_000_000)
            }
            throw fail("could not activate editor")
        case "fit":
            let app = try target()
            let observation = try await observe()
            guard let usable = observation["usable"] as? [String: Double] else { throw fail("missing usable display") }
            let element = AXUIElementCreateApplication(app.processIdentifier)
            var value: CFTypeRef?
            guard AXUIElementCopyAttributeValue(element, kAXWindowsAttribute as CFString, &value) == .success,
                  let windows = value as? [AXUIElement] else { throw fail("AX window discovery failed") }
            let window = windows.first { window in
                var title: CFTypeRef?
                return AXUIElementCopyAttributeValue(window,kAXTitleAttribute as CFString,&title) == .success
                    && title as? String == "NEOMACS-MENU-REPRO"
            }
            guard let window else { throw fail("AX editor window missing") }
            var point = CGPoint(x:usable["x"]!,y:usable["y"]!)
            var size = CGSize(width:usable["width"]!,height:usable["height"]!)
            guard let position = AXValueCreate(.cgPoint,&point), let extent = AXValueCreate(.cgSize,&size),
                  AXUIElementSetAttributeValue(window,kAXPositionAttribute as CFString,position) == .success,
                  AXUIElementSetAttributeValue(window,kAXSizeAttribute as CFString,extent) == .success else { throw fail("AX window placement failed") }
            var prior: CGRect?
            for _ in 0..<100 {
                try await Task.sleep(nanoseconds:50_000_000)
                let (_, windows, _) = try await content()
                if let frame=windows.first(where:{$0.title == "NEOMACS-MENU-REPRO"})?.frame {
                    if frame == prior { return rect(frame) }
                    prior = frame
                }
            }
            throw fail("window did not settle after fitting")
        case "observe": return try await observe()
        case "input":
            guard let action = request["action"] as? [String: Any] else { throw fail("missing action") }
            try input(action)
            // Allow the event to reach the application before the next request.
            try await Task.sleep(nanoseconds: 30_000_000)
            return true
        case "capture":
            guard let path=request["path"] as? String, path.hasPrefix("/") else { throw fail("capture path must be absolute") }
            let (_, _, display) = try await content()
            let filter = SCContentFilter(display: display, excludingWindows: [])
            let config = SCStreamConfiguration()
            let scale = filter.pointPixelScale
            config.width = Int((display.frame.width * Double(scale)).rounded())
            config.height = Int((display.frame.height * Double(scale)).rounded())
            config.showsCursor = false
            let image = try await SCScreenshotManager.captureImage(contentFilter: filter, configuration: config)
            guard let png = NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:]) else { throw fail("PNG encoding failed") }
            try png.write(to: URL(fileURLWithPath: path), options: .atomic)
            return ["bounds":rect(display.frame), "width":image.width, "height":image.height]
        default: throw fail("unknown operation")
        }
    }
}

// One connection owns the desktop session. Disconnect always releases input.
// Socket permissions limit control to the user who launched this helper.
func makeListener(_ path: String) throws -> Int32 {
    var address = sockaddr_un()
    address.sun_family = sa_family_t(AF_UNIX)
    let bytes = Array(path.utf8CString)
    guard bytes.count <= MemoryLayout.size(ofValue: address.sun_path) else { throw fail("socket path too long") }
    withUnsafeMutableBytes(of: &address.sun_path) { dest in
        bytes.withUnsafeBytes { src in dest.copyBytes(from: src) }
    }
    let fd = socket(AF_UNIX, SOCK_STREAM, 0)
    guard fd >= 0 else { throw fail("socket: \(errno)") }
    let status = withUnsafePointer(to: &address) {
        $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { Darwin.bind(fd, $0, socklen_t(MemoryLayout<sockaddr_un>.size)) }
    }
    guard status == 0 else { close(fd); throw fail("bind: \(errno); remove a stale socket only after checking its owner") }
    guard chmod(path, 0o600) == 0, listen(fd, 1) == 0 else { close(fd); throw fail("listen: \(errno)") }
    return fd
}
func readRequest(_ fd: Int32) throws -> Data? {
    var data = Data(), byte: UInt8 = 0
    while data.count < 65536 {
        let count = Darwin.read(fd, &byte, 1)
        if count == 0 { return nil }
        if count < 0 { if errno == EINTR { continue }; throw fail("read: \(errno)") }
        if byte == 10 { return data }
        data.append(byte)
    }
    throw fail("request too large")
}
func writeResponse(_ fd: Int32, _ response: [String: Any]) throws {
    var data = try JSONSerialization.data(withJSONObject: response)
    data.append(10)
    try data.withUnsafeBytes { raw in
        var offset = 0
        while offset < raw.count {
            let n = Darwin.write(fd, raw.baseAddress!.advanced(by: offset), raw.count-offset)
            if n <= 0 { throw fail("write: \(errno)") }; offset += n
        }
    }
}

@main
struct Main {
    @MainActor static func main() async throws {
        let args = CommandLine.arguments
        guard args.count == 2 || (args.count == 3 && args[2] == "--request-permissions") else {
            throw fail("usage: NeomacsGuiDriver SOCKET [--request-permissions]")
        }
        signal(SIGPIPE, SIG_IGN)
        umask(0o077)
        let path = CommandLine.arguments[1]
        let listener = try makeListener(path)
        defer { close(listener); unlink(path) }
        _ = NSApplication.shared
        NSApp.setActivationPolicy(.accessory)
        NSApp.finishLaunching()
        // Explicit setup only. Ordinary test runs never display TCC prompts.
        if args.count == 3 {
            let options = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
            _ = AXIsProcessTrustedWithOptions(options)
            if !CGPreflightScreenCaptureAccess() { _ = CGRequestScreenCaptureAccess() }
        }
        let desktop = Desktop()
        while true {
            let fd = await Task.detached { accept(listener, nil, nil) }.value
            if fd < 0 { continue }
            do {
                while let data = try await Task.detached(operation: { try readRequest(fd) }).value {
                    do {
                        guard let request = try JSONSerialization.jsonObject(with: data) as? [String: Any] else { throw fail("invalid request") }
                        let result = try await desktop.handle(request)
                        try writeResponse(fd, ["result":result])
                    } catch { try writeResponse(fd, ["error":String(describing:error)]) }
                }
            } catch { fputs("driver session: \(error)\n", stderr) }
            desktop.release()
            close(fd)
        }
    }
}
