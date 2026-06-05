import Foundation

public struct TokscaleSummary: Decodable, Equatable {
    public let version: Int
    public let generatedAt: String
    public var stale: Bool
    public var staleReason: String?
    public var collapsed: Collapsed
    public let today: Today
    public let totals: Totals
    public let providers: [Provider]
    public let quota: [QuotaProvider]
    public let history: [HistoryDay]
    public let top: Top
    public let latestSubmit: LatestSubmit?
    public let health: Health
    public let accuracy: Accuracy

    private enum CodingKeys: String, CodingKey {
        case version
        case generatedAt
        case stale
        case staleReason
        case collapsed
        case today
        case totals
        case providers
        case quota
        case history
        case top
        case latestSubmit
        case health
        case accuracy
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        version = try container.decode(Int.self, forKey: .version)
        generatedAt = try container.decode(String.self, forKey: .generatedAt)
        stale = try container.decode(Bool.self, forKey: .stale)
        staleReason = try container.decodeIfPresent(String.self, forKey: .staleReason)
        collapsed = try container.decode(Collapsed.self, forKey: .collapsed)
        today = try container.decode(Today.self, forKey: .today)
        totals = try container.decode(Totals.self, forKey: .totals)
        providers = try container.decodeIfPresent([Provider].self, forKey: .providers) ?? []
        quota = try container.decodeIfPresent([QuotaProvider].self, forKey: .quota) ?? []
        history = try container.decodeIfPresent([HistoryDay].self, forKey: .history) ?? []
        top = try container.decode(Top.self, forKey: .top)
        latestSubmit = try container.decodeIfPresent(LatestSubmit.self, forKey: .latestSubmit)
        health = try container.decode(Health.self, forKey: .health)
        accuracy = try container.decode(Accuracy.self, forKey: .accuracy)
    }

    public static func decode(_ data: Data) throws -> TokscaleSummary {
        try JSONDecoder().decode(TokscaleSummary.self, from: data)
    }

    public static func defaultSummaryURL(
        homeDirectory: URL = FileManager.default.homeDirectoryForCurrentUser
    ) -> URL {
        homeDirectory
            .appendingPathComponent(".config", isDirectory: true)
            .appendingPathComponent("tokens", isDirectory: true)
            .appendingPathComponent("cache", isDirectory: true)
            .appendingPathComponent("companion-summary.json", isDirectory: false)
    }

    public var statusTitle: String {
        let label = collapsed.label.isEmpty ? "Tokens" : collapsed.label
        return stale ? "\(label)!" : label
    }

    public var menuBarTitle: String {
        "Tokens"
    }

    public var menuLines: [String] {
        [
            "Today: \(formatUSD(today.costUsd)) - \(formatTokens(today.tokens)) tokens - \(today.messages) messages",
            "Total: \(formatUSD(totals.costUsd)) - \(formatTokens(totals.tokens)) tokens - \(totals.activeDays) active days",
            topLine,
            accuracyLine,
            "Last scan: \(formatDuration(milliseconds: health.lastScanDurationMs))"
        ]
    }

    public mutating func refreshFreshness(
        now: Date = Date(),
        calendar: Calendar = .current
    ) {
        guard let generatedAtDate = parseISODate(generatedAt) else {
            markStale(reason: "invalid-generated-at")
            return
        }
        if now.timeIntervalSince(generatedAtDate) > 2 * 60 * 60 {
            markStale(reason: "summary-older-than-2h")
            return
        }
        if today.date != localDateString(now: now, calendar: calendar) {
            markStale(reason: "summary-date-mismatch")
        }
    }

    public func needsRefreshOnOpen(
        now: Date = Date(),
        minimumInterval: TimeInterval = 60
    ) -> Bool {
        if stale {
            return true
        }
        guard let generatedAtDate = parseISODate(generatedAt) else {
            return true
        }
        return now.timeIntervalSince(generatedAtDate) >= minimumInterval
    }

    private mutating func markStale(reason: String) {
        stale = true
        staleReason = staleReason ?? reason
        collapsed = Collapsed(
            metric: collapsed.metric,
            label: collapsed.label,
            state: "stale"
        )
    }

