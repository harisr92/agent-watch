import SwiftUI

public struct MenuBarPopoverView: View {
    @ObservedObject var client = DaemonClient.shared
    @ObservedObject var windowManager = WindowManager.shared

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
                    Text(client.summarySubtitle)
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
                .liquidGlass(in: Capsule())

                // Separate Window Icon Button
                Button(action: {
                    windowManager.openDetachedWindow()
                }) {
                    Image(systemName: "macwindow.on.rectangle")
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundColor(.primary.opacity(0.85))
                        .frame(width: 26, height: 26)
                        .liquidGlass(in: RoundedRectangle(cornerRadius: 6), interactive: true)
                }
                .buttonStyle(.plain)
                .help("Separate into floating Apple glass window")

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

            // Status Banner when separate glass window is active
            if windowManager.isWindowOpen {
                HStack(spacing: 8) {
                    Image(systemName: "macwindow.fill")
                        .font(.system(size: 11))
                        .foregroundColor(.blue)

                    Text("Separate glass window active")
                        .font(.system(size: 11, weight: .medium, design: .rounded))
                        .foregroundColor(.secondary)

                    Spacer()

                    Button("Bring to Front") {
                        windowManager.bringToFront()
                        StatusBarController.shared.hidePopover()
                    }
                    .font(.system(size: 10, weight: .semibold, design: .rounded))
                    .buttonStyle(.bordered)
                    .controlSize(.mini)

                    Button("Dock Back") {
                        windowManager.closeDetachedWindow()
                    }
                    .font(.system(size: 10, weight: .semibold, design: .rounded))
                    .buttonStyle(.borderless)
                    .controlSize(.mini)
                    .foregroundColor(.secondary)
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 6)
                .background(Color.blue.opacity(0.08))
                .overlay(
                    Rectangle()
                        .stroke(Color.blue.opacity(0.18), lineWidth: 0.5)
                )
            }

            Divider()
                .opacity(0.3)

            // Scrollable Content
            ScrollView {
                AgentSessionListView(recentLimit: 3)
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

                // Separate Window text button
                Button(action: {
                    if windowManager.isWindowOpen {
                        windowManager.bringToFront()
                        StatusBarController.shared.hidePopover()
                    } else {
                        windowManager.openDetachedWindow()
                    }
                }) {
                    HStack(spacing: 5) {
                        Image(systemName: "macwindow.on.rectangle")
                            .font(.system(size: 11))
                        Text(windowManager.isWindowOpen ? "Focus Window" : "Separate Window")
                    }
                    .font(.system(size: 11, weight: .medium, design: .rounded))
                    .foregroundColor(.secondary)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 3)
                    .background(Color.white.opacity(0.06))
                    .clipShape(RoundedRectangle(cornerRadius: 5))
                    .overlay(
                        RoundedRectangle(cornerRadius: 5)
                            .stroke(Color.white.opacity(0.12), lineWidth: 0.5)
                    )
                }
                .buttonStyle(.plain)
                .help("Separate into floating Apple glass window")

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
    }
}
