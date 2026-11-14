import Foundation
import SwiftUI

public enum AgentStatus: String, Codable {
    case idle
    case running
    case waitingForInput = "waiting_for_input"
    case completed
    case failed

    public var displayName: String {
        switch self {
        case .idle: return "Idle"
        case .running: return "Running"
        case .waitingForInput: return "Needs Input"
        case .completed: return "Completed"
        case .failed: return "Failed"
        }
    }

    public var color: Color {
        switch self {
        case .idle: return .secondary
        case .running: return .blue
        case .waitingForInput: return .orange
        case .completed: return .green
        case .failed: return .red
        }
    }

    public var iconName: String {
        switch self {
        case .idle: return "pause.circle"
        case .running: return "arrow.triangle.2.circlepath"
        case .waitingForInput: return "exclamationmark.bubble.fill"
        case .completed: return "checkmark.circle.fill"
        case .failed: return "xmark.octagon.fill"
        }
    }
}

public enum PromptType: String, Codable {
    case confirm
    case text
    case select
}

public struct PendingPrompt: Identifiable, Codable {
    public let id: String
    public let sessionId: String
    public let promptText: String
    public let promptType: PromptType
    public let options: [String]
    public let createdAt: String

    enum CodingKeys: String, CodingKey {
        case id
        case sessionId = "session_id"
        case promptText = "prompt_text"
        case promptType = "prompt_type"
        case options
        case createdAt = "created_at"
    }
}

public struct AgentSession: Identifiable, Codable {
    public let id: String
    public let name: String
    public let command: String
    public let args: [String]
    public let cwd: String
    public let pid: Int?
    public var status: AgentStatus
    public var pendingPrompt: PendingPrompt?
    public let startedAt: String
    public var updatedAt: String
    public var recentOutput: [String]

    enum CodingKeys: String, CodingKey {
        case id
        case name
        case command
        case args
        case cwd
        case pid
        case status
        case pendingPrompt = "pending_prompt"
        case startedAt = "started_at"
        case updatedAt = "updated_at"
        case recentOutput = "recent_output"
    }

    public var directoryName: String {
        URL(fileURLWithPath: cwd).lastPathComponent
    }

    public var displayCommand: String {
        ([command] + args).joined(separator: " ")
    }
}

public struct UserInputPayload: Codable {
    public let input: String
    public let pressEnter: Bool

    enum CodingKeys: String, CodingKey {
        case input
        case pressEnter = "press_enter"
    }

    public init(input: String, pressEnter: Bool = true) {
        self.input = input
        self.pressEnter = pressEnter
    }
}
