import AppKit
import SwiftUI

/// Borderless panel used for the menu bar dropdown. It needs to become key so
/// text fields (prompt replies) receive keyboard input.
final class GlassDropdownPanel: NSPanel {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    // Borderless windows can't close themselves, so the File > Close Window
    // (Cmd+W) menu item would just beep. Hide the dropdown instead.
    override func performClose(_ sender: Any?) {
        StatusBarController.shared.hidePopover()
    }

    override func validateMenuItem(_ menuItem: NSMenuItem) -> Bool {
        if menuItem.action == #selector(performClose(_:)) { return true }
        return super.validateMenuItem(menuItem)
    }
}

/// Hosting view that reports SwiftUI size changes so the panel can follow them.
final class SizeReportingHostingView<Content: View>: NSHostingView<Content> {
    var onIntrinsicSizeChange: (() -> Void)?

    override func invalidateIntrinsicContentSize() {
        super.invalidateIntrinsicContentSize()
        onIntrinsicSizeChange?()
    }
}

@MainActor
public class StatusBarController: NSObject, ObservableObject {
    public static let shared = StatusBarController()

    static let cornerRadius: CGFloat = 18
    static let gapBelowMenuBar: CGFloat = 6

    private var statusItem: NSStatusItem?
    private var panel: GlassDropdownPanel?
    private var hostingView: SizeReportingHostingView<MenuBarPopoverView>?
    private var globalClickMonitor: Any?
    private var localKeyMonitor: Any?

    public var isDropdownShown: Bool { panel?.isVisible ?? false }

    private override init() {
        super.init()
    }

    public func setup() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)

        if let button = statusItem?.button {
            button.image = NSImage(systemSymbolName: "eyes", accessibilityDescription: "Agent Watch")
            button.action = #selector(togglePopover)
            button.target = self
        }

        panel = makePanel()
        updateStatusItem()
    }

    // MARK: - Panel construction

    private func makePanel() -> GlassDropdownPanel {
        let hosting = SizeReportingHostingView(rootView: MenuBarPopoverView())
        hosting.translatesAutoresizingMaskIntoConstraints = false
        hosting.onIntrinsicSizeChange = { [weak self] in
            DispatchQueue.main.async { self?.resizePanelToFit() }
        }
        hostingView = hosting

        let panel = GlassDropdownPanel(
            contentRect: NSRect(x: 0, y: 0, width: 400, height: 300),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: true
        )
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.level = .popUpMenu
        panel.isReleasedWhenClosed = false
        panel.hidesOnDeactivate = false
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .transient]
        panel.animationBehavior = .utilityWindow

        panel.contentView = makeGlassContainer(hosting: hosting)
        return panel
    }

    /// System Liquid Glass on macOS 26+ (follows System Settings > Appearance >
    /// Liquid Glass and Accessibility > Reduce Transparency); popover material
    /// on older systems.
    private func makeGlassContainer(hosting: NSView) -> NSView {
        let radius = Self.cornerRadius

        if #available(macOS 26.0, *) {
            let glass = NSGlassEffectView()
            glass.style = .regular
            glass.cornerRadius = radius
            glass.contentView = hosting
            return glass
        }

        let effect = NSVisualEffectView()
        effect.material = .popover
        effect.blendingMode = .behindWindow
        effect.state = .active
        effect.wantsLayer = true
        effect.layer?.cornerRadius = radius
        effect.layer?.masksToBounds = true
        effect.addSubview(hosting)
        NSLayoutConstraint.activate([
            hosting.leadingAnchor.constraint(equalTo: effect.leadingAnchor),
            hosting.trailingAnchor.constraint(equalTo: effect.trailingAnchor),
            hosting.topAnchor.constraint(equalTo: effect.topAnchor),
            hosting.bottomAnchor.constraint(equalTo: effect.bottomAnchor),
        ])
        return effect
    }

    // MARK: - Positioning

    private func buttonScreenFrame() -> NSRect? {
        guard let button = statusItem?.button, let window = button.window else { return nil }
        return window.convertToScreen(button.convert(button.bounds, to: nil))
    }

    /// Size the panel to the SwiftUI content, anchored under the status item.
    private func resizePanelToFit() {
        guard let panel = panel, let hosting = hostingView,
              let anchor = buttonScreenFrame() else { return }

        let size = hosting.fittingSize
        guard size.width > 0, size.height > 0 else { return }

        let screen = statusItem?.button?.window?.screen ?? NSScreen.main
        let visible = screen?.visibleFrame ?? .zero
        let margin: CGFloat = 8

        var x = anchor.midX - size.width / 2
        x = min(max(x, visible.minX + margin), visible.maxX - size.width - margin)
        let top = anchor.minY - Self.gapBelowMenuBar
        let height = min(size.height, top - visible.minY - margin)

        panel.setFrame(NSRect(x: x, y: top - height, width: size.width, height: height), display: true)
    }

    // MARK: - Show / hide

    @objc public func togglePopover() {
        if isDropdownShown {
            hidePopover()
        } else {
            showPopover()
        }
    }

    public func showPopover() {
        guard let panel = panel else { return }

        resizePanelToFit()
        NSApp.activate(ignoringOtherApps: true)
        panel.makeKeyAndOrderFront(nil)
        statusItem?.button?.highlight(true)
        installMonitors()
    }

    public func hidePopover() {
        panel?.orderOut(nil)
        statusItem?.button?.highlight(false)
        removeMonitors()
    }

    private func installMonitors() {
        if globalClickMonitor == nil {
            // Clicks in other apps close the dropdown. Clicks on our own status
            // item go through togglePopover instead.
            globalClickMonitor = NSEvent.addGlobalMonitorForEvents(
                matching: [.leftMouseDown, .rightMouseDown]
            ) { [weak self] _ in
                Task { @MainActor in
                    guard let self = self, self.isDropdownShown else { return }
                    let mouse = NSEvent.mouseLocation
                    if let frame = self.buttonScreenFrame(), frame.contains(mouse) { return }
                    if let panel = self.panel, panel.frame.contains(mouse) { return }
                    self.hidePopover()
                }
            }
        }

        if localKeyMonitor == nil {
            localKeyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
                // Cmd+W is handled by the main menu via GlassDropdownPanel.performClose.
                guard let self = self, self.isDropdownShown,
                      event.window === self.panel, event.keyCode == 53 /* esc */ else { return event }
                self.hidePopover()
                return nil
            }
        }
    }

    private func removeMonitors() {
        if let monitor = globalClickMonitor {
            NSEvent.removeMonitor(monitor)
            globalClickMonitor = nil
        }
        if let monitor = localKeyMonitor {
            NSEvent.removeMonitor(monitor)
            localKeyMonitor = nil
        }
    }

    // MARK: - Status item

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