    private var topLine: String {
        switch (top.client, top.model) {
        case let (client?, model?):
            return "Top: \(client) - \(model)"
        case let (client?, nil):
            return "Top: \(client)"
        case let (nil, model?):
            return "Top: \(model)"
        default:
            return "Top: none"
        }
    }

    private var accuracyLine: String {
        let source = accuracy.sourceKinds.first ?? (accuracy.warnings.isEmpty ? "unknown" : "warning")
        return "Accuracy: \(accuracy.confidence) - \(source)"
    }
}

public struct TokscaleSummaryStore {
    public let summaryURL: URL

    public init(summaryURL: URL = TokscaleSummary.defaultSummaryURL()) {
        self.summaryURL = summaryURL
    }

    public func load(
        now: Date = Date(),
        calendar: Calendar = .current
    ) throws -> TokscaleSummary? {
        guard FileManager.default.fileExists(atPath: summaryURL.path) else {
            return nil
        }
        var summary = try TokscaleSummary.decode(Data(contentsOf: summaryURL))
        summary.refreshFreshness(now: now, calendar: calendar)
        return summary
    }
}

public struct TokscaleDashboardModel: Equatable {
    public let hero: Hero
    public let clientLabels: [String]
    public let providers: [ProviderSummary]
    public let metrics: [Panel]
    public let insights: [Panel]
    public let quotaWindows: [QuotaWindowSummary]
    public let historyTrend: [HistoryPoint]
    public let previousWeekTrend: [HistoryPoint]
    public let currentWeekTrend: [HistoryPoint]
    public let historyPeak: HistoryPoint?
    public let spendHighlights: [Panel]
    public let health: HealthStatus
    private let summaryStale: Bool
    private let quotaWasRefreshed: Bool
    private let providerDetailsById: [String: ProviderDetails]

    private static let quotaBoardProviderIds = ["claude", "codex", "gemini"]

    public init(summary: TokscaleSummary) {
        let providerRows = Self.providerRows(summary: summary)
        let historyRows = Self.historyRows(summary: summary)
        let weekTrends = Self.weekTrends(historyRows: historyRows)
        let hasLiveQuotaRefresh = summary.health.quotaRefreshedAt != nil
        quotaWasRefreshed = hasLiveQuotaRefresh
        summaryStale = summary.stale
        clientLabels = providerRows.map { clientDisplayName($0.client) }
        providers = Self.providerSummaries(rows: providerRows, totalCost: summary.totals.costUsd)
        providerDetailsById = Dictionary(
            uniqueKeysWithValues: providerRows.map { row in
                (
                    row.client,
                    ProviderDetails(
                        id: row.client,
                        title: clientDisplayName(row.client),
                        model: row.topModel ?? "No model data",
                        today: "\(formatUSD(row.todayCostUsd)) today",
                        total: "\(formatUSD(row.costUsd)) total",
                        tokens: formatTokens(row.tokens),
                        messages: "\(row.messages) messages",
                        share: Self.providerShare(row.costUsd, totalCost: summary.totals.costUsd),
                        hasLiveQuotaRefresh: hasLiveQuotaRefresh
                    )
                )
            }
        )
        hero = Hero(
            title: summary.statusTitle,
            subtitle: "\(clientLabels.count) AI clients - local cache",
            state: summary.collapsed.state,
            progress: Self.progressAgainstDailyAverage(summary: summary),
            progressLabel: Self.progressLabelAgainstDailyAverage(summary: summary)
        )
        metrics = [
            Panel(
                title: "Today",
                value: formatUSD(summary.today.costUsd),
                detail: "\(formatTokens(summary.today.tokens)) tokens - \(summary.today.messages) messages"
            ),
            Panel(
                title: "Total",
                value: formatUSD(summary.totals.costUsd),
                detail: "\(formatTokens(summary.totals.tokens)) tokens - \(summary.totals.activeDays) active days"
            )
        ]
        insights = [
            Panel(
                title: "Top driver",
                value: summary.top.client ?? summary.top.model ?? "none",
                detail: summary.top.model ?? "No model data"
            ),
            Panel(
                title: "Accuracy",
                value: summary.accuracy.confidence,
                detail: summary.accuracy.sourceKinds.first ?? (summary.accuracy.warnings.isEmpty ? "unknown" : "warning")
            )
        ]
        quotaWindows = Self.quotaWindows(summary: summary)
        historyTrend = historyRows
        previousWeekTrend = weekTrends.previous
        currentWeekTrend = weekTrends.current
        historyPeak = historyRows.max { left, right in
            if left.costUsd == right.costUsd {
                return left.date < right.date
            }
            return left.costUsd < right.costUsd
        }
        spendHighlights = Self.spendHighlights(summary: summary, currentWeekTrend: weekTrends.current, previousWeekTrend: weekTrends.previous)
        health = HealthStatus(
            title: summary.stale ? "Stale" : "Fresh",
            detail: "Last scan \(formatDuration(milliseconds: summary.health.lastScanDurationMs))",
            warning: summary.staleReason ?? summary.health.warnings.first
        )
    }

