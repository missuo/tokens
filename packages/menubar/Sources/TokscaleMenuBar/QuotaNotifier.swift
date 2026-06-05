import TokscaleMenuBarCore
import UserNotifications

final class QuotaNotifier {
    init() {
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound]) { _, _ in }
    }

    func notify(_ alert: QuotaAlert) {
        let isCritical = alert.level == .critical || alert.level == .depleted
        let content = UNMutableNotificationContent()
        content.title = "\(alert.provider) quota \(isCritical ? "critical" : "low")"
        content.body = "\(alert.windowLabel) window at \(Int(alert.remainingPercent.rounded()))% remaining"
        content.sound = .default
        let request = UNNotificationRequest(
            identifier: "quota-\(alert.provider)-\(alert.windowLabel)-\(UUID().uuidString)",
            content: content,
            trigger: nil
        )
        UNUserNotificationCenter.current().add(request)
    }
}
