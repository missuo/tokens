import Foundation

/// Pure math for the Advanced page weekday × hour heatmap — no SwiftUI.
public enum HeatmapMath {
    public static let weekdayCount = 7
    public static let hourCount = 24

    /// Row labels in ISO weekday order (Monday first), matching natural weeks.
    public static let weekdayShortLabels = ["MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN"]

    /// Grid lookup for one (weekday, hour) pair.
    public static func cell(
        weekday: Int,
        hour: Int,
        in cells: [UsageWeekdayHourCell]
    ) -> UsageWeekdayHourCell? {
        cells.first { $0.weekday == weekday && $0.hour == hour }
    }

    /// Peak cell by cost — the answer to "which evening burns the most".
    /// Nil when the grid is empty or every cell is zero. Equal cost breaks
    /// by higher tokens; otherwise the first maximum in grid order wins.
    /// That is deterministic for the weekday-major ordered grid the CLI emits.
    public static func peak(in cells: [UsageWeekdayHourCell]) -> UsageWeekdayHourCell? {
        var best: UsageWeekdayHourCell?
        for cell in cells where cell.cost > 0 {
            guard let current = best else {
                best = cell
                continue
            }
            if cell.cost > current.cost
                || (cell.cost == current.cost && cell.tokens > current.tokens) {
                best = cell
            }
        }
        return best
    }

    /// Fill intensity 0…1 for one cell. Square-root scaling keeps small
    /// cells visible next to a heavy peak. Zero cost maps to 0.
    public static func intensity(cost: Double, maximum: Double) -> Double {
        guard cost > 0, maximum > 0 else { return 0 }
        return min(1, (cost / maximum).squareRoot())
    }

    /// Cell opacity in the minimal-mono ramp: a faint floor for non-zero
    /// cells, full strength at the peak.
    public static func cellOpacity(cost: Double, maximum: Double) -> Double {
        let level = intensity(cost: cost, maximum: maximum)
        guard level > 0 else { return 0 }
        return 0.2 + 0.8 * level
    }

    public static func weekdayLabel(_ weekday: Int) -> String {
        guard (1...weekdayCount).contains(weekday) else { return "?" }
        return weekdayShortLabels[weekday - 1]
    }

    /// `21` → `21:00–22:00`; wraps `23` → `23:00–00:00`.
    public static func hourRangeLabel(hour: Int) -> String {
        String(format: "%02d:00–%02d:00", hour, (hour + 1) % hourCount)
    }
}
