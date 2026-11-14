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

public struct InWindowNotification: Identifiable, Equatable {
    public let id: String
    public let sessionName: String
    public let promptHeadline: String
    public let promptText: String
    public let timestamp: Date

    public init(id: String, sessionName: String, promptHeadline: String, promptText: String, timestamp: Date = Date()) {
        self.id = id
        self.sessionName = sessionName
        self.promptHeadline = promptHeadline
        self.promptText = promptText
        self.timestamp = timestamp
    }

    public static func == (lhs: InWindowNotification, rhs: InWindowNotification) -> Bool {
        lhs.id == rhs.id
    }
}

@MainActor
public class WindowManager: NSObject, ObservableObject, NSWindowDelegate {
    public static let shared = WindowManager()

    public static let minWindowSize = NSSize(width: 480, height: 480)
    public static let defaultWindowSize = NSSize(width: 500, height: 640)

    @Published public private(set) var isWindowOpen: Bool = false
    @Published public private(set) var isWindowActiveState: Bool = false
    @Published public var activeNotification: InWindowNotification?
    @Published public var isAlwaysOnTop: Bool = false {
        didSet {
            updateWindowLevel()
        }
    }

    /// Evaluates live AppKit state to determine whether the detached window is currently key and active
    public var isWindowActive: Bool {
        guard isWindowOpen, let window = window else { return false }
        return window.isVisible && !window.isMiniaturized && window.isKeyWindow && NSApp.isActive
    }

    private var window: DetachedGlassWindow?
    private var notificationDismissTask: Task<Void, Never>?

    private override init() {
        super.init()

        NotificationCenter.default.addObserver(
            forName: NSApplication.didBecomeActiveNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor in
                self?.updateActiveState()
            }
        }

        NotificationCenter.default.addObserver(
            forName: NSApplication.didResignActiveNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor in
                self?.updateActiveState()
            }
        }
    }

    public func openDetachedWindow() {
        if let existing = window {
            if existing.isMiniaturized {
                existing.deminiaturize(nil)
            }
            existing.minSize = WindowManager.minWindowSize
            if existing.frame.width < WindowManager.minWindowSize.width || existing.frame.height < WindowManager.minWindowSize.height {
                var f = existing.frame
                f.size.width = max(f.size.width, WindowManager.minWindowSize.width)
                f.size.height = max(f.size.height, WindowManager.minWindowSize.height)
                existing.setFrame(f, display: true)
            }
            updateWindowLevel()
            existing.orderFrontRegardless()
            existing.makeKeyAndOrderFront(nil)
            NSRunningApplication.current.activate(options: [.activateIgnoringOtherApps, .activateAllWindows])
            NSApp.activate(ignoringOtherApps: true)
            isWindowOpen = true
            updateActiveState()
            StatusBarController.shared.hidePopover()
            return
        }

        let glassWindow = DetachedGlassWindow(
            contentRect: NSRect(origin: .zero, size: WindowManager.defaultWindowSize),
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
        glassWindow.minSize = WindowManager.minWindowSize
        glassWindow.isReleasedWhenClosed = false
        glassWindow.delegate = self
        glassWindow.tabbingMode = .disallowed
        glassWindow.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]

        glassWindow.setFrameAutosaveName("AgentWatchDetachedGlassWindow")

        // Strictly guarantee the window never opens smaller than minWindowSize
        var frame = glassWindow.frame
        if frame.size.width < WindowManager.minWindowSize.width || frame.size.height < WindowManager.minWindowSize.height {
            frame.size.width = max(frame.size.width, WindowManager.minWindowSize.width)
            frame.size.height = max(frame.size.height, WindowManager.minWindowSize.height)
            glassWindow.setFrame(frame, display: true)
        }

        if glassWindow.frame.origin.x == 0 && glassWindow.frame.origin.y == 0 {
            glassWindow.center()
        }

        let hostingController = NSHostingController(rootView: DetachedGlassWindowView())
        glassWindow.contentViewController = hostingController

        self.window = glassWindow
        updateWindowLevel()

        glassWindow.orderFrontRegardless()
        glassWindow.makeKeyAndOrderFront(nil)
        NSRunningApplication.current.activate(options: [.activateIgnoringOtherApps, .activateAllWindows])
        NSApp.activate(ignoringOtherApps: true)
        isWindowOpen = true
        updateActiveState()

        StatusBarController.shared.hidePopover()
    }

    public func closeDetachedWindow() {
        window?.orderOut(nil)
        isWindowOpen = false
        clearInWindowNotification()
        updateActiveState()
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

        if window.isMiniaturized {
            window.deminiaturize(nil)
        }

        updateWindowLevel()
        window.orderFrontRegardless()
        window.makeKeyAndOrderFront(nil)
        NSRunningApplication.current.activate(options: [.activateIgnoringOtherApps, .activateAllWindows])
        NSApp.activate(ignoringOtherApps: true)
        isWindowOpen = true
        updateActiveState()
    }

    public func toggleAlwaysOnTop() {
        isAlwaysOnTop.toggle()
    }

    public func showInWindowNotification(session: AgentSession, prompt: PendingPrompt) {
        notificationDismissTask?.cancel()

        withAnimation(.spring(response: 0.35, dampingFraction: 0.8)) {
            self.activeNotification = InWindowNotification(
                id: prompt.id,
                sessionName: session.name,
                promptHeadline: prompt.promptType.headline,
                promptText: prompt.promptText,
                timestamp: Date()
            )
        }

        notificationDismissTask = Task { @MainActor in
            try? await Task.sleep(nanoseconds: 6_000_000_000)
            if !Task.isCancelled {
                withAnimation(.easeInOut(duration: 0.25)) {
                    if self.activeNotification?.id == prompt.id {
                        self.activeNotification = nil
                    }
                }
            }
        }
    }

    public func clearInWindowNotification() {
        notificationDismissTask?.cancel()
        withAnimation(.easeInOut(duration: 0.2)) {
            self.activeNotification = nil
        }
    }

    private func updateWindowLevel() {
        window?.level = isAlwaysOnTop ? .floating : .normal
    }

    private func updateActiveState() {
        let active = isWindowActive
        if isWindowActiveState != active {
            isWindowActiveState = active
        }
    }

    // MARK: - NSWindowDelegate

    public func windowShouldClose(_ sender: NSWindow) -> Bool {
        closeDetachedWindow()
        return false
    }

    public func windowWillResize(_ sender: NSWindow, to frameSize: NSSize) -> NSSize {
        NSSize(
            width: max(frameSize.width, WindowManager.minWindowSize.width),
            height: max(frameSize.height, WindowManager.minWindowSize.height)
        )
    }

    public func windowWillClose(_ notification: Notification) {
        isWindowOpen = false
        updateActiveState()
    }

    public func windowDidBecomeKey(_ notification: Notification) {
        isWindowOpen = true
        updateActiveState()
    }

    public func windowDidResignKey(_ notification: Notification) {
        updateActiveState()
    }

    public func windowDidMiniaturize(_ notification: Notification) {
        updateActiveState()
    }

    public func windowDidDeminiaturize(_ notification: Notification) {
        updateActiveState()
    }
}