    public var quotaBoardProviders: [ProviderFocus] {
        Self.quotaBoardProviderIds.compactMap { providerFocusIfAvailable(for: $0) }
    }

    public func providerDetails(for id: String?) -> ProviderDetails {
        if let id, let details = providerDetailsById[id] {
            return details
        }
        if let first = providers.first, let details = providerDetailsById[first.id] {
            return details
        }
        return ProviderDetails(
            id: "none",
            title: "No provider",
            model: "No model data",
            today: "$0.00 today",
            total: "$0.00 total",
            tokens: "0",
            messages: "0 messages",
            share: 0,
            hasLiveQuotaRefresh: false
        )
    }

    public func providerFocus(for id: String?) -> ProviderFocus {
        let details = providerDetails(for: id)
        return providerFocus(details: details)
    }

    private func providerFocusIfAvailable(for id: String) -> ProviderFocus? {
        if let details = providerDetailsById[id] {
            return providerFocus(details: details)
        }
        let title = clientDisplayName(id)
        let hasQuota = quotaWindows.contains { window in
            let provider = window.provider.lowercased()
            return provider == id || provider == title.lowercased()
        }
        guard hasQuota else {
            return nil
        }
        return providerFocus(
            details: ProviderDetails(
                id: id,
                title: title,
                model: "No local model data",
                today: "$0.00 today",
                total: "$0.00 total",
                tokens: "0",
                messages: "0 messages",
                share: 0,
                hasLiveQuotaRefresh: quotaWasRefreshed
            )
        )
    }

    private func providerFocus(details: ProviderDetails) -> ProviderFocus {
        let normalized = details.id.lowercased()
        let quota = quotaWindows.filter { window in
            let provider = window.provider.lowercased()
            return provider == details.title.lowercased() || provider == normalized
        }
        let primary = quota.first { Self.isFiveHourQuotaTitle($0.title) } ?? quota.first
        let weekly = quota.first { Self.isWeeklyQuotaTitle($0.title) }

        return ProviderFocus(
            id: details.id,
            title: details.title,
            topModel: details.model,
            today: details.today,
            total: details.total,
            tokens: details.tokens,
            messages: details.messages,
            share: details.share,
            quotaWindows: quota,
            primaryQuota: primary,
            weeklyQuota: weekly,
            quotaStatus: quota.isEmpty ? "No live quota" : (summaryStale && !details.hasLiveQuotaRefresh ? "Cached" : "Live"),
            workTime: "Work time unavailable",
            focusedModelTime: Self.focusedModelTimeLabel(providerId: details.id, model: details.model)
        )
    }

    private static func providerRows(summary: TokscaleSummary) -> [TokscaleSummary.Provider] {
        if !summary.providers.isEmpty {
            return summary.providers.sorted { left, right in
                if left.costUsd == right.costUsd {
                    return left.client < right.client
                }
                return left.costUsd > right.costUsd
            }
        }
        return summary.totals.clients.map { client in
            TokscaleSummary.Provider(
                client: client,
                costUsd: 0,
                tokens: 0,
                messages: 0,
                todayCostUsd: 0,
                todayTokens: 0,
                todayMessages: 0,
                topModel: nil
            )
        }
    }

