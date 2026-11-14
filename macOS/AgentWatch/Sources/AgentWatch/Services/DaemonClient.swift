import Foundation
import Combine
import AppKit

@MainActor
public class DaemonClient: ObservableObject {
    public static let shared = DaemonClient()

    @Published public var sessions: [AgentSession] = []
    @Published public var isConnected: Bool = false
    @Published public var lastError: String?

    private var webSocketTask: URLSessionWebSocketTask?
    private let urlSession = URLSession(configuration: .default)
    private var reconnectTimer: Timer?
    private var daemonProcess: Process?

    public let daemonHost = "127.0.0.1"
    public let daemonPort = 45123

    public var apiBaseUrl: String {
        "http://\(daemonHost):\(daemonPort)"
    }

    public var wsUrl: String {
        "ws://\(daemonHost):\(daemonPort)/api/v1/events"
    }

    public var waitingSessions: [AgentSession] {
        sessions.filter { $0.status == .waitingForInput }
    }

    public var activeSessions: [AgentSession] {
        sessions.filter { $0.status == .running }
    }

    public var recentSessions: [AgentSession] {
        sessions.filter { $0.status == .completed || $0.status == .failed || $0.status == .idle }
    }

    public var summarySubtitle: String {
        let waiting = waitingSessions.count
        let active = activeSessions.count
        if waiting > 0 {
            return "\(waiting) waiting for input • \(active) running"
        } else if active > 0 {
            return "\(active) active agent\(active == 1 ? "" : "s")"
        } else {
            return "Ready to monitor agents"
        }
    }

    public init() {
        NotificationManager.shared.onInputResponse = { [weak self] sessionId, input in
            Task { @MainActor in
                self?.submitInput(sessionId: sessionId, input: input)
            }
        }
    }

