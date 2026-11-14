import SwiftUI

public struct DetachedGlassWindowView: View {
    @ObservedObject var client = DaemonClient.shared
    @ObservedObject var windowManager = WindowManager.shared

    public init() {}

    public var body: some View {
        ZStack {
            // Apple Glass background effect
            VisualEffectView(material: .underWindowBackground, blendingMode: .behindWindow)

            // Specular glass ambient gradient
            LinearGradient(
                colors: [
                    Color.white.opacity(0.12),
                    Color.white.opacity(0.03),
                    Color.black.opacity(0.06)
                ],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )

            // Top edge specular refraction line
            VStack {
                LinearGradient(
                    colors: [
                        Color.white.opacity(0.40),
                        Color.white.opacity(0.10),
                        Color.clear
                    ],
                    startPoint: .top,
                    endPoint: .bottom
                )
                .frame(height: 1.5)

                Spacer()
            }

            // Main Window Content Layout
            VStack(spacing: 0) {
                // Header (with traffic lights clearance)
                glassHeader

                // In-Window Notification Alert Banner
                if let notif = windowManager.activeNotification {
                    inWindowNotificationBanner(notif)
                }

                Divider()
                    .opacity(0.25)

                // Scrollable Content that expands to window size
                ScrollView {
                    AgentSessionListView(recentLimit: nil)
                        .padding(18)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)

                Divider()
                    .opacity(0.25)

                // Glass Footer
                glassFooter
            }
        }
        .frame(minWidth: WindowManager.minWindowSize.width, maxWidth: .infinity, minHeight: WindowManager.minWindowSize.height, maxHeight: .infinity)
        .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 16, style: .continuous)
                .stroke(
                    LinearGradient(
                        colors: [
                            Color.white.opacity(0.35),
                            Color.white.opacity(0.14),
                            Color.white.opacity(0.04),
                            Color.white.opacity(0.18)
                        ],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    ),
                    lineWidth: 1
                )
        )
    }

    // MARK: - Header
    private var glassHeader: some View {
        HStack(spacing: 10) {
            // Native macOS traffic light offset
            Spacer()
                .frame(width: 68)

            // Logo & Title
            HStack(spacing: 8) {
                ZStack {
                    Circle()
                        .fill(
                            LinearGradient(
                                colors: [Color.blue.opacity(0.35), Color.purple.opacity(0.25)],
                                startPoint: .topLeading,
                                endPoint: .bottomTrailing
                            )
                        )
                        .frame(width: 28, height: 28)
                    Image(systemName: "eyes.inverse")
                        .font(.system(size: 14, weight: .bold))
                        .foregroundColor(.blue)
                }

                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 6) {
                        Text("Agent Watch")
                            .font(.system(size: 14, weight: .bold, design: .rounded))
                            .lineLimit(1)
                            .fixedSize(horizontal: true, vertical: false)

                        Text("Glass")
                            .font(.system(size: 9.5, weight: .semibold, design: .rounded))
                            .foregroundColor(.blue.opacity(0.9))
                            .padding(.horizontal, 6)
                            .padding(.vertical, 1.5)
                            .background(Color.blue.opacity(0.15))
                            .clipShape(Capsule())
                            .lineLimit(1)
                            .fixedSize(horizontal: true, vertical: false)
                    }

                    Text(client.summarySubtitle)
                        .font(.system(size: 10.5, design: .rounded))
                        .foregroundColor(.secondary)
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
            }
            .layoutPriority(1)

            Spacer(minLength: 8)

            // Header Action Controls (Glass Pills)
            HStack(spacing: 6) {
                // Connection badge
                HStack(spacing: 5) {
                    Circle()
                        .fill(client.isConnected ? Color.green : Color.red)
                        .frame(width: 7, height: 7)

                    Text(client.isConnected ? "Live" : "Offline")
                        .font(.system(size: 11, weight: .medium, design: .rounded))
                        .foregroundColor(.secondary)
                        .lineLimit(1)
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .background(.ultraThinMaterial)
                .clipShape(Capsule())
                .overlay(
                    Capsule()
                        .stroke(Color.white.opacity(0.2), lineWidth: 0.5)
                )

                // Always on top (Pin) button
                Button(action: {
                    windowManager.toggleAlwaysOnTop()
                }) {
                    HStack(spacing: 4) {
                        Image(systemName: windowManager.isAlwaysOnTop ? "pin.fill" : "pin")
                            .font(.system(size: 11, weight: .semibold))
                        if windowManager.isAlwaysOnTop {
                            Text("Pinned")
                                .font(.system(size: 10.5, weight: .semibold, design: .rounded))
                                .lineLimit(1)
                        }
                    }
                    .foregroundColor(windowManager.isAlwaysOnTop ? .blue : .secondary)
                    .padding(.horizontal, 7)
                    .padding(.vertical, 4)
                    .background(
                        windowManager.isAlwaysOnTop
                            ? Color.blue.opacity(0.2)
                            : Color.white.opacity(0.06)
                    )
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                    .overlay(
                        RoundedRectangle(cornerRadius: 6)
                            .stroke(
                                windowManager.isAlwaysOnTop
                                    ? Color.blue.opacity(0.4)
                                    : Color.white.opacity(0.18),
                                lineWidth: 0.5
                            )
                    )
                }
                .buttonStyle(.plain)
                .help(windowManager.isAlwaysOnTop ? "Unpin window" : "Keep window always on top")

                // Dock to Menu Bar button
                Button(action: {
                    windowManager.dockToMenuBar()
                }) {
                    HStack(spacing: 4) {
                        Image(systemName: "menubar.arrow.up.rectangle")
                            .font(.system(size: 11, weight: .semibold))
                        Text("Dock")
                            .font(.system(size: 10.5, weight: .medium, design: .rounded))
                            .lineLimit(1)
                    }
                    .foregroundColor(.primary.opacity(0.85))
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .background(Color.white.opacity(0.08))
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                    .overlay(
                        RoundedRectangle(cornerRadius: 6)
                            .stroke(Color.white.opacity(0.18), lineWidth: 0.5)
                    )
                }
                .buttonStyle(.plain)
                .help("Dock back into menu bar drop-down")

                if !client.isConnected {
                    Button("Start Daemon") {
                        client.launchDaemon()
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
                    .lineLimit(1)
                }
            }
        }
        .padding(.horizontal, 16)
        .padding(.top, 14)
        .padding(.bottom, 12)
        .background(
            LinearGradient(
                colors: [Color.white.opacity(0.08), Color.clear],
                startPoint: .top,
                endPoint: .bottom
            )
        )
    }

    // MARK: - In-Window Notification Alert Banner
    private func inWindowNotificationBanner(_ notification: InWindowNotification) -> some View {
        HStack(spacing: 10) {
            ZStack {
                Circle()
                    .fill(Color.orange.opacity(0.25))
                    .frame(width: 26, height: 26)
                Image(systemName: "exclamationmark.bubble.fill")
                    .font(.system(size: 12, weight: .bold))
                    .foregroundColor(.orange)
            }

            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(notification.sessionName)
                        .font(.system(size: 12, weight: .bold, design: .rounded))
                    Text("•")
                        .font(.caption2)
                        .foregroundColor(.secondary.opacity(0.6))
                    Text(notification.promptHeadline)
                        .font(.system(size: 11, weight: .semibold, design: .rounded))
                        .foregroundColor(.orange)
                }

                Text(notification.promptText)
                    .font(.system(size: 11, design: .rounded))
                    .foregroundColor(.secondary)
                    .lineLimit(1)
            }

            Spacer()

            Button(action: {
                withAnimation(.easeInOut(duration: 0.2)) {
                    windowManager.clearInWindowNotification()
                }
            }) {
                Image(systemName: "xmark")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundColor(.secondary)
                    .padding(5)
                    .background(Color.white.opacity(0.08))
                    .clipShape(Circle())
            }
            .buttonStyle(.plain)
            .help("Dismiss notification")
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .background(
            RoundedRectangle(cornerRadius: 10)
                .fill(Color.orange.opacity(0.12))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(Color.orange.opacity(0.35), lineWidth: 0.8)
        )
        .padding(.horizontal, 16)
        .padding(.vertical, 6)
        .transition(.asymmetric(
            insertion: .move(edge: .top).combined(with: .opacity),
            removal: .opacity
        ))
    }

    // MARK: - Footer
    private var glassFooter: some View {
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

            Text("aw run <command>")
                .font(.system(size: 10.5, design: .monospaced))
                .foregroundColor(.secondary.opacity(0.7))
                .padding(.horizontal, 8)
                .padding(.vertical, 3)
                .background(Color.black.opacity(0.12))
                .clipShape(RoundedRectangle(cornerRadius: 5))

            Spacer()

            Button("Close Window") {
                windowManager.closeDetachedWindow()
            }
            .buttonStyle(.plain)
            .font(.system(size: 11, design: .rounded))
            .foregroundColor(.secondary.opacity(0.8))
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(Color.white.opacity(0.03))
    }
}
