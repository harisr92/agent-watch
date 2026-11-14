import XCTest
import SwiftUI
import AppKit
@testable import AgentWatch

@MainActor
final class PromptCardTests: XCTestCase {

    private func makeSession(prompt: PendingPrompt) -> AgentSession {
        AgentSession(
            id: prompt.sessionId,
            name: "test-agent",
            command: "python3",
            args: ["agent.py"],
            cwd: "/tmp",
            pid: 1234,
            status: .waitingForInput,
            pendingPrompt: prompt,
            startedAt: "2026-11-15T00:00:00Z",
            updatedAt: "2026-11-15T00:00:00Z",
            recentOutput: []
        )
    }

    private func decodePrompt(json: String) -> PendingPrompt {
        try! JSONDecoder().decode(PendingPrompt.self, from: json.data(using: .utf8)!)
    }

    func testConfirmPromptEscTriggersCancel() {
        let prompt = decodePrompt(json: """
        {
            "id": "p-confirm-1",
            "session_id": "s-1",
            "prompt_text": "Do you want to proceed? [y/N]",
            "prompt_type": "confirm",
            "choices": [{"label": "Yes", "value": "y"}, {"label": "No", "value": "n"}],
            "created_at": "2026-11-15T00:00:00Z"
        }
        """)
        let session = makeSession(prompt: prompt)

        var receivedInput: String?
        var receivedPressEnter: Bool?

        let card = PromptCardView(
            session: session,
            prompt: prompt,
            autoFocus: true,
            onInput: { input, pressEnter in
                receivedInput = input
                receivedPressEnter = pressEnter
            },
            onFocus: {}
        )

        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 400), styleMask: [.titled], backing: .buffered, defer: false)
        window.contentView = NSHostingView(rootView: card)
        window.makeKeyAndOrderFront(nil)
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.15))

        // Check auto focus
        let firstResp = window.firstResponder
        XCTAssertTrue(firstResp is NSText || firstResp is NSTextField, "Input field should be automatically focused")

        // Press Esc
        let escEvent = NSEvent.keyEvent(
            with: .keyDown,
            location: .zero,
            modifierFlags: [],
            timestamp: 0,
            windowNumber: window.windowNumber,
            context: nil,
            characters: "\u{1b}",
            charactersIgnoringModifiers: "\u{1b}",
            isARepeat: false,
            keyCode: 53
        )!
        window.sendEvent(escEvent)
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.1))

        XCTAssertEqual(receivedInput, "n", "Esc key should cancel/reject confirm prompt")
        XCTAssertEqual(receivedPressEnter, true)
    }

    func testConfirmPromptEnterTriggersAccept() {
        let prompt = decodePrompt(json: """
        {
            "id": "p-confirm-2",
            "session_id": "s-2",
            "prompt_text": "Do you want to proceed? [y/N]",
            "prompt_type": "confirm",
            "choices": [{"label": "Yes", "value": "y"}, {"label": "No", "value": "n"}],
            "created_at": "2026-11-15T00:00:00Z"
        }
        """)
        let session = makeSession(prompt: prompt)

        var receivedInput: String?
        var receivedPressEnter: Bool?

        let card = PromptCardView(
            session: session,
            prompt: prompt,
            autoFocus: true,
            onInput: { input, pressEnter in
                receivedInput = input
                receivedPressEnter = pressEnter
            },
            onFocus: {}
        )

        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 400), styleMask: [.titled], backing: .buffered, defer: false)
        window.contentView = NSHostingView(rootView: card)
        window.makeKeyAndOrderFront(nil)
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.15))

        // Press Enter
        let returnEvent = NSEvent.keyEvent(
            with: .keyDown,
            location: .zero,
            modifierFlags: [],
            timestamp: 0,
            windowNumber: window.windowNumber,
            context: nil,
            characters: "\r",
            charactersIgnoringModifiers: "\r",
            isARepeat: false,
            keyCode: 36
        )!
        window.sendEvent(returnEvent)
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.1))

        XCTAssertEqual(receivedInput, "y", "Enter key should accept/approve confirm prompt")
        XCTAssertEqual(receivedPressEnter, true)
    }

    func testSelectPromptEscTriggersCancelChoice() {
        let prompt = decodePrompt(json: """
        {
            "id": "p-select-1",
            "session_id": "s-3",
            "prompt_text": "Choose action",
            "prompt_type": "select",
            "choices": [
                {"label": "Run build", "value": "build"},
                {"label": "Cancel", "value": "cancel"}
            ],
            "press_enter": true,
            "created_at": "2026-11-15T00:00:00Z"
        }
        """)
        let session = makeSession(prompt: prompt)

        var receivedInput: String?
        var receivedPressEnter: Bool?

        let card = PromptCardView(
            session: session,
            prompt: prompt,
            autoFocus: true,
            onInput: { input, pressEnter in
                receivedInput = input
                receivedPressEnter = pressEnter
            },
            onFocus: {}
        )

        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 400), styleMask: [.titled], backing: .buffered, defer: false)
        window.contentView = NSHostingView(rootView: card)
        window.makeKeyAndOrderFront(nil)
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.15))

        // Press Esc
        let escEvent = NSEvent.keyEvent(
            with: .keyDown,
            location: .zero,
            modifierFlags: [],
            timestamp: 0,
            windowNumber: window.windowNumber,
            context: nil,
            characters: "\u{1b}",
            charactersIgnoringModifiers: "\u{1b}",
            isARepeat: false,
            keyCode: 53
        )!
        window.sendEvent(escEvent)
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.1))

        XCTAssertEqual(receivedInput, "cancel", "Esc key should pick cancel choice in select prompt")
        XCTAssertEqual(receivedPressEnter, true)
    }

    func testTextPromptEscSendsEscapeByte() {
        let prompt = decodePrompt(json: """
        {
            "id": "p-text-1",
            "session_id": "s-4",
            "prompt_text": "Enter query:",
            "prompt_type": "text",
            "created_at": "2026-11-15T00:00:00Z"
        }
        """)
        let session = makeSession(prompt: prompt)

        var receivedInput: String?
        var receivedPressEnter: Bool?

        let card = PromptCardView(
            session: session,
            prompt: prompt,
            autoFocus: true,
            onInput: { input, pressEnter in
                receivedInput = input
                receivedPressEnter = pressEnter
            },
            onFocus: {}
        )

        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 400), styleMask: [.titled], backing: .buffered, defer: false)
        window.contentView = NSHostingView(rootView: card)
        window.makeKeyAndOrderFront(nil)
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.15))

        // Input field should be auto focused
        let firstResp = window.firstResponder
        XCTAssertTrue(firstResp is NSText || firstResp is NSTextField, "Input field in text prompt should be auto-focused")

        // Press Esc
        let escEvent = NSEvent.keyEvent(
            with: .keyDown,
            location: .zero,
            modifierFlags: [],
            timestamp: 0,
            windowNumber: window.windowNumber,
            context: nil,
            characters: "\u{1b}",
            charactersIgnoringModifiers: "\u{1b}",
            isARepeat: false,
            keyCode: 53
        )!
        window.sendEvent(escEvent)
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.1))

        XCTAssertEqual(receivedInput, "\u{1b}", "Esc key should send escape byte on text prompt")
        XCTAssertEqual(receivedPressEnter, false)
    }

    func testSelectPromptEnterPicksFirstChoice() {
        let prompt = decodePrompt(json: """
        {
            "id": "p-select-2",
            "session_id": "s-5",
            "prompt_text": "Choose action",
            "prompt_type": "select",
            "choices": [
                {"label": "Run build", "value": "build"},
                {"label": "Cancel", "value": "cancel"}
            ],
            "press_enter": true,
            "created_at": "2026-11-15T00:00:00Z"
        }
        """)
        let session = makeSession(prompt: prompt)

        var receivedInput: String?
        var receivedPressEnter: Bool?

        let card = PromptCardView(
            session: session,
            prompt: prompt,
            autoFocus: true,
            onInput: { input, pressEnter in
                receivedInput = input
                receivedPressEnter = pressEnter
            },
            onFocus: {}
        )

        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 400), styleMask: [.titled], backing: .buffered, defer: false)
        window.contentView = NSHostingView(rootView: card)
        window.makeKeyAndOrderFront(nil)
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.15))

        // Press Enter
        let returnEvent = NSEvent.keyEvent(
            with: .keyDown,
            location: .zero,
            modifierFlags: [],
            timestamp: 0,
            windowNumber: window.windowNumber,
            context: nil,
            characters: "\r",
            charactersIgnoringModifiers: "\r",
            isARepeat: false,
            keyCode: 36
        )!
        window.sendEvent(returnEvent)
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.1))

        XCTAssertEqual(receivedInput, "build", "Enter key on empty select prompt should pick first choice")
        XCTAssertEqual(receivedPressEnter, true)
    }

    func testTextPromptTypingAndEnterTriggersInput() {
        let prompt = decodePrompt(json: """
        {
            "id": "p-text-2",
            "session_id": "s-6",
            "prompt_text": "Enter query:",
            "prompt_type": "text",
            "created_at": "2026-11-15T00:00:00Z"
        }
        """)
        let session = makeSession(prompt: prompt)

        var receivedInput: String?
        var receivedPressEnter: Bool?

        let card = PromptCardView(
            session: session,
            prompt: prompt,
            autoFocus: true,
            onInput: { input, pressEnter in
                receivedInput = input
                receivedPressEnter = pressEnter
            },
            onFocus: {}
        )

        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 400), styleMask: [.titled], backing: .buffered, defer: false)
        window.contentView = NSHostingView(rootView: card)
        window.makeKeyAndOrderFront(nil)
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.15))

        if let textView = window.firstResponder as? NSTextView {
            textView.insertText("search query", replacementRange: NSRange(location: 0, length: 0))
            RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.05))
        }

        // Press Enter
        let returnEvent = NSEvent.keyEvent(
            with: .keyDown,
            location: .zero,
            modifierFlags: [],
            timestamp: 0,
            windowNumber: window.windowNumber,
            context: nil,
            characters: "\r",
            charactersIgnoringModifiers: "\r",
            isARepeat: false,
            keyCode: 36
        )!
        window.sendEvent(returnEvent)
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.1))

        XCTAssertEqual(receivedInput, "search query", "Typed text should be submitted on Enter")
        XCTAssertEqual(receivedPressEnter, true)
    }
}
