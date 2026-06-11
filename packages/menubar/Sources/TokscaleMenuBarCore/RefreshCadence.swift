import Foundation

public enum RefreshCadence: String, CaseIterable, Sendable {
    case off
    case everyMinute
    case everyFiveMinutes

    public static let `default`: RefreshCadence = .everyMinute
    public static let storageKey = "openRefreshCadence"

    public init(storedValue: String?) {
        guard let storedValue, let parsed = RefreshCadence(rawValue: storedValue) else {
            self = .default
            return
        }
        self = parsed
    }

    public var minimumInterval: TimeInterval? {
        switch self {
        case .off:
            return nil
        case .everyMinute:
            return 60
        case .everyFiveMinutes:
            return 300
        }
    }

    public var title: String {
        switch self {
        case .off:
            return "Off"
        case .everyMinute:
            return "1 min"
        case .everyFiveMinutes:
            return "5 min"
        }
    }
}

public struct OpenRefreshScanPolicy: Sendable {
    public enum Scan: Equatable, Sendable {
        case none
        case today
    }

    public init() {}

    public func scan(
        summaryIsMissing: Bool,
        needsUsageScan: Bool,
        isBackgroundScanning: Bool,
        lastTodayScan: Date,
        now: Date = Date()
    ) -> Scan {
        if summaryIsMissing || !needsUsageScan || isBackgroundScanning {
            return .none
        }
        if now.timeIntervalSince(lastTodayScan) <= 600 {
            return .none
        }
        return .today
    }
}
