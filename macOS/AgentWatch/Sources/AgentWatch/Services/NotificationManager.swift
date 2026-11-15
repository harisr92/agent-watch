import Foundation
import UserNotifications
import AppKit

public class NotificationManager: NSObject, UNUserNotificationCenterDelegate {
    public static let shared = NotificationManager()
    public var onInputResponse: ((String, String) -> Void)?
    /// The user chose "Dismiss": hide the prompt without answering the agent.
    public var onDismissResponse: ((String) -> Void)?
    private var lastNotifiedPromptId: String?

    private var isAppBundle: Bool {
        Bundle.main.bundleURL.pathExtension == "app"
    }

    private override init() {
        super.init()
        guard isAppBundle else { return }
        UNUserNotificationCenter.current().delegate = self
        setupCategories()
    }

    public func requestAuthorization() {
        guard isAppBundle else { return }
        let options: UNAuthorizationOptions = [.alert, .sound, .badge]
        UNUserNotificationCenter.current().requestAuthorization(options: options) { granted, error in
            if let error = error {
                print("Notification auth error: \(error)")
            }
        }
    }

    private func setupCategories() {
        let approveAction = UNNotificationAction(
            identifier: "APPROVE_ACTION",
            title: "Approve (y)",
            options: [.foreground]
        )
        let rejectAction = UNNotificationAction(
            identifier: "REJECT_ACTION",
            title: "Reject (n)",
            options: [.destructive]
        )

        let dismissAction = UNNotificationAction(
            identifier: "DISMISS_ACTION",
            title: "Dismiss",
            options: []
        )

        let promptCategory = UNNotificationCategory(
            identifier: "AGENT_PROMPT",
            actions: [approveAction, rejectAction, dismissAction],
            intentIdentifiers: [],
            options: []
        )

        let replyAction = UNTextInputNotificationAction(
            identifier: "REPLY_ACTION",
            title: "Reply",
            options: [],
            textInputButtonTitle: "Send",
            textInputPlaceholder: "Type your reply..."
        )
        let textCategory = UNNotificationCategory(
            identifier: "AGENT_TEXT_PROMPT",
            actions: [replyAction, dismissAction],
            intentIdentifiers: [],
            options: []
        )
        let dismissOnlyCategory = UNNotificationCategory(
            identifier: "AGENT_DISMISSABLE_PROMPT",
            actions: [dismissAction],
            intentIdentifiers: [],
            options: []
        )

        // Select, secret and acknowledge prompts only get "Dismiss": a menu can't fit in a
        // notification and a secret shouldn't be typed into one. Clicking opens the popover.
        UNUserNotificationCenter.current().setNotificationCategories(
            [promptCategory, textCategory, dismissOnlyCategory]
        )
    }

    public func sendPromptNotification(session: AgentSession, prompt: PendingPrompt) {
        // Debounce: don't notify twice for the same prompt ID
        if lastNotifiedPromptId == prompt.id {
            return
        }
        lastNotifiedPromptId = prompt.id

        // 1. UNUserNotificationCenter for actionable notifications
        let content = UNMutableNotificationContent()
        content.title = "⏳ Waiting: \(session.name)"
        content.subtitle = subtitle(for: session)
        content.body = prompt.promptText
        content.sound = .default
        switch prompt.promptType {
        case .confirm:
            content.categoryIdentifier = "AGENT_PROMPT"
        case .text:
            content.categoryIdentifier = "AGENT_TEXT_PROMPT"
        case .select:
            content.categoryIdentifier = "AGENT_DISMISSABLE_PROMPT"
            let labels = prompt.choices.prefix(4).map(\.label).joined(separator: " · ")
            if !labels.isEmpty {
                content.body = "\(prompt.promptText)\n\(labels)"
            }
        case .secret, .acknowledge:
            content.categoryIdentifier = "AGENT_DISMISSABLE_PROMPT"
        }
        content.userInfo = [
            "session_id": session.id,
            "prompt_id": prompt.id
        ]

        let trigger = UNTimeIntervalNotificationTrigger(timeInterval: 0.1, repeats: false)
        let request = UNNotificationRequest(
            identifier: "prompt-\(prompt.id)",
            content: content,
            trigger: trigger
        )
        if isAppBundle {
            UNUserNotificationCenter.current().add(request)
        }

        // 2. Guaranteed native macOS banner via AppleScript
        sendSystemBanner(
            title: "⏳ Waiting: \(session.name)",
            subtitle: subtitle(for: session),
            body: prompt.promptText
        )

        // 3. Play immediate audio alert
        if NSSound(named: "Ping")?.play() != true {
            NSSound.beep()
        }
    }