    private static func providerSummaries(
        rows: [TokscaleSummary.Provider],
        totalCost: Double
    ) -> [ProviderSummary] {
        rows.map { row in
            let share = providerShare(row.costUsd, totalCost: totalCost)
            return ProviderSummary(
                id: row.client,
                label: clientDisplayName(row.client),
                value: formatUSD(row.costUsd),
                detail: "\(formatTokens(row.tokens)) tokens - \(Int((share * 100).rounded()))%",
                share: share
            )
        }
    }

    private static func providerShare(_ cost: Double, totalCost: Double) -> Double {
        guard totalCost > 0 else {
            return 0
        }
        return min(max(cost / totalCost, 0), 1)
    }

    private static func quotaWindows(summary: TokscaleSummary) -> [QuotaWindowSummary] {
        summary.quota.flatMap { provider in
            provider.windows.map { window in
                let usedPercent = min(max(window.usedPercent, 0), 100)
                let remainingPercent = min(max(window.remainingPercent, 0), 100)
                return QuotaWindowSummary(
                    provider: provider.provider,
                    plan: provider.plan,
                    title: Self.displayQuotaTitle(window.label),
                    value: window.remainingLabel ?? "\(formatPercent(remainingPercent))% left",
                    detail: "\(formatPercent(usedPercent))% used",
                    reset: window.resetsAt,
                    progress: usedPercent / 100,
                    usedPercent: usedPercent,
                    remainingPercent: remainingPercent
                )
            }
        }
    }

    private static func displayQuotaTitle(_ label: String) -> String {
        switch label.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() {
        case "session":
            return "5h"
        case "weekly":
            return "Week"
        default:
            return label
        }
    }

    private static func isFiveHourQuotaTitle(_ title: String) -> Bool {
        let normalized = title.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return normalized == "5h" || normalized == "session"
    }

    private static func isWeeklyQuotaTitle(_ title: String) -> Bool {
        let normalized = title.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return normalized == "week" || normalized == "weekly"
    }

    private static func historyRows(summary: TokscaleSummary) -> [HistoryPoint] {
        let maxCost = summary.history.map(\.costUsd).max() ?? 0
        return summary.history.map { day in
            HistoryPoint(
                date: day.date,
                value: formatUSD(day.costUsd),
                costUsd: day.costUsd,
                tokens: formatTokens(day.tokens),
                messages: "\(day.messages) messages",
                progress: maxCost > 0 ? min(max(day.costUsd / maxCost, 0), 1) : 0
            )
        }
    }

    private static func weekTrends(historyRows: [HistoryPoint]) -> (previous: [HistoryPoint], current: [HistoryPoint]) {
        let recent = Array(historyRows.suffix(14))
        if recent.count <= 7 {
            return ([], recent)
        }
        return (Array(recent.dropLast(7)), Array(recent.suffix(7)))
    }

    private static func spendHighlights(
        summary: TokscaleSummary,
        currentWeekTrend: [HistoryPoint],
        previousWeekTrend: [HistoryPoint]
    ) -> [Panel] {
        let currentWeekCost = currentWeekTrend.reduce(0) { $0 + $1.costUsd }
        let previousWeekCost = previousWeekTrend.reduce(0) { $0 + $1.costUsd }
        return [
            Panel(
                title: "Today",
                value: formatUSD(summary.today.costUsd),
                detail: "\(formatTokens(summary.today.tokens)) tokens - \(summary.today.messages) messages"
            ),
            Panel(
                title: "All-time",
                value: formatUSD(summary.totals.costUsd),
                detail: "\(formatTokens(summary.totals.tokens)) tokens - \(summary.totals.activeDays) active days"
            ),
            Panel(
                title: "7d spend",
                value: formatUSD(currentWeekCost),
                detail: weekComparisonDetail(current: currentWeekCost, previous: previousWeekCost)
            )
        ]
    }

    private static func weekComparisonDetail(current: Double, previous: Double) -> String {
        if previous <= 0 {
            return current > 0 ? "new vs prior 7d" : "flat vs prior 7d"
        }
        let percent = Int(((current - previous) / previous * 100).rounded())
        if percent > 0 {
            return "+\(percent)% vs prior 7d"
        }
        return "\(percent)% vs prior 7d"
    }

    private static func progressAgainstDailyAverage(summary: TokscaleSummary) -> Double {
        guard summary.totals.activeDays > 0, summary.totals.costUsd > 0 else {
            return 0
        }
        let dailyAverage = summary.totals.costUsd / Double(summary.totals.activeDays)
        guard dailyAverage > 0 else {
            return 0
        }
        return min(summary.today.costUsd / dailyAverage / 2, 1)
    }

