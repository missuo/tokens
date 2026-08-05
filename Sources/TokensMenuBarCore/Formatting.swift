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

    public static func chartCostTick(_ value: Double) -> String {
        if value.rounded() == value {
            return "$\(Int(value))"
        }
        let magnitude = abs(value)
        if magnitude < 0.00000001 {
            return String(format: "$%.1e", value)
        }
        let precision = max(2, min(8, Int(ceil(-log10(magnitude)))))
        return String(format: "$%.*f", precision, value)
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

    /// Input cache hit rate: cache-read / (input + cache-read). Returns 0…1.
    public static func inputCacheRate(input: Int64, cacheRead: Int64) -> Double {
        let denominator = input + cacheRead
        guard denominator > 0 else { return 0 }
        let n = max(0, Double(cacheRead))
        return min(1, n / Double(denominator))
    }

    public static func relativeTime(fromISO8601 value: String) -> String {
        guard let date = parseISO8601(value) else { return value }
        let rel = RelativeDateTimeFormatter()
        rel.unitsStyle = .short
        return rel.localizedString(for: date, relativeTo: Date())
    }

    public static func compactDateRange(
        _ range: DateSelectionRange,
        timeZone: TimeZone,
        locale: Locale = .current
    ) -> String {
        guard let start = try? DateRangePickerConversion.date(
            from: range.startDate,
            timeZone: timeZone
        ), let end = try? DateRangePickerConversion.date(
            from: range.endDate,
            timeZone: timeZone
        ) else {
            return range.startDate == range.endDate
                ? range.startDate
                : "\(range.startDate)–\(range.endDate)"
        }

        let formatter = DateFormatter()
        formatter.calendar = DateRangePickerConversion.calendar(timeZone: timeZone)
        formatter.timeZone = timeZone
        formatter.locale = locale
        formatter.dateStyle = .short
        formatter.timeStyle = .none
        let startText = formatter.string(from: start)
        guard range.startDate != range.endDate else { return startText }
        return "\(startText)–\(formatter.string(from: end))"
    }

    public static func chartBucketLabels(
        buckets: [UsageTimeBucket],
        granularity: UsageTimeGranularity,
        timeZone: TimeZone,
        locale: Locale = .current
    ) -> [String] {
        let dates = buckets.map { parseISO8601($0.nominalStart) }
        let baseLabels = dates.map { date -> String in
            guard let date else { return "—" }
            return chartLabel(
                for: date,
                granularity: granularity,
                timeZone: timeZone,
                locale: locale,
                includeOffset: false
            )
        }
        return disambiguateRepeatedLabels(
            baseLabels,
            dates: dates,
            granularity: granularity,
            timeZone: timeZone,
            locale: locale
        )
    }

    public static func chartBucketAccessibilityLabel(
        _ bucket: UsageTimeBucket,
        timeZone: TimeZone,
        locale: Locale = .current
    ) -> String {
        var states: [String] = []
        if bucket.contextOnly { states.append("context, excluded from selected total") }
        if bucket.active { states.append("active") }
        if bucket.incompleteEdge { states.append("incomplete edge") }
        let state = states.isEmpty ? "" : ", " + states.joined(separator: ", ")
        return "\(chartBucketTooltipRange(bucket, timeZone: timeZone, locale: locale)), "
            + "\(cost(bucket.totals.cost, locale: locale)), "
            + "\(compactTokens(bucket.totals.tokens, locale: locale)) tokens\(state)"
    }

    /// Compact covered-range label for the cost chart tooltip. Ranges within
    /// a single calendar day keep their hours (`Aug 5, 09:00 – 10:00`); a
    /// full day collapses to the date alone; ranges spanning two or more days
    /// drop the hours (`Aug 5 – 7`, `Aug 30 – Sep 1`). The covered end is
    /// exclusive, so day boundaries are derived from the last covered moment.
    public static func chartBucketTooltipRange(
        _ bucket: UsageTimeBucket,
        timeZone: TimeZone,
        locale: Locale = .current
    ) -> String {
        guard let start = parseISO8601(bucket.coveredStart),
              let end = parseISO8601(bucket.coveredEndExclusive) else {
            return "\(bucket.coveredStart) – \(bucket.coveredEndExclusive)"
        }
        let calendar = DateRangePickerConversion.calendar(timeZone: timeZone)
        let formatter = DateFormatter()
        formatter.calendar = calendar
        formatter.timeZone = timeZone
        formatter.locale = locale

        func string(from date: Date, format: String) -> String {
            formatter.dateFormat = format
            return formatter.string(from: date)
        }

        // Step back an instant from the exclusive end to get the last
        // covered moment for day-boundary decisions.
        let lastCovered = end.addingTimeInterval(-0.001)

        if calendar.isDate(start, inSameDayAs: lastCovered) {
            if calendar.startOfDay(for: start) == start,
               calendar.startOfDay(for: end) == end {
                // Covers the whole day; the hours carry no information.
                return string(from: start, format: "MMM d")
            }
            return "\(string(from: start, format: "MMM d, HH:mm")) – "
                + string(from: end, format: "HH:mm")
        }

        if calendar.isDate(start, equalTo: lastCovered, toGranularity: .month) {
            return "\(string(from: start, format: "MMM d")) – "
                + string(from: lastCovered, format: "d")
        }
        return "\(string(from: start, format: "MMM d")) – "
            + string(from: lastCovered, format: "MMM d")
    }

    private static func chartLabel(
        for date: Date,
        granularity: UsageTimeGranularity,
        timeZone: TimeZone,
        locale: Locale,
        includeOffset: Bool
    ) -> String {
        let formatter = DateFormatter()
        formatter.calendar = DateRangePickerConversion.calendar(timeZone: timeZone)
        formatter.timeZone = timeZone
        formatter.locale = locale
        switch granularity {
        case .hour:
            formatter.dateFormat = includeOffset ? "ha XXXXX" : "ha"
        case .day:
            formatter.setLocalizedDateFormatFromTemplate("EEE d")
        case .naturalWeek:
            formatter.setLocalizedDateFormatFromTemplate("MMM d")
        case .naturalMonth:
            formatter.setLocalizedDateFormatFromTemplate("MMM")
        }
        return formatter.string(from: date)
            .replacingOccurrences(of: "-", with: "−")
    }

    private static func disambiguateRepeatedLabels(
        _ labels: [String],
        dates: [Date?],
        granularity: UsageTimeGranularity,
        timeZone: TimeZone,
        locale: Locale
    ) -> [String] {
        let counts = Dictionary(grouping: labels, by: { $0 }).mapValues(\.count)
        guard counts.values.contains(where: { $0 > 1 }) else { return labels }
        return zip(labels, dates).map { label, date in
            guard counts[label, default: 0] > 1, let date else { return label }
            let formatter = DateFormatter()
            formatter.calendar = DateRangePickerConversion.calendar(timeZone: timeZone)
            formatter.timeZone = timeZone
            formatter.locale = locale
            switch granularity {
            case .hour:
                return chartLabel(
                    for: date,
                    granularity: .hour,
                    timeZone: timeZone,
                    locale: locale,
                    includeOffset: true
                )
            case .day, .naturalWeek:
                formatter.setLocalizedDateFormatFromTemplate("MMM d yy")
            case .naturalMonth:
                formatter.setLocalizedDateFormatFromTemplate("MMM yy")
            }
            return formatter.string(from: date)
        }
    }

    static func parseISO8601(_ value: String) -> Date? {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let date = formatter.date(from: value) { return date }
        formatter.formatOptions = [.withInternetDateTime]
        return formatter.date(from: value)
    }
}
