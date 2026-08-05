import Foundation

public enum UsagePeriod: String, CaseIterable, Identifiable, Codable {
    case today
    case days7 = "7d"
    case days30 = "30d"
    case all

    public var id: String { rawValue }

    public var cliValue: String { rawValue }

    public var title: String {
        switch self {
        case .today: return "Today"
        case .days7: return "7d"
        case .days30: return "30d"
        case .all: return "All"
        }
    }

    /// Uppercase tab labels for Minimal Mono period underlines.
    public var monoTitle: String {
        switch self {
        case .today: return "TODAY"
        case .days7: return "7D"
        case .days30: return "30D"
        case .all: return "ALL"
        }
    }
}

public enum MenuBarDisplayMode: String, CaseIterable, Identifiable, Codable {
    case tokens
    case cost
    case both

    public var id: String { rawValue }

    public var title: String {
        switch self {
        case .tokens: return "Tokens"
        case .cost: return "Cost"
        case .both: return "Both"
        }
    }
}

/// Background local rescan cadence for Settings → SCANNING.
/// Presets cover common cases; `custom(minutes:)` covers 5m…24h without a long menu.
public enum ScanIntervalOption: Equatable, Identifiable, Hashable {
    case fifteenMinutes
    case oneHour
    case sixHours
    case twelveHours
    /// Manual only (UI chip: OFF).
    case manual
    /// Clamped to 5…1440 minutes.
    case custom(minutes: Int)

    public var id: String { storageKey }

    /// Chip strip order (Custom is a mode, not a fixed duration).
    public static let chips: [ScanIntervalChip] = [
        .fifteenMinutes, .oneHour, .sixHours, .twelveHours, .off, .custom,
    ]

    public var isCustom: Bool {
        if case .custom = self { return true }
        return false
    }

    public var isManual: Bool {
        if case .manual = self { return true }
        return false
    }

    /// Minutes when custom; otherwise a sensible default for entering Custom.
    public var customMinutesOrDefault: Int {
        if case .custom(let minutes) = self {
            return Self.clampMinutes(minutes)
        }
        return 30
    }

    public var title: String {
        switch self {
        case .fifteenMinutes: return "15 min"
        case .oneHour: return "1 hour"
        case .sixHours: return "6 hours"
        case .twelveHours: return "12 hours"
        case .manual: return "Manual only"
        case .custom(let minutes):
            let m = Self.clampMinutes(minutes)
            if m % 60 == 0 {
                let h = m / 60
                return h == 1 ? "1 hour" : "\(h) hours"
            }
            return "\(m) min"
        }
    }

    public var timeInterval: TimeInterval? {
        switch self {
        case .fifteenMinutes: return 15 * 60
        case .oneHour: return 3600
        case .sixHours: return 6 * 3600
        case .twelveHours: return 12 * 3600
        case .manual: return nil
        case .custom(let minutes):
            return TimeInterval(Self.clampMinutes(minutes) * 60)
        }
    }

    /// UserDefaults / migration key.
    public var storageKey: String {
        switch self {
        case .fifteenMinutes: return "15m"
        case .oneHour: return "1h"
        case .sixHours: return "6h"
        case .twelveHours: return "12h"
        case .manual: return "manual"
        case .custom(let minutes):
            return "custom:\(Self.clampMinutes(minutes))"
        }
    }

    public static func fromStorage(_ raw: String?) -> ScanIntervalOption {
        guard let raw, !raw.isEmpty else { return .twelveHours }
        switch raw {
        case "15m": return .fifteenMinutes
        case "1h": return .oneHour
        case "6h": return .sixHours
        case "12h": return .twelveHours
        case "manual": return .manual
        // Legacy fixed 24h → custom 24h (still valid duration).
        case "24h": return .custom(minutes: 24 * 60)
        default:
            if raw.hasPrefix("custom:") {
                let body = raw.dropFirst("custom:".count)
                if let minutes = Int(body) {
                    return .custom(minutes: clampMinutes(minutes))
                }
            }
            return .twelveHours
        }
    }

    public static func clampMinutes(_ minutes: Int) -> Int {
        min(max(minutes, minimumCustomMinutes), maximumCustomMinutes)
    }

    public static let minimumCustomMinutes = 5
    public static let maximumCustomMinutes = 24 * 60

