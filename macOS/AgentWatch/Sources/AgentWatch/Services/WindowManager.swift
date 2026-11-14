import AppKit
import SwiftUI

public final class DetachedGlassWindow: NSWindow {
    public override var canBecomeKey: Bool {
        return true
    }

    public override var canBecomeMain: Bool {
        return true
    }
}

@MainActor
public class WindowManager: NSObject, ObservableObject, NSWindowDelegate {
    public static let shared = WindowManager()

    @Published public private(set) var isWindowOpen: Bool = false
    @Published public var isAlwaysOnTop: Bool = false {
        didSet {
            updateWindowLevel()
        }
    }

    private var window: DetachedGlassWindow?

    private override init() {
        super.init()
    }

    public func openDetachedWindow() {
        if let existing = window {
            updateWindowLevel()
            existing.makeKeyAndOrderFront(nil)
            NSApp.activate(ignoringOtherApps: true)
            isWindowOpen = true
            StatusBarController.shared.hidePopover()
            return
        }

        let glassWindow = DetachedGlassWindow(
            contentRect: NSRect(x: 0, y: 0, width: 460, height: 620),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )

        glassWindow.title = "Agent Watch"
        glassWindow.titleVisibility = .hidden
        glassWindow.titlebarAppearsTransparent = true
        glassWindow.isOpaque = false
        glassWindow.backgroundColor = .clear
        glassWindow.hasShadow = true
        glassWindow.isMovableByWindowBackground = true
        glassWindow.minSize = NSSize(width: 400, height: 460)
        glassWindow.isReleasedWhenClosed = false
        glassWindow.delegate = self
        glassWindow.tabbingMode = .disallowed
        glassWindow.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]

        glassWindow.setFrameAutosaveName("AgentWatchDetachedGlassWindow")

        if glassWindow.frame.origin.x == 0 && glassWindow.frame.origin.y == 0 {
            glassWindow.center()
        }

        let hostingController = NSHostingController(rootView: DetachedGlassWindowView())
        glassWindow.contentViewController = hostingController

        self.window = glassWindow
        updateWindowLevel()

        glassWindow.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        isWindowOpen = true

        StatusBarController.shared.hidePopover()
    }

    public func closeDetachedWindow() {
        window?.orderOut(nil)
        isWindowOpen = false
    }

    public func dockToMenuBar() {
        closeDetachedWindow()
        StatusBarController.shared.showPopover()
    }

    public func bringToFront() {
        guard let window = window else {
            openDetachedWindow()
            return
        }
        updateWindowLevel()
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        isWindowOpen = true
    }

    public func toggleAlwaysOnTop() {
        isAlwaysOnTop.toggle()
    }

    private func updateWindowLevel() {
        window?.level = isAlwaysOnTop ? .floating : .normal
    }

    // MARK: - NSWindowDelegate

    public func windowShouldClose(_ sender: NSWindow) -> Bool {
        closeDetachedWindow()
        return false
    }

    public func windowWillClose(_ notification: Notification) {
        isWindowOpen = false
    }
}
