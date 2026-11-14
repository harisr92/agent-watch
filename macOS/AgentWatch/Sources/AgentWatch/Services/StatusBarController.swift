import AppKit
import SwiftUI

@MainActor
public class StatusBarController: NSObject, ObservableObject, NSPopoverDelegate {
    public static let shared = StatusBarController()

    private var statusItem: NSStatusItem?
    public var popover: NSPopover?
    private var eventMonitor: Any?

    private override init() {
        super.init()
    }

    public func setup() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)

        let pop = NSPopover()
        pop.contentSize = NSSize(width: 420, height: 490)
        pop.behavior = .semitransient
        pop.animates = true
        pop.delegate = self
        pop.contentViewController = NSHostingController(rootView: MenuBarPopoverView())
        popover = pop

        if let button = statusItem?.button {
            button.image = NSImage(systemSymbolName: "eyes", accessibilityDescription: "Agent Watch")
            button.action = #selector(togglePopover)
            button.target = self
        }

        updateStatusItem()
    }

    @objc public func togglePopover() {
        guard let popover = popover else { return }
        if popover.isShown {
            hidePopover()
        } else {
            showPopover()
        }
    }

    public func showPopover() {
        guard let popover = popover, let button = statusItem?.button else { return }

        // Activate app so the popover takes immediate focus
        NSApp.activate(ignoringOtherApps: true)
        popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
        popover.contentViewController?.view.window?.makeKey()
        DispatchQueue.main.async {
            popover.contentViewController?.view.window?.makeKey()
        }

        // Setup global outside click monitor to auto-close
        if eventMonitor == nil {
            eventMonitor = NSEvent.addGlobalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) { [weak self] _ in
                guard let self = self, let popover = self.popover, popover.isShown else { return }

                // Don't close if click is on the status item button itself
                if let button = self.statusItem?.button, let window = button.window {
                    let mouseLoc = NSEvent.mouseLocation
                    let buttonFrame = window.convertToScreen(button.convert(button.bounds, to: nil))
                    if buttonFrame.contains(mouseLoc) {
                        return
                    }
                }

                // Don't close if click is inside the popover window
                if let popWindow = popover.contentViewController?.view.window {
                    let mouseLoc = NSEvent.mouseLocation
                    if popWindow.frame.contains(mouseLoc) {
                        return
                    }
                }

                self.hidePopover()
            }
        }
    }

    public func hidePopover() {
        popover?.performClose(nil)
        if let monitor = eventMonitor {
            NSEvent.removeMonitor(monitor)
            eventMonitor = nil
        }
    }

    public func popoverDidClose(_ notification: Notification) {
        if let monitor = eventMonitor {
            NSEvent.removeMonitor(monitor)
            eventMonitor = nil
        }
    }

    public func updateStatusItem() {
        guard let button = statusItem?.button else { return }
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
