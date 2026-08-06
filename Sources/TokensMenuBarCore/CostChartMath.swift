import Foundation

public enum CostChartMath {
    public struct Geometry: Equatable {
        public let bucketCount: Int
        public let barWidth: Double
        public let spacing: Double

        public func leadingX(for index: Int) -> Double {
            Double(index) * (barWidth + spacing)
        }

        public func centerX(for index: Int) -> Double {
            leadingX(for: index) + barWidth / 2
        }
    }

    public static func geometry(
        plotWidth: Double,
        bucketCount: Int,
        preferredSpacing: Double,
        maximumBarWidth: Double = 24
    ) -> Geometry {
        guard bucketCount > 0, plotWidth > 0 else {
            return Geometry(
                bucketCount: max(0, bucketCount),
                barWidth: 0,
                spacing: 0
            )
        }
        if bucketCount == 1 {
            return Geometry(
                bucketCount: 1,
                barWidth: min(maximumBarWidth, plotWidth),
                spacing: 0
            )
        }

        let preferredTotalSpacing = preferredSpacing * Double(bucketCount - 1)
        let fitted = max(1, (plotWidth - preferredTotalSpacing) / Double(bucketCount))
        let barWidth = min(maximumBarWidth, fitted)
        let remaining = max(0, plotWidth - barWidth * Double(bucketCount))
        let spacing = remaining / Double(bucketCount - 1)
        return Geometry(
            bucketCount: bucketCount,
            barWidth: barWidth,
            spacing: spacing
        )
    }

    public static func selectionBoundaryIndex(
        in buckets: [UsageTimeBucket],
        selectionStart: String
    ) -> Int? {
        guard let firstSelected = buckets.firstIndex(where: { !$0.contextOnly }),
              firstSelected > 0,
              let selectionDate = Formatting.parseISO8601(selectionStart),
              let bucketDate = Formatting.parseISO8601(buckets[firstSelected].coveredStart),
              selectionDate == bucketDate else {
            return nil
        }
        return firstSelected
    }

    public static func labelIndices(bucketCount: Int, maximumLabels: Int) -> Set<Int> {
        guard bucketCount > 0, maximumLabels > 0 else { return [] }
        guard bucketCount > maximumLabels else { return Set(0..<bucketCount) }
        guard maximumLabels > 1 else { return [bucketCount - 1] }

        let last = Double(bucketCount - 1)
        let intervals = Double(maximumLabels - 1)
        return Set((0..<maximumLabels).map { slot in
            Int((Double(slot) * last / intervals).rounded())
        })
    }

    public static func hoveredIndex(
        bucketID: String?,
        in buckets: [UsageTimeBucket]
    ) -> Int? {
        guard let bucketID else { return nil }
        return buckets.firstIndex(where: { $0.id == bucketID })
    }

    public static func yMax(costs: [Double]) -> Double {
        let maximum = costs.max() ?? 0
        if maximum <= 0 { return 1 }
        if maximum >= 1 { return ceil(maximum) }

        let magnitude = pow(10, floor(log10(maximum)))
        let normalized = maximum / magnitude
        let step: Double
        if normalized <= 1 {
            step = 1
        } else if normalized <= 2 {
            step = 2
        } else if normalized <= 5 {
            step = 5
        } else {
            step = 10
        }
        return step * magnitude
    }

    public static func yTicks(maximum: Double) -> [Double] {
        [0, maximum / 2, maximum]
    }
}
