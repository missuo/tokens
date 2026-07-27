import SwiftUI

/// 14-day cost bar chart with hover dimming, guide line, and tooltip (IX-A).
public struct CostChartView: View {
    public let days: [DayUsage]
    public var height: CGFloat
    /// Selected period raw value; hover clears when the period changes.
    public var periodRawValue: String

    @State private var hoveredDate: String?

    private let padL: CGFloat = 34
    private let padR: CGFloat = 4
    private let padT: CGFloat = 8
    private let padB: CGFloat = 22
    private let barSpacing: CGFloat = 3
    private let tooltipWidth: CGFloat = 140

    public init(
        days: [DayUsage],
        height: CGFloat = MenuBarLayout.chartHeight,
        periodRawValue: String = ""
    ) {
        self.days = days
        self.height = height
        self.periodRawValue = periodRawValue
    }

    public var body: some View {
        let chartDays = CostChartMath.daysForChart(from: days)
        Group {
            if chartDays.isEmpty {
                Text("No daily data")
                    .font(.body)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                chartBody(chartDays)
            }
        }
        .frame(maxWidth: .infinity)
        .frame(height: height)
        .onChange(of: periodRawValue) { _ in
            hoveredDate = nil
        }
    }

    private func chartBody(_ chartDays: [DayUsage]) -> some View {
        let costs = chartDays.map(\.cost)
        let yMax = CostChartMath.yMax(costs: costs)
        let ticks: [Double] = [0, yMax / 2, yMax]
        let count = chartDays.count
        let hoveredIndex = chartDays.firstIndex(where: { $0.date == hoveredDate })

        return GeometryReader { geo in
            let plotWidth = max(0, geo.size.width - padL - padR)
            let plotHeight = max(0, height - padT - padB)

            ZStack(alignment: .topLeading) {
                // Y-axis labels + grid lines
                ForEach(Array(ticks.enumerated()), id: \.offset) { _, value in
                    let y = padT + plotHeight * (1 - CGFloat(value / yMax))
                    Text("$\(Int(value))")
                        .font(.system(size: 9).monospacedDigit())
                        .foregroundStyle(.secondary)
                        .frame(width: padL - 6, alignment: .trailing)
                        .position(x: (padL - 6) / 2, y: y)

                    Rectangle()
                        .fill(Color.primary.opacity(value == 0 ? 0.22 : 0.06))
                        .frame(width: plotWidth, height: 1)
                        .position(x: padL + plotWidth / 2, y: y)
                }

                // Vertical hover guide through bar center
                if let hoveredIndex {
                    let centerX = padL + barCenterX(
                        index: hoveredIndex,
                        count: count,
                        plotWidth: plotWidth
                    )
                    Rectangle()
                        .fill(Color.primary.opacity(0.22))
                        .frame(width: 1, height: plotHeight)
                        .position(x: centerX, y: padT + plotHeight / 2)
                        .allowsHitTesting(false)
                }

                // Bars
                HStack(alignment: .bottom, spacing: barSpacing) {
                    ForEach(chartDays) { day in
                        barColumn(day: day, plotHeight: plotHeight, yMax: yMax)
                    }
                }
                .frame(width: plotWidth, height: plotHeight, alignment: .bottom)
                .offset(x: padL, y: padT)

                // Sparse X labels (ends + every other + hovered)
                HStack(spacing: barSpacing) {
                    ForEach(Array(chartDays.enumerated()), id: \.element.id) { index, day in
                        let isHovered = hoveredDate == day.date
                        let show = index == 0 || index == count - 1 || index % 2 == 1 || isHovered
                        Text(Formatting.chartDayLabel(isoDate: day.date))
                            .font(.system(size: 9).monospacedDigit())
                            .fontWeight(isHovered ? .bold : .regular)
                            .foregroundStyle(isHovered ? Color.primary : Color.secondary)
                            .frame(maxWidth: .infinity)
                            .opacity(show ? 1 : 0)
                            .accessibilityHidden(!show)
                    }
                }
                .frame(width: plotWidth, height: padB, alignment: .bottom)
                .offset(x: padL, y: height - padB)

                // Tooltip near hovered bar, clamped inside bounds
                if let hoveredIndex {
                    let day = chartDays[hoveredIndex]
                    let centerX = padL + barCenterX(
                        index: hoveredIndex,
                        count: count,
                        plotWidth: plotWidth
                    )
                    let clampedLeft = min(
                        max(8, centerX - tooltipWidth / 2),
                        max(8, geo.size.width - tooltipWidth - 8)
                    )
                    tooltip(for: day)
                        .offset(x: clampedLeft, y: 0)
                        .allowsHitTesting(false)
                        .zIndex(5)
                }
            }
        }
    }

    private func barColumn(day: DayUsage, plotHeight: CGFloat, yMax: Double) -> some View {
        let rawHeight = yMax > 0 ? plotHeight * CGFloat(day.cost / yMax) : 0
        let barHeight: CGFloat = day.cost > 0 ? max(2, rawHeight) : 0
        let isHovered = hoveredDate == day.date
        let anyHover = hoveredDate != nil
        let opacity: Double = {
            if isHovered { return 1 }
            if anyHover { return 0.28 }
            return 0.88
        }()

        return VStack(spacing: 0) {
            Spacer(minLength: 0)
            Rectangle()
                .fill(Color.primary)
                .frame(maxWidth: .infinity)
                .frame(height: barHeight)
                .opacity(opacity)
                .overlay {
                    if isHovered {
                        Rectangle()
                            .strokeBorder(Color.primary, lineWidth: 1)
                    }
                }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .contentShape(Rectangle())
        .onHover { inside in
            if inside {
                hoveredDate = day.date
            } else if hoveredDate == day.date {
                hoveredDate = nil
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(
            "\(day.date), \(Formatting.cost(day.cost)), \(Formatting.compactTokens(day.tokens)) tokens"
        )
    }

    private func tooltip(for day: DayUsage) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(day.date)
                .font(.system(size: 10))
                .foregroundStyle(.secondary)
                .tracking(0.8)

            HStack {
                Text("cost")
                    .foregroundStyle(.secondary)
                Spacer(minLength: 12)
                Text(Formatting.cost(day.cost))
                    .fontWeight(.semibold)
                    .monospacedDigit()
            }
            .font(.system(size: 12))

            HStack {
                Text("tokens")
                    .foregroundStyle(.secondary)
                Spacer(minLength: 12)
                Text(Formatting.compactTokens(day.tokens))
                    .monospacedDigit()
            }
            .font(.system(size: 12))
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .frame(width: tooltipWidth, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 4)
                .fill(Color(nsColor: .controlBackgroundColor))
                .shadow(color: .black.opacity(0.12), radius: 6, y: 2)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 4)
                .strokeBorder(Color.primary.opacity(0.12), lineWidth: 1)
        )
    }

    private func barCenterX(index: Int, count: Int, plotWidth: CGFloat) -> CGFloat {
        guard count > 0 else { return 0 }
        let totalSpacing = barSpacing * CGFloat(max(count - 1, 0))
        let barWidth = max(1, (plotWidth - totalSpacing) / CGFloat(count))
        return CGFloat(index) * (barWidth + barSpacing) + barWidth / 2
    }
}
