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

    func testUserInputPayloadEncoding() throws {
        let payload = UserInputPayload(input: "yes", pressEnter: true)
        let data = try JSONEncoder().encode(payload)
        let json = try JSONSerialization.jsonObject(with: data) as? [String: Any]

        XCTAssertEqual(json?["input"] as? String, "yes")
        XCTAssertEqual(json?["press_enter"] as? Bool, true)
    }

    func testStatusAttributes() {
        XCTAssertEqual(AgentStatus.running.displayName, "Running")
        XCTAssertEqual(AgentStatus.waitingForInput.displayName, "Needs Input")
        XCTAssertEqual(AgentStatus.completed.displayName, "Completed")
        XCTAssertEqual(AgentStatus.failed.displayName, "Failed")
    }
}
