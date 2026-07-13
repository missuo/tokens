import Foundation

public enum BackgroundScanAction: Equatable, Sendable {
    case full
    case today
    case none
}

public enum BackgroundScanPolicy {
    public static let historyRefreshInterval: TimeInterval = 86_400
    public static let historyRetryInterval: TimeInterval = 21_600
    public static let todayRefreshInterval: TimeInterval = 600
    public static let quotaOpenRefreshInterval: TimeInterval = 120

    public static func nextAction(
        now: Date,
        lastHistorySuccess: Date,
        lastHistoryAttempt: Date,
        lastTodayScan: Date
    ) -> BackgroundScanAction {
        let historyDue = now.timeIntervalSince(lastHistorySuccess) >= historyRefreshInterval
        let historyRetryDue = now.timeIntervalSince(lastHistoryAttempt) >= historyRetryInterval
        if historyDue, historyRetryDue {
            return .full
        }
        if now.timeIntervalSince(lastTodayScan) >= todayRefreshInterval {
            return .today
        }
        return .none
    }

    public static func restoredScanDates(
        generatedAt: String,
        historyGeneratedAt: String?
    ) -> (today: Date, history: Date)? {
        guard let today = parseISODate(generatedAt) else { return nil }
        let history = historyGeneratedAt.flatMap(parseISODate) ?? today
        return (today, history)
    }

    public static func shouldRefreshQuotaOnOpen(
        needsRefresh: Bool,
        isRefreshing: Bool,
        now: Date,
        lastAttempt: Date
    ) -> Bool {
        needsRefresh && !isRefreshing
            && now.timeIntervalSince(lastAttempt) >= quotaOpenRefreshInterval
    }

    public static func actionAfterRemoteHistorySync(succeeded: Bool) -> BackgroundScanAction {
        succeeded ? .today : .full
    }

    private static func parseISODate(_ value: String) -> Date? {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let date = formatter.date(from: value) { return date }
        formatter.formatOptions = [.withInternetDateTime]
        return formatter.date(from: value)
    }
}