    /// The agent finished its turn and waits for the next message. Keyed per turn by the
    /// session's update time, so it is independent of the prompt-id debounce above.
    public func sendDoneNotification(session: AgentSession) {
        let title = "✅ Done: \(session.name)"
        let body = session.taskTitle ?? "Finished. Waiting for your next message."

        let content = UNMutableNotificationContent()
        content.title = title
        content.subtitle = subtitle(for: session, includeTask: false)
        content.body = body
        content.sound = .default
        content.userInfo = ["session_id": session.id]
        let request = UNNotificationRequest(
            identifier: "done-\(session.id)-\(session.updatedAt)",
            content: content,
            trigger: UNTimeIntervalNotificationTrigger(timeInterval: 0.1, repeats: false)
        )
        if isAppBundle {
            UNUserNotificationCenter.current().add(request)
        }
        sendSystemBanner(title: title, subtitle: subtitle(for: session, includeTask: false), body: body)
    }

    /// "Fix login bug · my-app": the task the agent reports, then the folder.
    private func subtitle(for session: AgentSession, includeTask: Bool = true) -> String {
        let task = includeTask ? session.taskTitle : nil
        let parts = [task, session.directoryName].compactMap { $0 }.filter { !$0.isEmpty }
        return parts.isEmpty ? "Agent Watch" : parts.joined(separator: " · ")
    }

    /// Removes a prompt's notification from Notification Center once it is no longer pending.
    public func removePromptNotification(promptId: String) {
        guard isAppBundle else { return }
        UNUserNotificationCenter.current().removeDeliveredNotifications(withIdentifiers: ["prompt-\(promptId)"])
    }

    private func sendSystemBanner(title: String, subtitle: String, body: String) {
        let cleanTitle = title.replacingOccurrences(of: "\"", with: "\\\"")
        let cleanSubtitle = subtitle.replacingOccurrences(of: "\"", with: "\\\"")
        let cleanBody = body.replacingOccurrences(of: "\"", with: "\\\"").replacingOccurrences(of: "\n", with: " ")
        
        let script = "display notification \"\(cleanBody)\" with title \"\(cleanTitle)\" subtitle \"\(cleanSubtitle)\" sound name \"Ping\""
        
        DispatchQueue.global(qos: .userInitiated).async {
            if let appleScript = NSAppleScript(source: script) {
                var error: NSDictionary?
                appleScript.executeAndReturnError(&error)
            }
        }
    }

    public func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        let userInfo = response.notification.request.content.userInfo
        if let sessionId = userInfo["session_id"] as? String {
            switch response.actionIdentifier {
            case "APPROVE_ACTION":
                onInputResponse?(sessionId, "y")
            case "REJECT_ACTION":
                onInputResponse?(sessionId, "n")
            case "DISMISS_ACTION":
                onDismissResponse?(sessionId)
            case "REPLY_ACTION":
                if let reply = (response as? UNTextInputNotificationResponse)?.userText {
                    onInputResponse?(sessionId, reply)
                }
            case UNNotificationDefaultActionIdentifier:
                Task { @MainActor in
                    if WindowManager.shared.isWindowOpen {
                        WindowManager.shared.bringToFront()
                    } else {
                        StatusBarController.shared.showPopover()
                    }
                }
            default:
                break
            }
        }
        completionHandler()
    }

    public func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        completionHandler([.banner, .sound, .badge])
    }
}
