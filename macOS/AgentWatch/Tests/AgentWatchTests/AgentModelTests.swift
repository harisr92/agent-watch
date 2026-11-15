import XCTest
@testable import AgentWatch

final class AgentModelTests: XCTestCase {
    func testSessionJSONDecoding() throws {
        let json = """
        {
            "id": "agent-test-123",
            "name": "python3",
            "command": "python3",
            "args": ["mock_agent.py", "--verbose"],
            "cwd": "/Users/test/projects/my-app",
            "pid": 4242,
            "status": "waiting_for_input",
            "pending_prompt": {
                "id": "prompt-1",
                "session_id": "agent-test-123",
                "prompt_text": "Proceed with deployment? [y/N]",
                "prompt_type": "confirm",
                "options": ["y", "n"],
                "created_at": "2026-11-14T10:00:00Z"
            },
            "started_at": "2026-11-14T09:55:00Z",
            "updated_at": "2026-11-14T10:00:00Z",
            "recent_output": ["Log 1", "Log 2"]
        }
        """.data(using: .utf8)!

        let session = try JSONDecoder().decode(AgentSession.self, from: json)
        XCTAssertEqual(session.id, "agent-test-123")
        XCTAssertEqual(session.name, "python3")
        XCTAssertEqual(session.status, .waitingForInput)
        XCTAssertEqual(session.directoryName, "my-app")
        XCTAssertEqual(session.displayCommand, "python3 mock_agent.py --verbose")
        XCTAssertEqual(session.status.displayName, "Needs Input")

        let prompt = try XCTUnwrap(session.pendingPrompt)
        XCTAssertEqual(prompt.promptType, .confirm)
        XCTAssertEqual(prompt.options, ["y", "n"])
    }

    func testSelectPromptDecodingWithChoices() throws {
        let json = """
        {
            "id": "p2",
            "session_id": "s",
            "prompt_text": "Do you want to make this edit?",
            "prompt_type": "select",
            "options": ["1", "2", "3"],
            "choices": [
                {"label": "Yes", "value": "1"},
                {"label": "Yes, allow all edits", "value": "2"},
                {"label": "No", "value": "3"}
            ],
            "press_enter": false,
            "created_at": "2026-11-14T10:00:00Z"
        }
        """.data(using: .utf8)!

        let prompt = try JSONDecoder().decode(PendingPrompt.self, from: json)
        XCTAssertEqual(prompt.promptType, .select)
        XCTAssertEqual(prompt.choices.map(\.label), ["Yes", "Yes, allow all edits", "No"])
        XCTAssertEqual(prompt.choices.map(\.value), ["1", "2", "3"])
        XCTAssertFalse(prompt.pressEnter)
    }

    func testLegacyPromptFallsBackToOptions() throws {
        let json = """
        {
            "id": "p3",
            "session_id": "s",
            "prompt_text": "Deploy where?",
            "prompt_type": "some_future_type",
            "options": ["staging", "production"],
            "created_at": "2026-11-14T10:00:00Z"
        }
        """.data(using: .utf8)!

        let prompt = try JSONDecoder().decode(PendingPrompt.self, from: json)
        XCTAssertEqual(prompt.promptType, .text)
        XCTAssertEqual(prompt.choices, [
            PromptChoice(label: "staging", value: "staging"),
            PromptChoice(label: "production", value: "production"),
        ])
        XCTAssertTrue(prompt.pressEnter)
        XCTAssertEqual(prompt.confirmChoices.approve.value, "y")
    }

    func testUserInputPayloadEncoding() throws {
        let payload = UserInputPayload(input: "yes", pressEnter: true)
        let data = try JSONEncoder().encode(payload)
        let json = try JSONSerialization.jsonObject(with: data) as? [String: Any]

        XCTAssertEqual(json?["input"] as? String, "yes")
        XCTAssertEqual(json?["press_enter"] as? Bool, true)
    }

    func testActivityDecodingAndDisplay() throws {
        func session(status: String, extra: String) throws -> AgentSession {
            let json = """
            {"id": "s", "name": "claude", "command": "claude", "args": [], "cwd": "/tmp/app",
             "pid": null, "status": "\(status)", "pending_prompt": null,
             "started_at": "", "updated_at": "", "recent_output": []\(extra)}
            """
            return try JSONDecoder().decode(AgentSession.self, from: Data(json.utf8))
        }

        let working = try session(status: "running", extra: #", "activity": "working", "task_title": "Fix bug""#)
        XCTAssertEqual(working.activity, .working)
        XCTAssertEqual(working.taskTitle, "Fix bug")
        XCTAssertEqual(working.statusDisplayName, "Working")

        let done = try session(status: "waiting_for_input", extra: #", "activity": "done""#)
        XCTAssertEqual(done.statusDisplayName, "Done")

        // Older daemons send no activity; exited sessions show their process status.
        let legacy = try session(status: "running", extra: "")
        XCTAssertNil(legacy.activity)
        XCTAssertNil(legacy.taskTitle)
        XCTAssertEqual(legacy.statusDisplayName, "Running")
        let exited = try session(status: "completed", extra: #", "activity": "working""#)
        XCTAssertEqual(exited.statusDisplayName, "Completed")
    }

    func testFinishedTurnOnlyOnTransitionToDone() {
        XCTAssertTrue(AgentActivity.finishedTurn(from: .working, to: .done))
        XCTAssertTrue(AgentActivity.finishedTurn(from: .waiting, to: .done))
        XCTAssertFalse(AgentActivity.finishedTurn(from: .done, to: .done))
        XCTAssertFalse(AgentActivity.finishedTurn(from: nil, to: .done))
        XCTAssertFalse(AgentActivity.finishedTurn(from: .working, to: .waiting))
        XCTAssertFalse(AgentActivity.finishedTurn(from: .working, to: nil))
    }

    func testStatusAttributes() {
        XCTAssertEqual(AgentStatus.running.displayName, "Running")
        XCTAssertEqual(AgentStatus.waitingForInput.displayName, "Needs Input")
        XCTAssertEqual(AgentStatus.completed.displayName, "Completed")
        XCTAssertEqual(AgentStatus.failed.displayName, "Failed")
    }
}