    /// Minute-unit ladder for Custom ±.
    public static let minuteSteps = [5, 15, 30, 45, 60]
    /// Hour-unit ladder for Custom ± (stored as minutes).
    public static let hourStepsMinutes = [60, 120, 180, 360, 720, 1440]

    /// Next/previous custom duration on the unit ladder.
    public static func steppedCustomMinutes(
        from minutes: Int,
        unit: ScanIntervalCustomUnit,
        direction: Int
    ) -> Int {
        let ladder = unit == .minutes ? minuteSteps : hourStepsMinutes
        let current = clampMinutes(minutes)
        // Nearest index on ladder.
        var best = 0
        var bestDist = Int.max
        for (i, step) in ladder.enumerated() {
            let d = abs(step - current)
            if d < bestDist {
                bestDist = d
                best = i
            }
        }
        let next = best + (direction >= 0 ? 1 : -1)
        if next < 0 { return ladder[0] }
        if next >= ladder.count { return ladder[ladder.count - 1] }
        return ladder[next]
    }
}

/// Settings chip strip selection.
public enum ScanIntervalChip: String, CaseIterable, Identifiable {
    case fifteenMinutes = "15m"
    case oneHour = "1h"
    case sixHours = "6h"
    case twelveHours = "12h"
    case off = "off"
    case custom = "custom"

    public var id: String { rawValue }

    public var monoTitle: String {
        switch self {
        case .fifteenMinutes: return "15M"
        case .oneHour: return "1H"
        case .sixHours: return "6H"
        case .twelveHours: return "12H"
        case .off: return "OFF"
        case .custom: return "CUSTOM"
        }
    }

    public func matches(_ option: ScanIntervalOption) -> Bool {
        switch (self, option) {
        case (.fifteenMinutes, .fifteenMinutes): return true
        case (.oneHour, .oneHour): return true
        case (.sixHours, .sixHours): return true
        case (.twelveHours, .twelveHours): return true
        case (.off, .manual): return true
        case (.custom, .custom): return true
        default: return false
        }
    }
}

public enum ScanIntervalCustomUnit: String, Equatable {
    case minutes
    case hours

    /// Prefer hours when the value is a whole hour ≥ 1h.
    public static func preferred(forMinutes minutes: Int) -> ScanIntervalCustomUnit {
        let m = ScanIntervalOption.clampMinutes(minutes)
        return (m >= 60 && m % 60 == 0) ? .hours : .minutes
    }
}

public struct DateSelectionRange: Codable, Equatable, Hashable {
    public let startDate: String
    public let endDate: String

    public init(startDate: String, endDate: String) {
        self.startDate = startDate
        self.endDate = endDate
    }

    public var isOrdered: Bool { startDate <= endDate }
}

public enum UsageSelection: Equatable, Hashable, Codable {
    case preset(UsagePeriod)
    case custom(DateSelectionRange)

    public var preset: UsagePeriod? {
        if case .preset(let period) = self { return period }
        return nil
    }

    public var customRange: DateSelectionRange? {
        if case .custom(let range) = self { return range }
        return nil
    }

    private enum CodingKeys: String, CodingKey {
        case kind
        case preset
        case startDate
        case endDate
    }

    private enum Kind: String, Codable {
        case preset
        case custom
    }

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        switch try values.decode(Kind.self, forKey: .kind) {
        case .preset:
            self = .preset(try values.decode(UsagePeriod.self, forKey: .preset))
        case .custom:
            self = .custom(
                DateSelectionRange(
                    startDate: try values.decode(String.self, forKey: .startDate),
                    endDate: try values.decode(String.self, forKey: .endDate)
                )
            )
        }
    }

    public func encode(to encoder: Encoder) throws {
        var values = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .preset(let period):
            try values.encode(Kind.preset, forKey: .kind)
            try values.encode(period, forKey: .preset)
        case .custom(let range):
            try values.encode(Kind.custom, forKey: .kind)
            try values.encode(range.startDate, forKey: .startDate)
            try values.encode(range.endDate, forKey: .endDate)
        }
    }
}

public struct UsageReport: Codable, Equatable {
    public let schemaVersion: Int
    public let generatedAt: String
    public let selection: UsageSelection
    public let dateRange: UsageDateRange
    public let scan: ScanInfo
    public let summary: UsageSummary
    public let tokenBreakdown: TokenBreakdown
    public let byClient: [ClientUsage]
    public let byProject: [ProjectUsage]
    public let byModel: [ModelUsage]
    public let timeSeries: UsageTimeSeries
    public let meta: UsageMeta
}

