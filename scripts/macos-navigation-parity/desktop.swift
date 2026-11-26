import AppKit
import ApplicationServices

if CommandLine.arguments.count == 1 {
    let screen = NSScreen.screens.first
    let info: [String: Any] = [
        "post_events": CGPreflightPostEventAccess(),
        "accessibility": AXIsProcessTrusted(),
        "desktop": screen != nil,
        "screen_width_points": screen?.frame.width ?? 0,
        "screen_height_points": screen?.frame.height ?? 0,
        "screen_scale": screen?.backingScaleFactor ?? 0,
        "screen_max_fps": screen?.maximumFramesPerSecond ?? 0,
    ]
    let data = try JSONSerialization.data(withJSONObject: info, options: [.sortedKeys])
    print(String(decoding: data, as: UTF8.self))
} else {
    let pid = pid_t(CommandLine.arguments[1])!
    guard let app = NSRunningApplication(processIdentifier: pid), !app.isTerminated else {
        fatalError("owned editor process unavailable")
    }
    app.activate(options: [.activateIgnoringOtherApps])
    if CommandLine.arguments.count > 2 && CommandLine.arguments[2] == "repeat" {
        guard CGPreflightPostEventAccess() else {
            print("BLOCKED: post_events permission")
            exit(42)
        }
        let start = ProcessInfo.processInfo.systemUptime
        for index in 0..<260 {
            let delay = start + Double(index) * 0.1 - ProcessInfo.processInfo.systemUptime
            if delay > 0 { Thread.sleep(forTimeInterval: delay) }
            for down in [true, false] {
                guard let event = CGEvent(keyboardEventSource: nil, virtualKey: 45, keyDown: down) else {
                    fatalError("C-n event allocation failed")
                }
                event.flags = .maskControl
                if down {
                    event.setIntegerValueField(.keyboardEventAutorepeat, value: index == 0 ? 0 : 1)
                }
                event.postToPid(pid)
            }
        }
        print("posted260 native C-n pairs to owned PID \(pid)")
    } else {
        print("activated owned PID \(pid)")
    }
}
