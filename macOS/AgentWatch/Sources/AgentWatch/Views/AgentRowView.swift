import SwiftUI

public struct AgentRowView: View {
    public let session: AgentSession
    public let onFocus: () -> Void
    public let onTerminate: () -> Void

    @State private var isOutputExpanded: Bool = false

    public var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 10) {
                // Pulsing Status Dot
                ZStack {
                    Circle()
                        .fill(session.statusColor.opacity(0.25))
                        .frame(width: 18, height: 18)
                    Circle()
                        .fill(session.statusColor)
                        .frame(width: 8, height: 8)
                }

                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 6) {
                        Text(session.name)
                            .font(.system(size: 13, weight: .bold, design: .rounded))

                        Text("•")
                            .font(.caption2)
                            .foregroundColor(.secondary.opacity(0.6))

                        Text(session.directoryName)
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundColor(.secondary)
                    }

                    Text(session.displayCommand)
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundColor(.secondary.opacity(0.8))
                        .lineLimit(1)
                }

                Spacer()

                // Status chip
                Text(session.statusDisplayName)
                    .font(.system(size: 10, weight: .semibold, design: .rounded))
                    .padding(.horizontal, 7)
                    .padding(.vertical, 3)
                    .background(session.statusColor.opacity(0.15))
                    .foregroundColor(session.statusColor)
                    .clipShape(Capsule())

                // Expand terminal logs button
                Button(action: { isOutputExpanded.toggle() }) {
                    Image(systemName: isOutputExpanded ? "chevron.up.circle.fill" : "chevron.down.circle")
                        .font(.system(size: 13))
                        .foregroundColor(.secondary)
                }
                .buttonStyle(.plain)

                // Jump to terminal
                Button(action: onFocus) {
                    Image(systemName: "arrow.up.forward.app")
                        .font(.system(size: 13))
                        .foregroundColor(.secondary)
                }
                .buttonStyle(.plain)
                .help("Focus Terminal")

                // Terminate
                if session.status == .running {
                    Button(action: onTerminate) {
                        Image(systemName: "stop.circle.fill")
                            .font(.system(size: 13))
                            .foregroundColor(.red.opacity(0.8))
                    }
                    .buttonStyle(.plain)
                    .help("Stop Agent")
                }
            }

            // Expanded Terminal View (Apple Glass dark console)
            if isOutputExpanded && !session.recentOutput.isEmpty {
                VStack(alignment: .leading, spacing: 3) {
                    ForEach(Array(session.recentOutput.suffix(8).enumerated()), id: \.offset) { _, line in
                        Text(line)
                            .font(.system(size: 10.5, design: .monospaced))
                            .foregroundColor(.green.opacity(0.9))
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
                .padding(10)
                .background(
                    RoundedRectangle(cornerRadius: 8)
                        .fill(Color.black.opacity(0.85))
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 8)
                        .stroke(Color.white.opacity(0.12), lineWidth: 0.5)
                )
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 9)
        .background(
            RoundedRectangle(cornerRadius: 11)
                .fill(Color(NSColor.controlBackgroundColor).opacity(0.45))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 11)
                .stroke(Color.white.opacity(0.15), lineWidth: 0.5)
        )
    }
}
