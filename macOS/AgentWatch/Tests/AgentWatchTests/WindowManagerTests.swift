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

    func testInWindowNotification() {
        let manager = WindowManager.shared

        let session = AgentSession(
            id: "s1",
            name: "test-agent",
            command: "python3",
            args: ["agent.py"],
            cwd: "/tmp",
            pid: 1234,
            status: .waitingForInput,
            pendingPrompt: nil,
            startedAt: "2026-11-15T00:00:00Z",
            updatedAt: "2026-11-15T00:00:00Z",
            recentOutput: []
        )

        let promptJSON = """
        {
            "id": "p-123",
            "session_id": "s1",
            "prompt_text": "Confirm delete?",
            "prompt_type": "confirm",
            "created_at": "2026-11-15T00:00:00Z"
        }
        """.data(using: .utf8)!
        let prompt = try! JSONDecoder().decode(PendingPrompt.self, from: promptJSON)

        manager.showInWindowNotification(session: session, prompt: prompt)
        XCTAssertNotNil(manager.activeNotification)
        XCTAssertEqual(manager.activeNotification?.sessionName, "test-agent")
        XCTAssertEqual(manager.activeNotification?.promptText, "Confirm delete?")

        manager.clearInWindowNotification()
        XCTAssertNil(manager.activeNotification)
    }

    func testDismissPromptClearsItLocally() {
        let client = DaemonClient.shared
        let manager = WindowManager.shared
        let prompt = try! JSONDecoder().decode(PendingPrompt.self, from: Data("""
        {"id": "p-dismiss", "session_id": "s-dismiss", "prompt_text": "Proceed? [y/n]",
         "prompt_type": "confirm", "options": ["y", "n"], "created_at": "2026-11-15T00:00:00Z"}
        """.utf8))
        let session = AgentSession(
            id: "s-dismiss", name: "agent", command: "agent", args: [], cwd: "/tmp", pid: nil,
            status: .waitingForInput, pendingPrompt: prompt,
            startedAt: "", updatedAt: "", recentOutput: []
        )
        let saved = client.sessions
        defer { client.sessions = saved }
        client.sessions = [session]
        manager.showInWindowNotification(session: session, prompt: prompt)

        client.applyPromptDismissed(sessionId: "s-dismiss")

        XCTAssertEqual(client.sessions.first?.status, .running)
        XCTAssertNil(client.sessions.first?.pendingPrompt)
        XCTAssertTrue(client.waitingSessions.isEmpty)
        XCTAssertNil(manager.activeNotification, "the banner for the dismissed prompt goes away")

        // Unknown sessions are ignored.
        client.applyPromptDismissed(sessionId: "missing")
        XCTAssertEqual(client.sessions.count, 1)
    }

    func testAgentRequestFocusRouting() {
        let client = DaemonClient.shared
        let manager = WindowManager.shared

        let session = AgentSession(
            id: "s2",
            name: "focus-agent",
            command: "cargo",
            args: ["build"],
            cwd: "/tmp",
            pid: 5678,
            status: .waitingForInput,
            pendingPrompt: nil,
            startedAt: "2026-11-15T00:00:00Z",
            updatedAt: "2026-11-15T00:00:00Z",
            recentOutput: []
        )

        let promptJSON = """
        {
            "id": "p-456",
            "session_id": "s2",
            "prompt_text": "Select environment",
            "prompt_type": "select",
            "options": ["staging", "prod"],
            "created_at": "2026-11-15T00:00:00Z"
        }
        """.data(using: .utf8)!
        let prompt = try! JSONDecoder().decode(PendingPrompt.self, from: promptJSON)

        // When window is open and not active, handleAgentRequest brings window to front
        manager.openDetachedWindow()
        XCTAssertTrue(manager.isWindowOpen)

        client.handleAgentRequest(session: session, prompt: prompt)
        XCTAssertTrue(manager.isWindowOpen)
        XCTAssertEqual(manager.activeNotification?.id, "p-456")

        manager.closeDetachedWindow()
        XCTAssertFalse(manager.isWindowOpen)
    }

    func testWindowMinimumSizeEnforced() {
        let manager = WindowManager.shared
        manager.openDetachedWindow()

        // Check minimum size constants
        XCTAssertGreaterThanOrEqual(WindowManager.minWindowSize.width, 480)
        XCTAssertGreaterThanOrEqual(WindowManager.minWindowSize.height, 480)

        // Delegate clamps any resize attempts below minimum size
        let attemptedSmall = NSSize(width: 250, height: 200)
        let dummyWindow = NSWindow()
        let clamped = manager.windowWillResize(dummyWindow, to: attemptedSmall)
        XCTAssertEqual(clamped.width, WindowManager.minWindowSize.width)
        XCTAssertEqual(clamped.height, WindowManager.minWindowSize.height)

        manager.closeDetachedWindow()
    }
}
