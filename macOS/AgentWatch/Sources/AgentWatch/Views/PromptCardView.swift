import SwiftUI

public struct PromptCardView: View {
    public let session: AgentSession
    public let prompt: PendingPrompt
    /// Sends `input` to the agent; the flag says whether Enter follows it.
    public let onInput: (String, Bool) -> Void
    public let onFocus: () -> Void

    @State private var inputText: String = ""
    @State private var hoveredChoice: PromptChoice?

    public var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            header
            questionBox
            answerArea
        }
        .padding(14)
        .background(
            RoundedRectangle(cornerRadius: 14)
                .fill(
                    LinearGradient(
                        colors: [Color.orange.opacity(0.18), Color.orange.opacity(0.06)],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                )
        )
        .overlay(
            RoundedRectangle(cornerRadius: 14)
                .stroke(
                    LinearGradient(
                        colors: [Color.orange.opacity(0.55), Color.white.opacity(0.2), Color.orange.opacity(0.15)],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    ),
                    lineWidth: 1.2
                )
        )
        .shadow(color: Color.orange.opacity(0.2), radius: 12, x: 0, y: 4)
    }

    // MARK: - Header

    private var header: some View {
        HStack {
            HStack(spacing: 8) {
                ZStack {
                    Circle()
                        .fill(Color.orange.opacity(0.2))
                        .frame(width: 24, height: 24)
                    Image(systemName: headerIcon)
                        .font(.system(size: 12, weight: .bold))
                        .foregroundColor(.orange)
                }

                VStack(alignment: .leading, spacing: 1) {
                    Text(session.name)
                        .font(.system(.subheadline, design: .rounded))
                        .fontWeight(.bold)
                        .foregroundColor(.primary)

                    Text(prompt.promptType.headline)
                        .font(.system(size: 10, weight: .semibold, design: .rounded))
                        .foregroundColor(.orange)
                }
            }

            Spacer()

            // Directory pill (glass style)
            HStack(spacing: 4) {
                Image(systemName: "folder.fill")
                    .font(.system(size: 9))
                Text(session.directoryName)
                    .font(.system(size: 11, weight: .medium, design: .monospaced))
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 3)
            .background(.ultraThinMaterial)
            .overlay(
                Capsule()
                    .stroke(Color.white.opacity(0.2), lineWidth: 0.5)
            )
            .clipShape(Capsule())

            // Focus terminal button
            Button(action: onFocus) {
                Image(systemName: "arrow.up.forward.app")
                    .font(.system(size: 12))
                    .foregroundColor(.secondary)
                    .padding(5)
                    .background(.ultraThinMaterial)
                    .clipShape(Circle())
            }
            .buttonStyle(.plain)
            .help("Jump to Terminal")
        }
    }

    private var headerIcon: String {
        switch prompt.promptType {
        case .confirm: return "checkmark.shield.fill"
        case .select: return "list.bullet.circle.fill"
        case .text: return "exclamationmark.bubble.fill"
        case .secret: return "key.fill"
        case .acknowledge: return "return"
        }
    }

    // MARK: - Question

    private var questionBox: some View {
        Text(prompt.promptText)
            .font(.system(size: 13, weight: .medium, design: .monospaced))
            .foregroundColor(.primary)
            .lineSpacing(3)
            .textSelection(.enabled)
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                RoundedRectangle(cornerRadius: 10)
                    .fill(Color.black.opacity(0.18))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 10)
                    .stroke(
                        LinearGradient(
                            colors: [Color.orange.opacity(0.5), Color.orange.opacity(0.15)],
                            startPoint: .topLeading,
                            endPoint: .bottomTrailing
                        ),
                        lineWidth: 1
                    )
            )
    }

    // MARK: - Answers

    @ViewBuilder
    private var answerArea: some View {
        switch prompt.promptType {
        case .confirm:
            confirmAnswers
        case .select:
            if prompt.choices.isEmpty {
                inputField(placeholder: "Type your choice...", secure: false)
            } else {
                selectAnswers
            }
        case .text:
            inputField(placeholder: "Type your reply...", secure: false)
        case .secret:
            VStack(alignment: .leading, spacing: 6) {
                inputField(placeholder: "Enter secret (hidden)...", secure: true)
                Label("Sent straight to the agent's terminal and never stored.", systemImage: "lock.fill")
                    .font(.system(size: 10, design: .rounded))
                    .foregroundColor(.secondary)
            }
        case .acknowledge:
            HStack {
                pillButton(title: "Continue", icon: "return", color: .blue) {
                    onInput(prompt.choices.first?.value ?? "", true)
                }
                Spacer()
            }
        }
    }

    private var confirmAnswers: some View {
        let pair = prompt.confirmChoices
        return HStack(spacing: 8) {
            pillButton(title: "Approve", icon: "checkmark", color: .green) {
                onInput(pair.approve.value, prompt.pressEnter)
            }
            .help("Sends \"\(pair.approve.value)\"")

            pillButton(title: "Reject", icon: "xmark", color: .red) {
                onInput(pair.reject.value, prompt.pressEnter)
            }
            .help("Sends \"\(pair.reject.value)\"")

            inputField(placeholder: "Or type a custom reply...", secure: false)
        }
    }

    private var selectAnswers: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(Array(prompt.choices.enumerated()), id: \.offset) { index, choice in
                choiceRow(index: index, choice: choice)
            }

            // Arrow-key menus can't take typed answers; everything else can.
            if prompt.pressEnter {
                inputField(placeholder: "Or type another answer...", secure: false)
                    .padding(.top, 2)
            }
        }
    }

    private func choiceRow(index: Int, choice: PromptChoice) -> some View {
        let isHovered = hoveredChoice == choice
        return Button(action: { onInput(choice.value, prompt.pressEnter) }) {
            HStack(spacing: 10) {
                Text(badge(for: index, choice: choice))
                    .font(.system(size: 11, weight: .bold, design: .rounded))
                    .foregroundColor(.orange)
                    .frame(width: 22, height: 22)
                    .background(Circle().fill(Color.orange.opacity(0.18)))

                Text(choice.label)
                    .font(.system(size: 12, weight: .medium, design: .rounded))
                    .foregroundColor(.primary)
                    .multilineTextAlignment(.leading)
                    .frame(maxWidth: .infinity, alignment: .leading)

                Image(systemName: "chevron.right")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundColor(.secondary)
                    .opacity(isHovered ? 1 : 0)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .background(
                RoundedRectangle(cornerRadius: 8)
                    .fill(isHovered ? Color.orange.opacity(0.16) : Color.white.opacity(0.05))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 8)
                    .stroke(Color.white.opacity(isHovered ? 0.3 : 0.15), lineWidth: 0.5)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovering in
            hoveredChoice = hovering ? choice : (hoveredChoice == choice ? nil : hoveredChoice)
        }
    }

    /// Shows the key the agent expects when it is short ("1", "a"), else the position.
    private func badge(for index: Int, choice: PromptChoice) -> String {
        let value = choice.value.trimmingCharacters(in: .whitespacesAndNewlines)
        if (1...2).contains(value.count), value.allSatisfy({ $0.isLetter || $0.isNumber }) {
            return value.uppercased()
        }
        return "\(index + 1)"
    }

    // MARK: - Building blocks

    private func pillButton(title: String, icon: String, color: Color, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 5) {
                Image(systemName: icon)
                    .font(.system(size: 11, weight: .bold))
                Text(title)
                    .font(.system(size: 12, weight: .semibold, design: .rounded))
            }
            .foregroundColor(.white)
            .padding(.horizontal, 14)
            .padding(.vertical, 7)
            .background(
                LinearGradient(
                    colors: [color.opacity(0.9), color.opacity(0.75)],
                    startPoint: .top,
                    endPoint: .bottom
                )
            )
            .clipShape(Capsule())
            .overlay(
                Capsule()
                    .stroke(Color.white.opacity(0.35), lineWidth: 0.8)
            )
            .shadow(color: color.opacity(0.35), radius: 6, x: 0, y: 2)
        }
        .buttonStyle(.plain)
    }

    private func inputField(placeholder: String, secure: Bool) -> some View {
        HStack {
            Group {
                if secure {
                    SecureField(placeholder, text: $inputText)
                } else {
                    TextField(placeholder, text: $inputText)
                }
            }
            .textFieldStyle(.plain)
            .font(.system(size: 12, design: .rounded))
            .onSubmit(submitTyped)

            if !inputText.isEmpty {
                Button(action: submitTyped) {
                    Image(systemName: "arrow.up.circle.fill")
                        .font(.system(size: 16))
                        .foregroundColor(.blue)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(.ultraThinMaterial)
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .stroke(Color.white.opacity(0.25), lineWidth: 0.5)
        )
        .cornerRadius(8)
    }

    private func submitTyped() {
        // Secrets may legitimately be empty (e.g. a key with no passphrase); other replies may not.
        guard !inputText.isEmpty || prompt.promptType == .secret else { return }
        onInput(inputText, true)
        inputText = ""
    }
}
