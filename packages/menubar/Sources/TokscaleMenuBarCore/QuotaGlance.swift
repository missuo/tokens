import Foundation

public enum UrgencyLevel: Equatable {
    case normal
    case warning
    case critical
}

public enum QuotaGlance {
    public struct GlanceWindow: Equatable {
        public let provider: String
        public let label: String
        public let usedPercent: Double
        public let remainingPercent: Double
        public let resetsAt: String?
    }

    public struct ProviderHeadroom: Equatable {
        public let provider: String
        public let remainingPercent: Double
    }

    public static func recentSpend(_ history: [TokscaleSummary.HistoryDay], days: Int) -> Double {
        history.suffix(days).reduce(0) { $0 + $1.costUsd }
    }

    public static func urgency(remainingPercent: Double) -> UrgencyLevel {
        if remainingPercent <= 10 { return .critical }
        if remainingPercent <= 20 { return .warning }
        return .normal
    }

    public static func resetCountdown(from resetsAt: String?, now: Date = Date()) -> String? {
        guard let resetsAt, let resetDate = parseISODate(resetsAt) else { return nil }
        let seconds = resetDate.timeIntervalSince(now)
        guard seconds > 0 else { return nil }
        let minutes = Int(seconds / 60)
        if minutes < 60 { return "\(max(minutes, 1))m" }
        let hours = minutes / 60
        if hours < 24 { return "\(hours)h" }
        return "\(hours / 24)d"
    }

    public static func mostConstrained(
        in providers: [TokscaleSummary.QuotaProvider]
    ) -> GlanceWindow? {
        var best: GlanceWindow?
        for provider in providers {
            for window in provider.windows {
                let candidate = GlanceWindow(
                    provider: provider.provider,
                    label: window.label,
                    usedPercent: window.usedPercent,
                    remainingPercent: window.remainingPercent,
                    resetsAt: window.resetsAt
                )
                if let current = best, current.remainingPercent <= candidate.remainingPercent {
                    continue
                }
                best = candidate
            }
        }
        return best
    }

    public static func bestNow(
        in providers: [TokscaleSummary.QuotaProvider]
    ) -> ProviderHeadroom? {
        var result: ProviderHeadroom?
        for provider in providers where !provider.windows.isEmpty {
            let headroom = provider.windows.map(\.remainingPercent).min() ?? 0
            if let current = result, current.remainingPercent >= headroom {
                continue
            }
            result = ProviderHeadroom(provider: provider.provider, remainingPercent: headroom)
        }
        return result
    }

    public static func providersByUrgency(
        _ providers: [TokscaleSummary.QuotaProvider]
    ) -> [String] {
        providers
            .filter { !$0.windows.isEmpty }
            .sorted { lhs, rhs in
                (lhs.windows.map(\.remainingPercent).min() ?? 0)
                    < (rhs.windows.map(\.remainingPercent).min() ?? 0)
            }
            .map(\.provider)
    }
}
