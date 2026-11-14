import AppKit
import SwiftUI

@MainActor
public class StatusBarController: ObservableObject {
    public static let shared = StatusBarController()

    private var statusItem: NSStatusItem!
    public var popover: NSPopover!
    private var eventMonitor: Any?

    private init() {}

    public func setup() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)

        popover = NSPopover()
        popover.contentSize = NSSize(width: 420, height: 490)
        popover.behavior = .transient
        popover.animates = true
        popover.contentViewController = NSHostingController(rootView: MenuBarPopoverView())

        if let button = statusItem.button {
            button.image = NSImage(systemSymbolName: "eyes", accessibilityDescription: "Agent Watch")
            button.action = #selector(togglePopover)
            button.target = self
        }

        updateStatusItem()
    }

    @objc public func togglePopover() {
        if popover.isShown {
            hidePopover()
        } else {
            showPopover()
        }
    }

    public func showPopover() {
        guard let button = statusItem.button else { return }

        // Activate app so the popover takes immediate focus
        NSApp.activate(ignoringOtherApps: true)
        popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)

        // Setup global outside click monitor to auto-close
        if eventMonitor == nil {
            eventMonitor = NSEvent.addGlobalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) { [weak self] _ in
                self?.hidePopover()
            }
        }
    }

    public func hidePopover() {
        popover.performClose(nil)
        if let monitor = eventMonitor {
            NSEvent.removeMonitor(monitor)
            eventMonitor = nil
        }
    }

    public func updateStatusItem() {
        guard let button = statusItem.button else { return }
        let client = DaemonClient.shared

        if !client.waitingSessions.isEmpty {
            button.image = NSImage(systemSymbolName: "exclamationmark.bubble.fill", accessibilityDescription: "Needs Input")
            button.title = " \(client.waitingSessions.count)"
        } else if !client.activeSessions.isEmpty {
            button.image = NSImage(systemSymbolName: "eyes.inverse", accessibilityDescription: "Active")
            button.title = ""
        } else {
            button.image = NSImage(systemSymbolName: "eyes", accessibilityDescription: "Idle")
            button.title = ""
        }
    }
}
