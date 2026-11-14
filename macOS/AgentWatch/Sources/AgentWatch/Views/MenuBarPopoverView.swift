import SwiftUI

public struct MenuBarPopoverView: View {
    @ObservedObject var client = DaemonClient.shared

    public init() {}

    public var body: some View {
        VStack(spacing: 0) {
            // Apple Glass Header
            HStack(spacing: 10) {
                ZStack {
                    Circle()
                        .fill(
                            LinearGradient(
                                colors: [Color.blue.opacity(0.3), Color.purple.opacity(0.2)],
                                startPoint: .topLeading,
                                endPoint: .bottomTrailing
                            )
                        )
                        .frame(width: 28, height: 28)
                    Image(systemName: "eyes.inverse")
                        .font(.system(size: 14, weight: .bold))
                        .foregroundColor(.blue)
                }

                VStack(alignment: .leading, spacing: 1) {
                    Text("Agent Watch")
                        .font(.system(size: 14, weight: .bold, design: .rounded))
                    Text(summarySubtitle)
                        .font(.system(size: 10, design: .rounded))
                        .foregroundColor(.secondary)
                }

                Spacer()

                // Connection badge (glass pill)
                HStack(spacing: 5) {
                    Circle()
                        .fill(client.isConnected ? Color.green : Color.red)
                        .frame(width: 7, height: 7)

                    Text(client.isConnected ? "Live" : "Offline")
                        .font(.system(size: 11, weight: .medium, design: .rounded))
                        .foregroundColor(.secondary)
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 3)
                .background(.ultraThinMaterial)
                .clipShape(Capsule())
                .overlay(
                    Capsule()
                        .stroke(Color.white.opacity(0.2), lineWidth: 0.5)
                )

                if !client.isConnected {
                    Button("Start Daemon") {
                        client.launchDaemon()
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
            .background(
                LinearGradient(
                    colors: [Color.white.opacity(0.08), Color.clear],
                    startPoint: .top,
                    endPoint: .bottom
                )
            )

            Divider()
                .opacity(0.3)

            // Scrollable Content
            ScrollView {
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
                            Text("RECENT HISTORY")
                                .font(.system(size: 11, weight: .bold, design: .rounded))
                                .foregroundColor(.secondary.opacity(0.8))

                            ForEach(client.recentSessions.prefix(3)) { session in
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
                .padding(16)
            }
            .frame(maxHeight: 460)

            Divider()
                .opacity(0.3)

            // Apple Glass Footer
            HStack {
                Button(action: {
                    client.fetchSessions()
                    client.connectWebSocket()
                }) {
                    HStack(spacing: 4) {
                        Image(systemName: "arrow.clockwise")
                        Text("Refresh")
                    }
                    .font(.system(size: 11, design: .rounded))
                    .foregroundColor(.secondary)
                }
                .buttonStyle(.plain)

                Spacer()

                Button("Quit") {
                    NSApplication.shared.terminate(nil)
                }
                .buttonStyle(.plain)
                .font(.system(size: 11, design: .rounded))
                .foregroundColor(.secondary.opacity(0.8))
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            .background(Color.white.opacity(0.04))
        }
        .frame(width: 400)
        .background(.ultraThinMaterial)
    }

    private var summarySubtitle: String {
        let waiting = client.waitingSessions.count
        let active = client.activeSessions.count
        if waiting > 0 {
            return "\(waiting) waiting for input • \(active) running"
        } else if active > 0 {
            return "\(active) active agent\(active == 1 ? "" : "s")"
        } else {
            return "Ready to monitor agents"
        }
    }
}
