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

public enum ScanIntervalOption: String, CaseIterable, Identifiable, Codable {
    case oneHour = "1h"
    case sixHours = "6h"
    case twelveHours = "12h"
    case twentyFourHours = "24h"
    case manual = "manual"

    public var id: String { rawValue }

    public var title: String {
        switch self {
        case .oneHour: return "1 hour"
        case .sixHours: return "6 hours"
        case .twelveHours: return "12 hours"
        case .twentyFourHours: return "24 hours"
        case .manual: return "Manual only"
        }
    }

    public var timeInterval: TimeInterval? {
        switch self {
        case .oneHour: return 3600
        case .sixHours: return 6 * 3600
        case .twelveHours: return 12 * 3600
        case .twentyFourHours: return 24 * 3600
        case .manual: return nil
        }
    }
}

public struct UsageReport: Codable, Equatable {
    public let schemaVersion: Int
    public let generatedAt: String
    public let period: String
    public let dateRange: DateRange
    public let scan: ScanInfo
    public let summary: UsageSummary
    public let tokenBreakdown: TokenBreakdown
    public let byClient: [ClientUsage]
    public let byModel: [ModelUsage]
    public let byDay: [DayUsage]
    public let meta: UsageMeta
}

public struct DateRange: Codable, Equatable {
    public let start: String
    public let end: String
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
    public let share: Double
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

public struct DayUsage: Codable, Equatable, Identifiable {
    public var id: String { date }
    public let date: String
    public let tokens: Int64
    public let cost: Double
    public let messages: Int32
    public let intensity: UInt8
}

public struct UsageMeta: Codable, Equatable {
    public let cliVersion: String
    public let timezone: String
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
