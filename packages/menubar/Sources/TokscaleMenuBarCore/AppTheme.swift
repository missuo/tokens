import Foundation

public enum AppTheme: String, CaseIterable, Sendable {
    case amber
    case ocean
    case forest
    case mono

    public static let `default`: AppTheme = .amber
    public static let storageKey = "appTheme"

    public init(storedValue: String?) {
        guard let storedValue, let parsed = AppTheme(rawValue: storedValue) else {
            self = .default
            return
        }
        self = parsed
    }

    public var title: String {
        switch self {
        case .amber:
            return "Amber"
        case .ocean:
            return "Ocean"
        case .forest:
            return "Forest"
        case .mono:
            return "Mono"
        }
    }

    /// Accent as HSB components so the core stays free of SwiftUI; the app maps it to a Color.
    public var accentHSB: (hue: Double, saturation: Double, brightness: Double) {
        switch self {
        case .amber:
            return (0.065, 0.96, 0.98)
        case .ocean:
            return (0.58, 0.82, 0.94)
        case .forest:
            return (0.38, 0.66, 0.78)
        case .mono:
            return (0.08, 0.05, 0.58)
        }
    }
}
