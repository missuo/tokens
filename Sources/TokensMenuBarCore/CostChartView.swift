import AppKit
import SwiftUI

/// Generic report-v3 cost buckets with supplied context/edge semantics.
public struct CostChartView: View {
    public let timeSeries: UsageTimeSeries
    public let timeZone: TimeZone
    public var height: CGFloat

    private let yMax: Double
    private let ticks: [Double]
    private let labels: [String]
    private let visibleLabels: Set<Int>
    private let accessibilityLabels: [String: String]

    @Environment(\.colorScheme) private var colorScheme
    @State private var hoveredBucketID: String?

    private let padL: CGFloat = 34
    private let padR: CGFloat = 4
    private let padT: CGFloat = 8
    private let padB: CGFloat = 24
    private let preferredBarSpacing: CGFloat = 3
    private let tooltipWidth: CGFloat = 196

    public init(
        timeSeries: UsageTimeSeries,
        timeZone: TimeZone,
        height: CGFloat = MenuBarLayout.chartHeight
    ) {
        self.timeSeries = timeSeries
        self.timeZone = timeZone
        self.height = height

        let costs = timeSeries.buckets.map(\.totals.cost)
        let yMax = CostChartMath.yMax(costs: costs)
        self.yMax = yMax
        self.ticks = CostChartMath.yTicks(maximum: yMax)
        self.labels = Formatting.chartBucketLabels(
            buckets: timeSeries.buckets,
            granularity: timeSeries.granularity,
            timeZone: timeZone
        )
        self.visibleLabels = CostChartMath.labelIndices(
            bucketCount: timeSeries.buckets.count,
            maximumLabels: timeSeries.granularity == .hour ? 6 : 5
        )
        self.accessibilityLabels = Dictionary(
            uniqueKeysWithValues: timeSeries.buckets.map { bucket in
                (
                    bucket.id,
                    Formatting.chartBucketAccessibilityLabel(
                        bucket,
                        timeZone: timeZone
                    )
                )
            }
        )
    }

    private var barColor: Color {
        colorScheme == .dark
            ? Color.white.opacity(0.92)
            : Color.black.opacity(0.88)
    }

    private var contextColor: Color {
        colorScheme == .dark
            ? Color.white.opacity(0.24)
            : Color.black.opacity(0.20)
    }

