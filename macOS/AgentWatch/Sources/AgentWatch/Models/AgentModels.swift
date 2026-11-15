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
    case secret
    case acknowledge

    /// Unknown types from a newer daemon fall back to free text instead of failing to decode.
    public init(from decoder: Decoder) throws {
        let raw = try decoder.singleValueContainer().decode(String.self)
        self = PromptType(rawValue: raw) ?? .text
    }

    public var headline: String {
        switch self {
        case .confirm: return "Approval Required"
        case .select: return "Choose an Option"
        case .text: return "Input Required"
        case .secret: return "Secret Required"
        case .acknowledge: return "Waiting to Continue"
        }
    }
}

/// One answer the user can pick: `label` is displayed, `value` is sent to the agent.
public struct PromptChoice: Codable, Hashable {
    public let label: String
    public let value: String

    public init(label: String, value: String) {
        self.label = label
        self.value = value
    }
}

public struct PendingPrompt: Identifiable, Codable {
    /// Prompt text the daemon uses for an agent idling at its chat box (Claude Code's `❯`).
    /// Must match `CHAT_INPUT_PROMPT` in crates/aw-core/src/detector.rs.
    public static let chatInputText = "Waiting for your next message"

    /// An idle chat box appears after every reply, so it is answerable but should not interrupt.
    public var isChatInput: Bool { promptText == Self.chatInputText }

    public let id: String
    public let sessionId: String
    public let promptText: String
    public let promptType: PromptType
    public let options: [String]
    public let choices: [PromptChoice]
    /// Whether a chosen value must be followed by Enter (false for keypress-driven menus).
    public let pressEnter: Bool
    public let createdAt: String

    enum CodingKeys: String, CodingKey {
        case id
        case sessionId = "session_id"
        case promptText = "prompt_text"
        case promptType = "prompt_type"
        case options
        case choices
        case pressEnter = "press_enter"
        case createdAt = "created_at"
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        sessionId = try c.decode(String.self, forKey: .sessionId)
        promptText = try c.decode(String.self, forKey: .promptText)
        promptType = try c.decode(PromptType.self, forKey: .promptType)
        options = try c.decodeIfPresent([String].self, forKey: .options) ?? []
        let decodedChoices = try c.decodeIfPresent([PromptChoice].self, forKey: .choices) ?? []
        // Older daemons only send raw option values; show them as their own labels.
        choices = decodedChoices.isEmpty
            ? options.map { PromptChoice(label: $0, value: $0) }
            : decodedChoices
        pressEnter = try c.decodeIfPresent(Bool.self, forKey: .pressEnter) ?? true
        createdAt = try c.decode(String.self, forKey: .createdAt)
    }

    /// Choices for a yes/no prompt, falling back to y/n when the daemon sent none.
    public var confirmChoices: (approve: PromptChoice, reject: PromptChoice) {
        let approve = choices.first { ["y", "yes"].contains($0.value.lowercased()) }
            ?? PromptChoice(label: "Yes", value: "y")
        let reject = choices.first { ["n", "no"].contains($0.value.lowercased()) }
            ?? PromptChoice(label: "No", value: "n")
        return (approve, reject)
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