    public func start() {
        connectWebSocket()
        fetchSessions()
        startHeartbeat()

        // Auto-launch daemon if not already running
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) { [weak self] in
            if !(self?.isConnected ?? false) {
                self?.launchDaemon()
            }
        }
    }

    public func connectWebSocket() {
        guard let url = URL(string: wsUrl) else { return }

        webSocketTask?.cancel(with: .goingAway, reason: nil)
        webSocketTask = urlSession.webSocketTask(with: url)
        webSocketTask?.resume()

        listenWebSocket()
    }

    private func listenWebSocket() {
        webSocketTask?.receive { [weak self] result in
            Task { @MainActor in
                guard let self = self else { return }

                switch result {
                case .success(let message):
                    self.isConnected = true
                    self.lastError = nil
                    switch message {
                    case .string(let text):
                        self.handleIncomingEvent(text: text)
                    case .data(let data):
                        if let text = String(data: data, encoding: .utf8) {
                            self.handleIncomingEvent(text: text)
                        }
                    @unknown default:
                        break
                    }
                    self.listenWebSocket() // Continue listening

                case .failure(let error):
                    self.isConnected = false
                    self.lastError = error.localizedDescription
                    self.scheduleReconnect()
                }
            }
        }
    }

    private func handleIncomingEvent(text: String) {
        guard let data = text.data(using: .utf8) else { return }

        // 1. Initial snapshot check
        if let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
           let type = json["type"] as? String {

            if type == "init",
               let sessionsData = try? JSONSerialization.data(withJSONObject: json["sessions"] ?? []),
               let initialSessions = try? JSONDecoder().decode([AgentSession].self, from: sessionsData) {
                self.sessions = initialSessions
                StatusBarController.shared.updateStatusItem()
                if self.waitingSessions.count > 0 {
                    StatusBarController.shared.showPopover()
                }
                return
            }

            if type == "session_registered" || type == "session_updated",
               let payload = json["payload"],
               let payloadData = try? JSONSerialization.data(withJSONObject: payload),
               let session = try? JSONDecoder().decode(AgentSession.self, from: payloadData) {
                updateOrInsertSession(session)
                return
            }

            if type == "prompt_required",
               let payload = json["payload"] as? [String: Any],
               let sessionId = payload["session_id"] as? String,
               let promptObj = payload["prompt"],
               let promptData = try? JSONSerialization.data(withJSONObject: promptObj),
               let prompt = try? JSONDecoder().decode(PendingPrompt.self, from: promptData) {
                if let idx = sessions.firstIndex(where: { $0.id == sessionId }) {
                    sessions[idx].status = .waitingForInput
                    sessions[idx].pendingPrompt = prompt
                    NotificationManager.shared.sendPromptNotification(session: sessions[idx], prompt: prompt)
                } else {
                    let fallbackSession = AgentSession(
                        id: sessionId,
                        name: "Agent",
                        command: "",
                        args: [],
                        cwd: FileManager.default.currentDirectoryPath,
                        pid: nil,
                        status: .waitingForInput,
                        pendingPrompt: prompt,
                        startedAt: "",
                        updatedAt: "",
                        recentOutput: []
                    )
                    sessions.insert(fallbackSession, at: 0)
                    NotificationManager.shared.sendPromptNotification(session: fallbackSession, prompt: prompt)
                }
                StatusBarController.shared.updateStatusItem()
                StatusBarController.shared.showPopover()
                return
            }

            if type == "prompt_resolved",
               let payload = json["payload"] as? [String: Any],
               let sessionId = payload["session_id"] as? String {
                if let idx = sessions.firstIndex(where: { $0.id == sessionId }) {
                    sessions[idx].status = .running
                    sessions[idx].pendingPrompt = nil
                }
                StatusBarController.shared.updateStatusItem()
                return
            }

            if type == "session_terminated",
               let payload = json["payload"] as? [String: Any],
               let sessionId = payload["session_id"] as? String {
                let exitCode = payload["exit_code"] as? Int
                if let idx = sessions.firstIndex(where: { $0.id == sessionId }) {
                    sessions[idx].status = (exitCode == 0) ? .completed : .failed
                    sessions[idx].pendingPrompt = nil
                }
                StatusBarController.shared.updateStatusItem()
                return
            }
        }
    }

    private func updateOrInsertSession(_ session: AgentSession) {
        let oldStatus = sessions.first(where: { $0.id == session.id })?.status
        if let index = sessions.firstIndex(where: { $0.id == session.id }) {
            sessions[index] = session
        } else {
            sessions.insert(session, at: 0)
        }

        StatusBarController.shared.updateStatusItem()

        if session.status == .waitingForInput,
           let prompt = session.pendingPrompt,
           oldStatus != .waitingForInput {
            StatusBarController.shared.showPopover()
            NotificationManager.shared.sendPromptNotification(session: session, prompt: prompt)
        }
    }

    public func fetchSessions() {
        guard let url = URL(string: "\(apiBaseUrl)/api/v1/sessions") else { return }

        Task {
            do {
                let (data, response) = try await URLSession.shared.data(from: url)
                if let httpResp = response as? HTTPURLResponse, httpResp.statusCode == 200 {
                    let decoded = try JSONDecoder().decode([AgentSession].self, from: data)
                    self.sessions = decoded
                    self.isConnected = true
                }
            } catch {
                self.isConnected = false
            }
        }
    }

    public func submitInput(sessionId: String, input: String, pressEnter: Bool = true) {
        guard let url = URL(string: "\(apiBaseUrl)/api/v1/sessions/\(sessionId)/input") else { return }

        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")

        let payload = UserInputPayload(input: input, pressEnter: pressEnter)
        request.httpBody = try? JSONEncoder().encode(payload)

        // Optimistically update local state
        if let idx = sessions.firstIndex(where: { $0.id == sessionId }) {
            sessions[idx].status = .running
            sessions[idx].pendingPrompt = nil
        }

        Task {
            do {
                _ = try await URLSession.shared.data(for: request)
            } catch {
                print("Failed to send input: \(error)")
            }
        }
    }

    public func terminateSession(sessionId: String) {
        guard let url = URL(string: "\(apiBaseUrl)/api/v1/sessions/\(sessionId)/terminate") else { return }

        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try? JSONSerialization.data(withJSONObject: ["exit_code": 1])

        Task {
            _ = try? await URLSession.shared.data(for: request)
        }
    }

    public func focusTerminal(for session: AgentSession) {
        // Bring Terminal or iTerm to the front using AppleScript
        let script = """
        tell application "System Events"
            set termApps to {"Ghostty", "iTerm2", "Terminal", "Alacritty", "kitty", "Cursor", "Code"}
            repeat with appName in termApps
                if exists (process appName) then
                    tell process appName to set frontmost to true
                    exit repeat
                end if
            end repeat
        end tell
        """

        if let appleScript = NSAppleScript(source: script) {
            var errorInfo: NSDictionary?
            appleScript.executeAndReturnError(&errorInfo)
        }
    }

    public func launchDaemon() {
        guard daemonProcess == nil else { return }

        let candidates: [String?] = [
            Bundle.main.executableURL?.deletingLastPathComponent().appendingPathComponent("aw").path,
            Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/aw").path,
            "/usr/local/bin/aw",
            "\(FileManager.default.homeDirectoryForCurrentUser.path)/.cargo/bin/aw",
            "\(FileManager.default.homeDirectoryForCurrentUser.path)/dev/work/agent-watch/target/release/aw"
        ]

        var execPath: String?
        for path in candidates {
            if let p = path, FileManager.default.isExecutableFile(atPath: p) {
                execPath = p
                break
            }
        }

        guard let validPath = execPath else {
            print("Could not find 'aw' binary in bundle or PATH.")
            return
        }

        let process = Process()
        process.executableURL = URL(fileURLWithPath: validPath)
        process.arguments = ["daemon"]

        do {
            try process.run()
            self.daemonProcess = process
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.8) {
                self.connectWebSocket()
                self.fetchSessions()
            }
        } catch {
            print("Failed to auto-launch daemon: \(error)")
        }
    }

    private func scheduleReconnect() {
        reconnectTimer?.invalidate()
        reconnectTimer = Timer.scheduledTimer(withTimeInterval: 3.0, repeats: false) { [weak self] _ in
            Task { @MainActor in
                self?.connectWebSocket()
            }
        }
    }

    private func startHeartbeat() {
        Timer.scheduledTimer(withTimeInterval: 5.0, repeats: true) { [weak self] _ in
            Task { @MainActor in
                if !(self?.isConnected ?? false) {
                    self?.fetchSessions()
                }
            }
        }
    }
}
