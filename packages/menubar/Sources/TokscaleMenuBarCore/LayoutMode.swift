import Foundation

/// Popover layout: a single scrolling dashboard (everything at once) or a paged
/// view — page 1 a ClaudeBar-style large-type quota glance, page 2 the history
/// and activity detail.
public enum LayoutMode: String, CaseIterable, Sendable {
    case single
    case paged

    public static let `default`: LayoutMode = .paged
    public static let storageKey = "popoverLayoutMode"

    public init(storedValue: String?) {
        guard let storedValue, let parsed = LayoutMode(rawValue: storedValue) else {
            self = .default
            return
        }
        self = parsed
    }

    public var title: String {
        switch self {
        case .single:
            return "Single"
        case .paged:
            return "Paged"
        }
    }
}
