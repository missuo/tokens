import XCTest
@testable import TokensMenuBarCore

final class CostChartTests: XCTestCase {
    func testGeometryRetainsOneSlotPerBucketIncludingZeroes() {
        let geometry = CostChartMath.geometry(
            plotWidth: 330,
            bucketCount: 30,
            preferredSpacing: 3
        )

        XCTAssertEqual(geometry.bucketCount, 30)
        XCTAssertGreaterThanOrEqual(geometry.barWidth, 1)
        XCTAssertEqual(geometry.centerX(for: 0), geometry.barWidth / 2, accuracy: 0.001)
        XCTAssertLessThan(geometry.centerX(for: 29), 330)
    }

    func testBoundaryUsesSelectionStartAndOnlyAppearsWithLeadingContext() {
        let withContext = [
            bucket(
                id: "context-1",
                start: "2026-08-03T22:00:00-07:00",
                end: "2026-08-03T23:00:00-07:00",
                contextOnly: true
            ),
            bucket(
                id: "context-2",
                start: "2026-08-03T23:00:00-07:00",
                end: "2026-08-04T00:00:00-07:00",
                contextOnly: true
            ),
            bucket(
                id: "selected",
                start: "2026-08-04T00:00:00-07:00",
                end: "2026-08-04T01:00:00-07:00",
                contextOnly: false
            ),
        ]
        XCTAssertEqual(
            CostChartMath.selectionBoundaryIndex(
                in: withContext,
                selectionStart: "2026-08-04T00:00:00-07:00"
            ),
            2
        )
        XCTAssertNil(
            CostChartMath.selectionBoundaryIndex(
                in: withContext,
                selectionStart: "2026-08-03T23:00:00-07:00"
            )
        )

        let withoutContext = [
            bucket(id: "selected-1", contextOnly: false),
            bucket(id: "selected-2", contextOnly: false),
        ]
        XCTAssertNil(
            CostChartMath.selectionBoundaryIndex(
                in: withoutContext,
                selectionStart: "2026-08-04T00:00:00Z"
            )
        )
    }

    func testSparseLabelIndicesAreDistributedAcrossFullRange() {
        XCTAssertEqual(CostChartMath.labelIndices(bucketCount: 0, maximumLabels: 6), [])
        XCTAssertEqual(CostChartMath.labelIndices(bucketCount: 1, maximumLabels: 6), [0])
        XCTAssertEqual(CostChartMath.labelIndices(bucketCount: 12, maximumLabels: 6), [0, 2, 4, 7, 9, 11])
        XCTAssertEqual(CostChartMath.labelIndices(bucketCount: 7, maximumLabels: 5), [0, 2, 3, 5, 6])
        XCTAssertEqual(CostChartMath.labelIndices(bucketCount: 5, maximumLabels: 6), [0, 1, 2, 3, 4])
    }

    func testYMaxAndTicksPreserveSubDollarPrecision() {
        XCTAssertEqual(CostChartMath.yMax(costs: [1.2, 5.8, 3]), 6)
        XCTAssertEqual(CostChartMath.yMax(costs: [0.12, 0.5]), 0.5, accuracy: 0.0001)
        XCTAssertEqual(CostChartMath.yMax(costs: [0, 0]), 1)
        XCTAssertEqual(CostChartMath.yTicks(maximum: 0.5), [0, 0.25, 0.5])
        XCTAssertEqual(
            CostChartMath.yTicks(maximum: 1).map(Formatting.chartCostTick),
            ["$0", "$0.50", "$1"]
        )
        XCTAssertEqual(
            CostChartMath.yTicks(maximum: 0.01).map(Formatting.chartCostTick),
            ["$0", "$0.005", "$0.01"]
        )
        XCTAssertEqual(
            CostChartMath.yTicks(maximum: 0.0001).map(Formatting.chartCostTick),
            ["$0", "$0.00005", "$0.0001"]
        )
    }

    func testStaleHoveredBucketDoesNotCountAsActiveHover() {
        let buckets = [bucket(id: "new", contextOnly: false)]
        XCTAssertEqual(CostChartMath.hoveredIndex(bucketID: "new", in: buckets), 0)
        XCTAssertNil(CostChartMath.hoveredIndex(bucketID: "old", in: buckets))
        XCTAssertNil(CostChartMath.hoveredIndex(bucketID: nil, in: buckets))
    }

    private func bucket(
        id: String,
        start: String = "2026-08-04T00:00:00Z",
        end: String = "2026-08-04T01:00:00Z",
        contextOnly: Bool
    ) -> UsageTimeBucket {
        UsageTimeBucket(
            id: id,
            nominalStart: start,
            nominalEndExclusive: end,
            coveredStart: start,
            coveredEndExclusive: end,
            totals: UsageTotals(tokens: 0, cost: 0, messages: 0),
            contextOnly: contextOnly,
            incompleteEdge: false,
            active: false
        )
    }
}
