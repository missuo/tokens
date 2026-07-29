import Foundation

public enum Formatting {
    /// Locale-aware compact notation backed by CLDR rules via Foundation:
    /// `24k` / `1.2m` / `2b` in English, `2.4万` / `120万` / `20亿` in
    /// Simplified Chinese, and so on. Delegating the abbreviation rules to
    /// the platform keeps every supported language correct without
    /// hand-written per-locale tables.
    private static func compact(_ value: Double, locale: Locale) -> String {
        let formatted = value.formatted(.number.notation(.compactName).locale(locale))
        // Lowercase a single-letter ASCII unit suffix (K/M/B in English) so
        // the menu bar reads `$24k`; multi-letter units (e.g. German `Mrd.`)
        // are left exactly as CLDR formats them.
        guard let last = formatted.last, last.isASCII, last.isUppercase, last.isLetter else {
            return formatted
        }
        let prefix = formatted.dropLast()
        if let before = prefix.last, before.isASCII, before.isLetter {
            return formatted
        }
        return prefix + last.lowercased()
    }

    public static func compactTokens(_ value: Int64, locale: Locale = .current) -> String {
        compact(Double(value), locale: locale)
    }

    /// USD cost. Values below $1000 keep two decimals for precision; larger
    /// values are abbreviated with the locale-aware compact notation so the
    /// menu bar title stays short (e.g. `$24128.26` → `$24k`).
    public static func cost(_ value: Double, locale: Locale = .current) -> String {
        if value > 0 && value < 0.01 {
            return "<$0.01"
        }
        if abs(value) >= 1000 {
            return "$" + compact(value, locale: locale)
        }
        return String(format: "$%.2f", value)
    }

    public static func menuBarTitle(
        report: UsageReport?,
        mode: MenuBarDisplayMode,
        missingBinary: Bool,
        hasError: Bool,
        locale: Locale = .current
    ) -> String {
        if missingBinary {
            return "tokens?"
        }
        guard let report else {
            return hasError ? "—" : "…"
        }
        let tokens = compactTokens(report.summary.totalTokens, locale: locale)
        let costText = cost(report.summary.totalCost, locale: locale)
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
}
