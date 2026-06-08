import Foundation

public enum AutoRefresh: String, CaseIterable, Sendable {
    case off
    case everyFiveMinutes
    case everyFifteenMinutes

    public static let `default`: AutoRefresh = .off
    public static let storageKey = "autoRefresh"

    public init(storedValue: String?) {
        guard let storedValue, let parsed = AutoRefresh(rawValue: storedValue) else {
            self = .default
            return
        }
        self = parsed
    }

    public var interval: TimeInterval? {
        switch self {
        case .off:
            return nil
        case .everyFiveMinutes:
            return 300
        case .everyFifteenMinutes:
            return 900
        }
    }

    public var title: String {
        switch self {
        case .off:
            return "Off"
        case .everyFiveMinutes:
            return "5 min"
        case .everyFifteenMinutes:
            return "15 min"
        }
    }
}
