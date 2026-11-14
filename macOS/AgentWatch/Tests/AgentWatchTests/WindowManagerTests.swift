import XCTest
@testable import AgentWatch

@MainActor
final class WindowManagerTests: XCTestCase {
    func testWindowManagerLifecycle() {
        let manager = WindowManager.shared

        // Initial open
        manager.openDetachedWindow()
        XCTAssertTrue(manager.isWindowOpen)

        // Always on top toggle
        let initialTop = manager.isAlwaysOnTop
        manager.toggleAlwaysOnTop()
        XCTAssertEqual(manager.isAlwaysOnTop, !initialTop)
        manager.toggleAlwaysOnTop()
        XCTAssertEqual(manager.isAlwaysOnTop, initialTop)

        // Close window
        manager.closeDetachedWindow()
        XCTAssertFalse(manager.isWindowOpen)

        // Bring to front restores / opens
        manager.bringToFront()
        XCTAssertTrue(manager.isWindowOpen)

        // Dock to menu bar closes window
        manager.dockToMenuBar()
        XCTAssertFalse(manager.isWindowOpen)
    }

    func testDaemonClientSummarySubtitle() {
        let client = DaemonClient.shared
        // With no sessions, should return idle ready string
        XCTAssertFalse(client.summarySubtitle.isEmpty)
    }
}