    private static func progressLabelAgainstDailyAverage(summary: TokscaleSummary) -> String {
        guard summary.totals.activeDays > 0, summary.totals.costUsd > 0 else {
            return "No daily average yet"
        }
        let dailyAverage = summary.totals.costUsd / Double(summary.totals.activeDays)
        guard dailyAverage > 0 else {
            return "No daily average yet"
        }
        let percent = Int((summary.today.costUsd / dailyAverage * 100).rounded())
        return "\(percent)% of daily average"
    }

    private static func focusedModelTimeLabel(providerId: String, model: String) -> String {
        if providerId.lowercased() == "claude", model.lowercased().contains("sonnet") {
            return "Sonnet-only unavailable"
        }
        return "Model time unavailable"
    }
}

public extension TokscaleDashboardModel {
    enum QuotaDisplayMode: String, CaseIterable, Equatable {
        case remaining
        case used

        public var title: String {
            switch self {
            case .remaining:
                return "Left"
            case .used:
                return "Used"
            }
        }
    }

    struct Hero: Equatable {
        public let title: String
        public let subtitle: String
        public let state: String
        public let progress: Double
        public let progressLabel: String
    }

    struct Panel: Equatable {
        public let title: String
        public let value: String
        public let detail: String

        public init(title: String, value: String, detail: String) {
            self.title = title
            self.value = value
            self.detail = detail
        }
    }

    struct HealthStatus: Equatable {
        public let title: String
        public let detail: String
        public let warning: String?
    }

    struct QuotaWindowSummary: Equatable {
        public let provider: String
        public let plan: String?
        public let title: String
        public let value: String
        public let detail: String
        public let reset: String?
        public let progress: Double
        public let usedPercent: Double
        public let remainingPercent: Double

        public func value(for mode: QuotaDisplayMode) -> String {
            switch mode {
            case .remaining:
                return value
            case .used:
                return "\(formatPercent(usedPercent))% used"
            }
        }

        public func detail(for mode: QuotaDisplayMode) -> String {
            switch mode {
            case .remaining:
                return detail
            case .used:
                return "\(formatPercent(remainingPercent))% left"
            }
        }

        public func progress(for mode: QuotaDisplayMode) -> Double {
            switch mode {
            case .remaining:
                return remainingPercent / 100
            case .used:
                return usedPercent / 100
            }
        }
    }

    struct HistoryPoint: Equatable {
        public let date: String
        public let value: String
        public let costUsd: Double
        public let tokens: String
        public let messages: String
        public let progress: Double
    }

    struct ProviderSummary: Equatable {
        public let id: String
        public let label: String
        public let value: String
        public let detail: String
        public let share: Double
    }

    struct ProviderDetails: Equatable {
        public let id: String
        public let title: String
        public let model: String
        public let today: String
        public let total: String
        public let tokens: String
        public let messages: String
        public let share: Double
        public let hasLiveQuotaRefresh: Bool
    }

    struct ProviderFocus: Equatable {
        public let id: String
        public let title: String
        public let topModel: String
        public let today: String
        public let total: String
        public let tokens: String
        public let messages: String
        public let share: Double
        public let quotaWindows: [QuotaWindowSummary]
        public let primaryQuota: QuotaWindowSummary?
        public let weeklyQuota: QuotaWindowSummary?
        public let quotaStatus: String
        public let workTime: String
        public let focusedModelTime: String
    }
}

public extension TokscaleSummary {
    struct Collapsed: Decodable, Equatable {
        public let metric: String
        public let label: String
        public let state: String
    }

    struct Today: Decodable, Equatable {
        public let date: String
        public let costUsd: Double
        public let tokens: Int64
        public let messages: Int
    }

    struct Totals: Decodable, Equatable {
        public let costUsd: Double
        public let tokens: Int64
        public let activeDays: Int
        public let clients: [String]
        public let models: Int
    }

    struct Provider: Decodable, Equatable {
        public let client: String
        public let costUsd: Double
        public let tokens: Int64
        public let messages: Int
        public let todayCostUsd: Double
        public let todayTokens: Int64
        public let todayMessages: Int
        public let topModel: String?

