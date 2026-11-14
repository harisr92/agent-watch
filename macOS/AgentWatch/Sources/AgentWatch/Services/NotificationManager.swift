import Foundation
import UserNotifications
import AppKit

public class NotificationManager: NSObject, UNUserNotificationCenterDelegate {
    public static let shared = NotificationManager()
    public var onInputResponse: ((String, String) -> Void)?
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

        let promptCategory = UNNotificationCategory(
            identifier: "AGENT_PROMPT",
            actions: [approveAction, rejectAction],
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
            actions: [replyAction],
            intentIdentifiers: [],
            options: []
        )

        // Select, secret and acknowledge prompts have no inline actions: a menu can't fit in a
        // notification and a secret shouldn't be typed into one. Clicking opens the popover.
        UNUserNotificationCenter.current().setNotificationCategories([promptCategory, textCategory])
    }

    public func sendPromptNotification(session: AgentSession, prompt: PendingPrompt) {
        // Debounce: don't notify twice for the same prompt ID
        if lastNotifiedPromptId == prompt.id {
            return
        }
        lastNotifiedPromptId = prompt.id

        // 1. UNUserNotificationCenter for actionable notifications
        let content = UNMutableNotificationContent()
        content.title = "⚠️ Input Required: \(session.name)"
        content.subtitle = "Folder: \(session.directoryName)"
        content.body = prompt.promptText
        content.sound = .default
        switch prompt.promptType {
        case .confirm:
            content.categoryIdentifier = "AGENT_PROMPT"
        case .text:
            content.categoryIdentifier = "AGENT_TEXT_PROMPT"
        case .select:
            let labels = prompt.choices.prefix(4).map(\.label).joined(separator: " · ")
            if !labels.isEmpty {
                content.body = "\(prompt.promptText)\n\(labels)"
            }
        case .secret, .acknowledge:
            break
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
            title: "⚠️ Input Required: \(session.name)",
            subtitle: session.directoryName.isEmpty ? "Agent Watch" : session.directoryName,
            body: prompt.promptText
        )
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
