import Foundation

public enum ContributionCalendarGrid {
    public static func build(
        days: [(date: String, costUsd: Double)],
        endDate: String
    ) -> [[Int]] {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC") ?? .current
        let formatter = DateFormatter()
        formatter.calendar = calendar
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = calendar.timeZone
        formatter.dateFormat = "yyyy-MM-dd"

        guard let end = formatter.date(from: endDate),
            let cutoff = calendar.date(byAdding: .day, value: -364, to: end)
        else { return [] }

        var byDate: [String: Double] = [:]
        for day in days {
            guard let date = formatter.date(from: day.date), date >= cutoff, date <= end else {
                continue
            }
            byDate[day.date] = max(byDate[day.date] ?? 0, day.costUsd)
        }
        let sortedCosts = byDate.values.filter { $0 > 0 }.sorted()
        func bucket(_ cost: Double) -> Int {
            guard cost > 0 else { return 0 }
            guard !sortedCosts.isEmpty else { return 1 }
            func threshold(_ percentile: Double) -> Double {
                let index = Int((Double(sortedCosts.count) - 1) * percentile)
                return sortedCosts[min(sortedCosts.count - 1, index)]
            }
            if cost <= threshold(0.25) { return 1 }
            if cost <= threshold(0.50) { return 2 }
            if cost <= threshold(0.75) { return 3 }
            return 4
        }

        let leadingDays = calendar.component(.weekday, from: cutoff) - 1
        let trailingDays = 7 - calendar.component(.weekday, from: end)
        guard let gridStart = calendar.date(byAdding: .day, value: -leadingDays, to: cutoff),
            let gridEnd = calendar.date(byAdding: .day, value: trailingDays, to: end)
        else { return [] }

        var columns: [[Int]] = []
        var current: [Int] = []
        var date = gridStart
        while date <= gridEnd {
            if date < cutoff || date > end {
                current.append(-1)
            } else {
                current.append(bucket(byDate[formatter.string(from: date)] ?? 0))
            }
            if current.count == 7 {
                columns.append(current)
                current = []
            }
            guard let next = calendar.date(byAdding: .day, value: 1, to: date) else {
                return []
            }
            date = next
        }
        return columns
    }
}