        public init(
            client: String,
            costUsd: Double,
            tokens: Int64,
            messages: Int,
            todayCostUsd: Double,
            todayTokens: Int64,
            todayMessages: Int,
            topModel: String?
        ) {
            self.client = client
            self.costUsd = costUsd
            self.tokens = tokens
            self.messages = messages
            self.todayCostUsd = todayCostUsd
            self.todayTokens = todayTokens
            self.todayMessages = todayMessages
            self.topModel = topModel
        }
    }

    struct QuotaProvider: Decodable, Equatable {
        public let provider: String
        public let plan: String?
        public let windows: [QuotaWindow]
    }

    struct QuotaWindow: Decodable, Equatable {
        public let label: String
        public let usedPercent: Double
        public let remainingPercent: Double
        public let remainingLabel: String?
        public let resetsAt: String?
    }

    struct HistoryDay: Decodable, Equatable {
        public let date: String
        public let costUsd: Double
        public let tokens: Int64
        public let messages: Int
    }

    struct Top: Decodable, Equatable {
        public let client: String?
        public let model: String?
    }

    struct LatestSubmit: Decodable, Equatable {
        public let status: String
        public let finishedAt: String
        public let submissionId: String?
    }

    struct Health: Decodable, Equatable {
        public let summaryPath: String
        public let lastScanDurationMs: Int
        public let quotaRefreshedAt: String?
        public let warnings: [String]
    }

    struct Accuracy: Decodable, Equatable {
        public let confidence: String
        public let sourceKinds: [String]
        public let warnings: [String]
    }
}

private extension String {
    var displayName: String {
        switch self {
        case "todayCost":
            return "Today cost"
        case "todayTokens":
            return "Today tokens"
        default:
            return self
        }
    }
}

private func clientDisplayName(_ value: String) -> String {
    switch value.lowercased() {
    case "claude":
        return "Claude"
    case "codex":
        return "Codex"
    case "gemini":
        return "Gemini"
    case "openclaw":
        return "OpenClaw"
    case "copilot":
        return "Copilot"
    case "antigravity":
        return "Antigravity"
    default:
        return value.prefix(1).uppercased() + value.dropFirst()
    }
}

private func formatUSD(_ value: Double) -> String {
    if abs(value) >= 1_000 {
        return String(format: "$%.1fK", value / 1_000)
    }
    return String(format: "$%.2f", value)
}

private func formatPercent(_ value: Double) -> String {
    let rounded = value.rounded()
    if abs(value - rounded) < 0.05 {
        return "\(Int(rounded))"
    }
    return String(format: "%.1f", value)
}

private func formatTokens(_ value: Int64) -> String {
    if value >= 1_000_000_000 {
        return compact(Double(value) / 1_000_000_000, suffix: "B")
    }
    if value >= 1_000_000 {
        return compact(Double(value) / 1_000_000, suffix: "M")
    }
    if value >= 1_000 {
        return compact(Double(value) / 1_000, suffix: "K")
    }
    return "\(value)"
}

private func compact(_ value: Double, suffix: String) -> String {
    let formatted = String(format: "%.1f", value)
    if formatted.hasSuffix(".0") {
        return "\(formatted.dropLast(2))\(suffix)"
    }
    return "\(formatted)\(suffix)"
}

private func formatDuration(milliseconds: Int) -> String {
    let seconds = max(0, (milliseconds + 500) / 1_000)
    let minutes = seconds / 60
    let remainingSeconds = seconds % 60
    if minutes > 0 {
        return "\(minutes)m \(remainingSeconds)s"
    }
    return "\(remainingSeconds)s"
}

private func parseISODate(_ value: String) -> Date? {
    let fractional = ISO8601DateFormatter()
    fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    if let date = fractional.date(from: value) {
        return date
    }
    return ISO8601DateFormatter().date(from: value)
}

private func localDateString(now: Date, calendar: Calendar) -> String {
    let components = calendar.dateComponents([.year, .month, .day], from: now)
    guard let year = components.year, let month = components.month, let day = components.day else {
        return ""
    }
    return String(format: "%04d-%02d-%02d", year, month, day)
}
