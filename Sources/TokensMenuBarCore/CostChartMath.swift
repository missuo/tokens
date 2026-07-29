import Foundation

public enum CostChartMath {
    public static func daysForChart(from days: [DayUsage], limit: Int = 14) -> [DayUsage] {
        let sorted = days.sorted { $0.date < $1.date }
        guard sorted.count > limit else { return sorted }
        return Array(sorted.suffix(limit))
    }

    public static func yMax(costs: [Double]) -> Double {
        let m = costs.max() ?? 0
        if m <= 0 { return 1 }
        return ceil(m)
    }
}
