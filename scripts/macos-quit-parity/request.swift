import AppKit
import ApplicationServices
import Carbon
if CommandLine.arguments.count == 1 {
    let info: [String: Any] = ["post_events": CGPreflightPostEventAccess(),
                               "accessibility": AXIsProcessTrusted(),
                               "desktop": !NSScreen.screens.isEmpty]
    let data = try JSONSerialization.data(withJSONObject: info, options: [.sortedKeys])
    print(String(decoding: data, as: UTF8.self))
    exit(0)
}
let pid = pid_t(CommandLine.arguments[1])!
guard let app = NSRunningApplication(processIdentifier: pid) else { fatalError("no owned application") }
if CommandLine.arguments[2] == "terminate" {
    print("terminate-request-accepted=\(app.terminate())")
} else if CommandLine.arguments[2] == "apple-event" {
    // Address only the child created by probe.py. Never request automation
    // permission interactively or quit another application by bundle ID.
    let event = NSAppleEventDescriptor(
        eventClass: AEEventClass(kCoreEventClass),
        eventID: AEEventID(kAEQuitApplication),
        targetDescriptor: NSAppleEventDescriptor(processIdentifier: pid),
        returnID: AEReturnID(kAutoGenerateReturnID),
        transactionID: AETransactionID(kAnyTransactionID))
    do {
        let reply = try event.sendEvent(options: [.waitForReply, .neverInteract], timeout: 3)
        print("apple-event-quit-reply=\(reply)")
    } catch {
        print("BLOCKED: apple-event quit: \(error)")
        exit(43)
    }
} else {
    guard CGPreflightPostEventAccess() else { print("BLOCKED: post_events permission"); exit(42) }
    app.activate(options: [.activateIgnoringOtherApps])
    Thread.sleep(forTimeInterval: 0.2)
    for down in [true, false] {
        guard let event = CGEvent(keyboardEventSource: nil, virtualKey: 12, keyDown: down) else { fatalError("key event") }
        event.flags = .maskCommand
        event.postToPid(pid)
    }
    print("command-q-posted")
}
