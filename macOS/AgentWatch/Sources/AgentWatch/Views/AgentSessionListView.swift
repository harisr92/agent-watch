import SwiftUI

public struct AgentSessionListView: View {
    @ObservedObject var client = DaemonClient.shared
    public var recentLimit: Int?

    public init(recentLimit: Int? = 3) {
        self.recentLimit = recentLimit
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            // 1. NEEDS ATTENTION (URGENT GLASS CARDS)
            if !client.waitingSessions.isEmpty {
                VStack(alignment: .leading, spacing: 10) {
                    HStack {
                        HStack(spacing: 5) {
                            Image(systemName: "bell.badge.fill")
                                .font(.system(size: 11))
                            Text("NEEDS ATTENTION")
                                .font(.system(size: 11, weight: .bold, design: .rounded))
                        }
                        .foregroundColor(.orange)

                        Spacer()

                        Text("\(client.waitingSessions.count)")
                            .font(.system(size: 11, weight: .bold, design: .rounded))
                            .padding(.horizontal, 7)
                            .padding(.vertical, 1)
                            .background(Color.orange.opacity(0.25))
                            .foregroundColor(.orange)
                            .clipShape(Capsule())
                    }

                    ForEach(client.waitingSessions) { session in
                        if let prompt = session.pendingPrompt {
                            PromptCardView(
                                session: session,
                                prompt: prompt,
                                autoFocus: session.id == client.waitingSessions.first?.id,
                                onInput: { input, pressEnter in
                                    client.submitInput(
                                        sessionId: session.id,
                                        input: input,
                                        pressEnter: pressEnter
                                    )
                                },
                                onFocus: {
                                    client.focusTerminal(for: session)
                                }
                            )
                            .id(prompt.id)
                        }
                    }
                }
            }

            // 2. ACTIVE AGENTS
            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    Text("ACTIVE AGENTS")
                        .font(.system(size: 11, weight: .bold, design: .rounded))
                        .foregroundColor(.secondary)

                    Spacer()

                    Text("\(client.activeSessions.count)")
                        .font(.system(size: 11, weight: .semibold, design: .rounded))
                        .foregroundColor(.secondary)
                }

                if client.activeSessions.isEmpty && client.waitingSessions.isEmpty {
                    VStack(spacing: 10) {
                        Image(systemName: "terminal.fill")
                            .font(.system(size: 28))
                            .foregroundColor(.secondary.opacity(0.4))

                        Text("No agents currently running")
                            .font(.system(size: 13, weight: .medium, design: .rounded))
                            .foregroundColor(.secondary)

                        Text("aw run <command>")
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundColor(.secondary.opacity(0.8))
                            .padding(.horizontal, 10)
                            .padding(.vertical, 4)
                            .background(Color.black.opacity(0.12))
                            .clipShape(RoundedRectangle(cornerRadius: 6))
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 20)
                } else {
                    ForEach(client.activeSessions) { session in
                        AgentRowView(
                            session: session,
                            onFocus: {
                                client.focusTerminal(for: session)
                            },
                            onTerminate: {
                                client.terminateSession(sessionId: session.id)
                            }
                        )
                    }
                }
            }

            // 3. RECENT HISTORY
            if !client.recentSessions.isEmpty {
                VStack(alignment: .leading, spacing: 8) {
                    HStack {
                        Text("RECENT HISTORY")
                            .font(.system(size: 11, weight: .bold, design: .rounded))
                            .foregroundColor(.secondary.opacity(0.8))

                        Spacer()

                        Button {
                            client.clearHistory()
                        } label: {
                            Text("Clear")
                                .font(.system(size: 11, weight: .semibold, design: .rounded))
                                .foregroundColor(.secondary)
                        }
                        .buttonStyle(.plain)
                        .help("Remove all finished sessions from history")
                    }

                    let historyItems = displayedRecentSessions
                    ForEach(historyItems) { session in
                        AgentRowView(
                            session: session,
                            onFocus: {
                                client.focusTerminal(for: session)
                            },
                            onTerminate: {}
                        )
                    }
                }
            }
        }
    }

    private var displayedRecentSessions: [AgentSession] {
        if let limit = recentLimit {
            return Array(client.recentSessions.prefix(limit))
        }
        return client.recentSessions
    }
}