public struct UsageDateRange: Codable, Equatable {
    public let startDate: String
    public let endDate: String
    public let timezone: String
}

public struct ScanInfo: Codable, Equatable {
    public let mode: String
    public let forceRescan: Bool
    public let durationMs: UInt32
    public let cache: ScanCacheInfo
}

public struct ScanCacheInfo: Codable, Equatable {
    public let sourceHits: UInt64
    public let sourceMisses: UInt64
    public let snapshotRebuilt: Bool
    public let snapshotSchemaVersion: UInt32
}

public struct UsageSummary: Codable, Equatable {
    public let totalTokens: Int64
    public let totalCost: Double
    public let messages: Int32
    public let activeDays: Int32
    public let clients: [String]
    public let models: [String]
}

public struct TokenBreakdown: Codable, Equatable {
    public let input: Int64
    public let output: Int64
    public let cacheRead: Int64
    public let cacheWrite: Int64
    public let reasoning: Int64
}

public struct ClientUsage: Codable, Equatable, Identifiable {
    public var id: String { client }
    public let client: String
    public let tokens: Int64
    public let cost: Double
    public let messages: Int32
    public let share: Double
    public let models: [ClientModelUsage]
}

public struct ClientModelUsage: Codable, Equatable, Identifiable {
    public var id: String { "\(providerId)/\(modelId)" }
    public let modelId: String
    public let providerId: String
    public let tokens: Int64
    public let cost: Double
    public let messages: Int32
}

public struct ProjectUsage: Codable, Equatable, Identifiable {
    public var id: String { projectKey ?? "__unattributed__" }
    public let projectKey: String?
    public let displayName: String
    public let tokens: Int64
    public let cost: Double
    public let messages: Int32
    public let models: [ProjectModelUsage]
}

public struct ProjectModelUsage: Codable, Equatable, Identifiable {
    public var id: String { "\(providerId)/\(modelId)" }
    public let modelId: String
    public let providerId: String
    public let tokens: Int64
    public let cost: Double
    public let messages: Int32
}

public struct ModelUsage: Codable, Equatable, Identifiable {
    public var id: String { "\(providerId)/\(modelId)" }
    public let modelId: String
    public let providerId: String
    public let tokens: Int64
    public let cost: Double
    public let messages: Int32
    public let share: Double
    public let clients: [String]
}

public enum UsageTimeGranularity: String, Codable, Equatable {
    case hour
    case day
    case naturalWeek
    case naturalMonth

    public var title: String {
        switch self {
        case .hour: return "Hourly"
        case .day: return "Daily"
        case .naturalWeek: return "Weekly"
        case .naturalMonth: return "Monthly"
        }
    }
}

public struct UsageTotals: Codable, Equatable {
    public let tokens: Int64
    public let cost: Double
    public let messages: Int32
}

public struct UsageTimeBucket: Codable, Equatable, Identifiable {
    public let id: String
    public let nominalStart: String
    public let nominalEndExclusive: String
    public let coveredStart: String
    public let coveredEndExclusive: String
    public let totals: UsageTotals
    public let contextOnly: Bool
    public let incompleteEdge: Bool
    public let active: Bool
}

public struct UsageTimeSeries: Codable, Equatable {
    public let granularity: UsageTimeGranularity
    public let selectionStart: String
    public let buckets: [UsageTimeBucket]
    public let unplaced: UsageTotals
}

public struct UsageMeta: Codable, Equatable {
    public let cliVersion: String
    public let timezone: String
    public let reportContract: String
}

public struct UsageErrorReport: Codable {
    public let schemaVersion: Int
    public let error: UsageErrorBody
}

public struct UsageErrorBody: Codable {
    public let code: String
    public let message: String
}

public enum UsageServiceError: LocalizedError, Equatable {
    case binaryNotFound
    case invalidJSON(String)
    case commandFailed(code: Int32, message: String)
    case timeout

    public var errorDescription: String? {
        switch self {
        case .binaryNotFound:
            return "tokens CLI not found. Install with: brew install owo-network/brew/tokens"
        case .invalidJSON(let detail):
            return "Could not parse tokens usage JSON: \(detail)"
        case .commandFailed(_, let message):
            return message
        case .timeout:
            return "tokens usage timed out"
        }
    }
}
