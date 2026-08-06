import SwiftUI

/// ADVANCED page, first section — cost heatmap by reporting-timezone hour of
/// day × ISO weekday over the selected range: "which evenings burn the most".
public struct AdvancedHeatmapSection: View {
    public let report: UsageReport

    public init(report: UsageReport) {
        self.report = report
    }

    private var cells: [UsageWeekdayHourCell] { report.weekdayHour ?? [] }
    private var maximumCost: Double { cells.map(\.cost).max() ?? 0 }

    public var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("WEEKDAY × HOUR COST")
                .font(.system(size: 10, design: .monospaced))
                .foregroundStyle(.secondary)
                .tracking(1.0)
                .accessibilityAddTraits(.isHeader)

            if cells.isEmpty {
                Text("No hourly data for this range")
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(.secondary)
            } else {
                grid
                peakLine
            }
        }
    }

    // MARK: - Grid

    /// 24 cells × 11pt + 23 gaps × 3pt + 28pt row label + 3pt gap = 364pt,
    /// the panel content width (400 − 2×18 horizontal padding).
    private var grid: some View {
        VStack(alignment: .leading, spacing: 3) {
            hourHeader
            ForEach(1...HeatmapMath.weekdayCount, id: \.self) { weekday in
                gridRow(weekday: weekday)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Cost heatmap by hour of day and weekday")
    }

    private var hourHeader: some View {
        HStack(spacing: 3) {
            Text("")
                .frame(width: 28, alignment: .leading)
            ForEach(0..<HeatmapMath.hourCount, id: \.self) { hour in
                Text(hour % 6 == 0 ? "\(hour)" : "")
                    .font(.system(size: 8, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .frame(width: 11)
            }
        }
    }

    private func gridRow(weekday: Int) -> some View {
        HStack(spacing: 3) {
            Text(HeatmapMath.weekdayLabel(weekday))
                .font(.system(size: 8, design: .monospaced))
                .foregroundStyle(.secondary)
                .frame(width: 28, alignment: .leading)
            ForEach(0..<HeatmapMath.hourCount, id: \.self) { hour in
                gridCell(weekday: weekday, hour: hour)
            }
        }
    }

    private func gridCell(weekday: Int, hour: Int) -> some View {
        let cell = HeatmapMath.cell(weekday: weekday, hour: hour, in: cells)
        let cost = cell?.cost ?? 0
        let opacity = HeatmapMath.cellOpacity(cost: cost, maximum: maximumCost)
        return RoundedRectangle(cornerRadius: 2)
            .fill(Color.primary.opacity(cost > 0 ? opacity : 0.05))
            .frame(width: 11, height: 11)
            .help(cell.map { cellTooltip($0) } ?? "")
            .accessibilityLabel(
                cell.map { cellTooltip($0) } ?? "\(HeatmapMath.weekdayLabel(weekday)) \(hour):00, no usage"
            )
    }

    private func cellTooltip(_ cell: UsageWeekdayHourCell) -> String {
        "\(HeatmapMath.weekdayLabel(cell.weekday)) \(HeatmapMath.hourRangeLabel(hour: cell.hour)) · "
            + "\(Formatting.cost(cell.cost)) · \(Formatting.compactTokens(cell.tokens)) tokens"
    }

    // MARK: - Peak

    @ViewBuilder
    private var peakLine: some View {
        if let peak = HeatmapMath.peak(in: cells) {
            Text(
                "PEAK · \(HeatmapMath.weekdayLabel(peak.weekday)) "
                    + "\(HeatmapMath.hourRangeLabel(hour: peak.hour)) · \(Formatting.cost(peak.cost))"
            )
            .font(.system(size: 10, design: .monospaced))
            .foregroundStyle(.secondary)
            .tracking(0.6)
            .accessibilityLabel(
                "Peak usage: \(HeatmapMath.weekdayLabel(peak.weekday)) "
                    + "\(HeatmapMath.hourRangeLabel(hour: peak.hour)), \(Formatting.cost(peak.cost))"
            )
        }
    }
}