    private var dimBarColor: Color {
        colorScheme == .dark
            ? Color.white.opacity(0.18)
            : Color.black.opacity(0.14)
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

    private var tooltipBackground: Color { Color(nsColor: .windowBackgroundColor) }

    public var body: some View {
        Group {
            if timeSeries.buckets.isEmpty {
                Text("No \(timeSeries.granularity.title.lowercased()) cost data")
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                chartBody
            }
        }
        .frame(maxWidth: .infinity)
        .frame(height: height)
        .onChange(of: timeSeries.buckets.map(\.id)) { bucketIDs in
            if let hoveredBucketID, !bucketIDs.contains(hoveredBucketID) {
                self.hoveredBucketID = nil
            }
        }
    }

    private var chartBody: some View {
        let buckets = timeSeries.buckets
        let hoveredIndex = CostChartMath.hoveredIndex(
            bucketID: hoveredBucketID,
            in: buckets
        )

        return GeometryReader { geo in
            let plotWidth = max(0, geo.size.width - padL - padR)
            let plotHeight = max(0, geo.size.height - padT - padB)
            let geometry = CostChartMath.geometry(
                plotWidth: Double(plotWidth),
                bucketCount: buckets.count,
                preferredSpacing: Double(preferredBarSpacing)
            )

            ZStack(alignment: .topLeading) {
                RoundedRectangle(cornerRadius: 4, style: .continuous)
                    .fill(plotBackground)
                    .frame(width: plotWidth, height: plotHeight)
                    .offset(x: padL, y: padT)

                ForEach(Array(ticks.enumerated()), id: \.offset) { _, value in
                    let y = padT + plotHeight * (1 - CGFloat(value / yMax))
                    Text(Formatting.chartCostTick(value))
                        .font(.system(size: 9, design: .monospaced).monospacedDigit())
                        .foregroundStyle(.secondary)
                        .frame(width: padL - 6, alignment: .trailing)
                        .position(x: (padL - 6) / 2, y: y)

                    Rectangle()
                        .fill(value == 0 ? baselineColor : gridColor)
                        .frame(width: plotWidth, height: 1)
                        .offset(x: padL, y: y)
                }

                Canvas { context, _ in
                    for (index, bucket) in buckets.enumerated() {
                        let rawHeight = plotHeight * CGFloat(bucket.totals.cost / yMax)
                        let barHeight: CGFloat = bucket.totals.cost > 0 ? max(2, rawHeight) : 1
                        let x = padL + CGFloat(geometry.leadingX(for: index))
                        let y = padT + plotHeight - barHeight
                        let rect = CGRect(
                            x: x,
                            y: y,
                            width: CGFloat(geometry.barWidth),
                            height: barHeight
                        )
                        let path = barPath(rect)
                        let anyHover = hoveredIndex != nil
                        let isHovered = hoveredBucketID == bucket.id
                        let base = bucket.contextOnly ? contextColor : barColor
                        let fill = isHovered ? barColor : (anyHover ? dimBarColor : base)

                        context.fill(path, with: .color(fill))

                        if bucket.incompleteEdge, barHeight > 2 {
                            context.drawLayer { layer in
                                layer.clip(to: path)
                                let stripe = colorScheme == .dark
                                    ? Color.black.opacity(0.34)
                                    : Color.white.opacity(0.46)
                                var diagonal = -rect.height
                                while diagonal < rect.width + rect.height {
                                    var stripePath = Path()
                                    stripePath.move(to: CGPoint(x: rect.minX + diagonal, y: rect.maxY))
                                    stripePath.addLine(to: CGPoint(x: rect.minX + diagonal + rect.height, y: rect.minY))
                                    layer.stroke(stripePath, with: .color(stripe), lineWidth: 1)
                                    diagonal += 5
                                }
                            }
                        } else if bucket.incompleteEdge {
                            var edgePath = Path()
                            edgePath.move(to: CGPoint(x: rect.minX, y: rect.maxY))
                            edgePath.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY - 4))
                            context.stroke(edgePath, with: .color(barColor), lineWidth: 1)
                        }

                        if bucket.active || isHovered {
                            context.stroke(
                                path,
                                with: .color(barColor),
                                lineWidth: bucket.active ? 2 : 1
                            )
                        }
                    }

                    if let boundary = CostChartMath.selectionBoundaryIndex(
                        in: buckets,
                        selectionStart: timeSeries.selectionStart
                    ) {
                        let x = padL + CGFloat(geometry.leadingX(for: boundary))
                            - CGFloat(geometry.spacing) / 2
                        var path = Path()
                        path.move(to: CGPoint(x: x, y: padT))
                        path.addLine(to: CGPoint(x: x, y: padT + plotHeight))
                        context.stroke(
                            path,
                            with: .color(baselineColor),
                            style: StrokeStyle(lineWidth: 1, dash: [4, 3])
                        )
                    }
                }
                .frame(width: geo.size.width, height: geo.size.height)
                .allowsHitTesting(false)

                HStack(alignment: .bottom, spacing: 0) {
                    ForEach(Array(buckets.enumerated()), id: \.element.id) { index, bucket in
                        Color.clear
                            .frame(
                                width: hitWidth(index: index, geometry: geometry),
                                height: plotHeight
                            )
                            .contentShape(Rectangle())
                            .onHover { inside in
                                if inside {
                                    hoveredBucketID = bucket.id
                                } else if hoveredBucketID == bucket.id {
                                    hoveredBucketID = nil
                                }
                            }
                            .accessibilityElement(children: .ignore)
                            .accessibilityLabel(accessibilityLabels[bucket.id] ?? bucket.id)
                    }
                }
                .frame(width: plotWidth, height: plotHeight, alignment: .leading)
                .offset(x: padL, y: padT)

                if let hoveredIndex {
                    let centerX = padL + CGFloat(geometry.centerX(for: hoveredIndex))
                    Rectangle()
                        .fill(baselineColor)
                        .frame(width: 1, height: plotHeight)
                        .offset(x: centerX, y: padT)
                        .allowsHitTesting(false)
                }

                ForEach(Array(buckets.enumerated()), id: \.element.id) { index, bucket in
                    let show = visibleLabels.contains(index) || hoveredBucketID == bucket.id
                    if show {
                        Text(labels[index])
                            .font(.system(size: 9, design: .monospaced).monospacedDigit())
                            .fontWeight(hoveredBucketID == bucket.id ? .bold : .regular)
                            .foregroundStyle(hoveredBucketID == bucket.id ? Color.primary : Color.secondary)
                            .lineLimit(1)
                            .fixedSize()
                            .position(
                                x: padL + CGFloat(geometry.centerX(for: index)),
                                y: geo.size.height - padB / 2 + 2
                            )
                    }
                }

                if let hoveredIndex {
                    let bucket = buckets[hoveredIndex]
                    let centerX = padL + CGFloat(geometry.centerX(for: hoveredIndex))
                    let clampedLeft = min(
                        max(8, centerX - tooltipWidth / 2),
                        max(8, geo.size.width - tooltipWidth - 8)
                    )
                    tooltip(for: bucket)
                        .offset(x: clampedLeft, y: 4)
                        .allowsHitTesting(false)
                        .zIndex(5)
                }
            }
        }
    }

    private func hitWidth(index: Int, geometry: CostChartMath.Geometry) -> CGFloat {
        let left = index == 0 ? 0 : geometry.spacing / 2
        let right = index == geometry.bucketCount - 1 ? 0 : geometry.spacing / 2
        return CGFloat(geometry.barWidth + left + right)
    }

    private func barPath(_ rect: CGRect) -> Path {
        let radius = min(4, rect.width / 2, rect.height)
        guard radius > 0 else { return Path(rect) }
        var path = Path()
        path.move(to: CGPoint(x: rect.minX, y: rect.maxY))
        path.addLine(to: CGPoint(x: rect.minX, y: rect.minY + radius))
        path.addQuadCurve(
            to: CGPoint(x: rect.minX + radius, y: rect.minY),
            control: CGPoint(x: rect.minX, y: rect.minY)
        )
        path.addLine(to: CGPoint(x: rect.maxX - radius, y: rect.minY))
        path.addQuadCurve(
            to: CGPoint(x: rect.maxX, y: rect.minY + radius),
            control: CGPoint(x: rect.maxX, y: rect.minY)
        )
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY))
        path.closeSubpath()
        return path
    }

    private func tooltip(for bucket: UsageTimeBucket) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(Formatting.chartBucketTooltipRange(bucket, timeZone: timeZone))
                .font(.system(size: 9, design: .monospaced))
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)

            HStack {
                Text("cost").foregroundStyle(.secondary)
                Spacer(minLength: 12)
                Text(Formatting.cost(bucket.totals.cost))
                    .fontWeight(.semibold)
                    .monospacedDigit()
            }
            HStack {
                Text("tokens").foregroundStyle(.secondary)
                Spacer(minLength: 12)
                Text(Formatting.compactTokens(bucket.totals.tokens)).monospacedDigit()
            }
            if bucket.contextOnly {
                Text("CONTEXT · EXCLUDED FROM TOTAL")
                    .foregroundStyle(.secondary)
            }
            if bucket.active {
                Text("ACTIVE")
                    .fontWeight(.semibold)
            }
            if bucket.incompleteEdge {
                Text("INCOMPLETE CALENDAR EDGE")
                    .foregroundStyle(.secondary)
            }
        }
        .font(.system(size: 11, design: .monospaced))
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
