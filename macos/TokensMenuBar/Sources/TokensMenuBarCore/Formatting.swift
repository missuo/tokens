import Foundation

public enum Formatting {
    public static func compactTokens(_ value: Int64) -> String {
        let absValue = abs(Double(value))
        let sign = value < 0 ? "-" : ""
        switch absValue {
        case 0..<1000:
            return "\(sign)\(value)"
        case 1000..<1_000_000:
            return String(format: "%@%.1fK", sign, absValue / 1000).replacingOccurrences(of: ".0K", with: "K")
        case 1_000_000..<1_000_000_000:
            return String(format: "%@%.1fM", sign, absValue / 1_000_000).replacingOccurrences(of: ".0M", with: "M")
        default:
            return String(format: "%@%.1fB", sign, absValue / 1_000_000_000).replacingOccurrences(of: ".0B", with: "B")
        }
    }

    public static func cost(_ value: Double) -> String {
        if value > 0 && value < 0.01 {
            return "<$0.01"
        }
        return String(format: "$%.2f", value)
    }

    public static func menuBarTitle(
        report: UsageReport?,
        mode: MenuBarDisplayMode,
        missingBinary: Bool,
        hasError: Bool
    ) -> String {
        if missingBinary {
            return "tokens?"
        }
        guard let report else {
            return hasError ? "—" : "…"
        }
        let tokens = compactTokens(report.summary.totalTokens)
        let costText = cost(report.summary.totalCost)
        switch mode {
        case .tokens:
            return tokens
        case .cost:
            return costText
        case .both:
            return "\(tokens) · \(costText)"
        }
    }

    public static func percent(_ share: Double) -> String {
        String(format: "%.0f%%", share * 100)
    }

    /// Input cache hit rate: cache-read / (input + cache-read). Returns 0…1.
    public static func inputCacheRate(input: Int64, cacheRead: Int64) -> Double {
        let denominator = input + cacheRead
        guard denominator > 0 else { return 0 }
        let n = max(0, Double(cacheRead))
        return min(1, n / Double(denominator))
    }

    public static func relativeTime(fromISO8601 value: String) -> String {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        var date = formatter.date(from: value)
        if date == nil {
            formatter.formatOptions = [.withInternetDateTime]
            date = formatter.date(from: value)
        }
        guard let date else { return value }
        let rel = RelativeDateTimeFormatter()
        rel.unitsStyle = .short
        return rel.localizedString(for: date, relativeTo: Date())
    }

    /// Dense chart axis label: `"2026-07-24"` → `"24"`. Tooltip should use full ISO date.
    public static func chartDayLabel(isoDate: String) -> String {
        if isoDate.count >= 10 {
            return String(isoDate.suffix(2))
        }
        return isoDate
    }
}
