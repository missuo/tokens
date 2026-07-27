import AppKit
import SwiftUI

/// 14-day cost bar chart with hover dimming, guide line, and tooltip (IX-A).
public struct CostChartView: View {
    public let days: [DayUsage]
    public var height: CGFloat
    /// Selected period raw value; hover clears when the period changes.
    public var periodRawValue: String

    @Environment(\.colorScheme) private var colorScheme
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

    /// Explicit mono fills — `Color.primary` inside a vibrant NSPopover can wash out to a solid white plate.
    private var barColor: Color {
        colorScheme == .dark
            ? Color.white.opacity(0.92)
            : Color.black.opacity(0.88)
    }

    private var dimBarColor: Color {
        colorScheme == .dark
            ? Color.white.opacity(0.28)
            : Color.black.opacity(0.24)
    }

    private var gridColor: Color {
        colorScheme == .dark
            ? Color.white.opacity(0.12)
            : Color.black.opacity(0.10)
    }

    private var baselineColor: Color {
        colorScheme == .dark
            ? Color.white.opacity(0.28)
            : Color.black.opacity(0.22)
    }

    private var plotBackground: Color {
        colorScheme == .dark
            ? Color.white.opacity(0.04)
            : Color.black.opacity(0.03)
    }

    private var tooltipBackground: Color {
        Color(nsColor: .windowBackgroundColor)
    }

    public var body: some View {
        let chartDays = CostChartMath.daysForChart(from: days)
        Group {
            if chartDays.isEmpty {
                Text("No daily data")
                    .font(.system(size: 11, design: .monospaced))
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
            let plotHeight = max(0, geo.size.height - padT - padB)
            let totalSpacing = barSpacing * CGFloat(max(count - 1, 0))
            let barWidth = count > 0 ? max(1, (plotWidth - totalSpacing) / CGFloat(count)) : 1

            ZStack(alignment: .topLeading) {
                // Plot well — keeps bars visible against both light and dark popover materials.
                RoundedRectangle(cornerRadius: 4, style: .continuous)
                    .fill(plotBackground)
                    .frame(width: plotWidth, height: plotHeight)
                    .offset(x: padL, y: padT)

                // Y-axis labels + grid lines
                ForEach(Array(ticks.enumerated()), id: \.offset) { _, value in
                    let y = padT + plotHeight * (1 - CGFloat(value / yMax))
                    Text("$\(Int(value.rounded()))")
                        .font(.system(size: 9, design: .monospaced).monospacedDigit())
                        .foregroundStyle(.secondary)
                        .frame(width: padL - 6, alignment: .trailing)
                        .position(x: (padL - 6) / 2, y: y)

                    Rectangle()
                        .fill(value == 0 ? baselineColor : gridColor)
                        .frame(width: plotWidth, height: 1)
                        .offset(x: padL, y: y)
                }

                // Bars (Canvas avoids vibrant-material fill washout of Shape styles)
                Canvas { context, _ in
                    for (index, day) in chartDays.enumerated() {
                        let rawHeight = yMax > 0 ? plotHeight * CGFloat(day.cost / yMax) : 0
                        let barHeight: CGFloat = day.cost > 0 ? max(2, rawHeight) : 0
                        guard barHeight > 0 else { continue }

                        let x = padL + CGFloat(index) * (barWidth + barSpacing)
                        let y = padT + plotHeight - barHeight
                        let rect = CGRect(x: x, y: y, width: barWidth, height: barHeight)

                        let isHovered = hoveredDate == day.date
                        let anyHover = hoveredDate != nil
                        let fill: Color = {
                            if isHovered { return barColor }
                            if anyHover { return dimBarColor }
                            return barColor.opacity(0.95)
                        }()

                        context.fill(Path(rect), with: .color(fill))
                        if isHovered {
                            context.stroke(
                                Path(rect.insetBy(dx: 0.5, dy: 0.5)),
                                with: .color(barColor),
                                lineWidth: 1
                            )
                        }
                    }
                }
                .frame(width: geo.size.width, height: geo.size.height)
                .allowsHitTesting(false)

                // Invisible hit targets per bar (Canvas is not hoverable)
                HStack(alignment: .bottom, spacing: barSpacing) {
                    ForEach(chartDays) { day in
                        Color.clear
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
                }
                .frame(width: plotWidth, height: plotHeight)
                .offset(x: padL, y: padT)

                // Vertical hover guide
                if let hoveredIndex {
                    let centerX = padL + CGFloat(hoveredIndex) * (barWidth + barSpacing) + barWidth / 2
                    Rectangle()
                        .fill(baselineColor)
                        .frame(width: 1, height: plotHeight)
                        .offset(x: centerX, y: padT)
                        .allowsHitTesting(false)
                }

                // Sparse X labels
                HStack(spacing: barSpacing) {
                    ForEach(Array(chartDays.enumerated()), id: \.element.id) { index, day in
                        let isHovered = hoveredDate == day.date
                        let show = index == 0 || index == count - 1 || index % 2 == 1 || isHovered
                        Text(Formatting.chartDayLabel(isoDate: day.date))
                            .font(.system(size: 9, design: .monospaced).monospacedDigit())
                            .fontWeight(isHovered ? .bold : .regular)
                            .foregroundStyle(isHovered ? Color.primary : Color.secondary)
                            .frame(maxWidth: .infinity)
                            .opacity(show ? 1 : 0)
                            .accessibilityHidden(!show)
                    }
                }
                .frame(width: plotWidth, height: padB, alignment: .bottom)
                .offset(x: padL, y: geo.size.height - padB)

                // Tooltip
                if let hoveredIndex {
                    let day = chartDays[hoveredIndex]
                    let centerX = padL + CGFloat(hoveredIndex) * (barWidth + barSpacing) + barWidth / 2
                    let clampedLeft = min(
                        max(8, centerX - tooltipWidth / 2),
                        max(8, geo.size.width - tooltipWidth - 8)
                    )
                    tooltip(for: day)
                        .offset(x: clampedLeft, y: 4)
                        .allowsHitTesting(false)
                        .zIndex(5)
                }
            }
        }
    }

    private func tooltip(for day: DayUsage) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(day.date)
                .font(.system(size: 10, design: .monospaced))
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
            .font(.system(size: 12, design: .monospaced))

            HStack {
                Text("tokens")
                    .foregroundStyle(.secondary)
                Spacer(minLength: 12)
                Text(Formatting.compactTokens(day.tokens))
                    .monospacedDigit()
            }
            .font(.system(size: 12, design: .monospaced))
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .frame(width: tooltipWidth, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 4)
                .fill(tooltipBackground)
                .shadow(color: .black.opacity(0.18), radius: 6, y: 2)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 4)
                .strokeBorder(Color.primary.opacity(0.14), lineWidth: 1)
        )
    }
}
