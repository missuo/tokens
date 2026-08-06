import XCTest
@testable import TokensMenuBarCore

final class HeatmapTests: XCTestCase {
    private func makeCell(
        _ weekday: Int,
        _ hour: Int,
        tokens: Int64 = 0,
        cost: Double = 0,
        messages: Int32 = 0
    ) -> UsageWeekdayHourCell {
        UsageWeekdayHourCell(
            weekday: weekday,
            hour: hour,
            tokens: tokens,
            cost: cost,
            messages: messages
        )
    }

    func testCellLookupFindsTheRequestedPair() {
        let cells = [makeCell(2, 21, tokens: 100, cost: 1.5), makeCell(5, 9, tokens: 50, cost: 0.5)]

        XCTAssertEqual(HeatmapMath.cell(weekday: 2, hour: 21, in: cells)?.tokens, 100)
        XCTAssertNil(HeatmapMath.cell(weekday: 3, hour: 21, in: cells))
    }

    func testPeakIgnoresZeroCostCellsAndPicksHighestCost() {
        let cells = [
            makeCell(1, 9, tokens: 5_000, cost: 0),
            makeCell(3, 22, tokens: 100, cost: 4.2),
            makeCell(6, 14, tokens: 9_000, cost: 1.0),
        ]

        let peak = HeatmapMath.peak(in: cells)
        XCTAssertEqual(peak?.weekday, 3)
        XCTAssertEqual(peak?.hour, 22)
    }

    func testPeakIsNilWhenEveryCellIsZeroOrEmpty() {
        XCTAssertNil(HeatmapMath.peak(in: []))
        XCTAssertNil(HeatmapMath.peak(in: [makeCell(1, 0), makeCell(7, 23)]))
    }

    func testPeakTieResolvesToEarliestWeekdayHour() {
        let cells = [makeCell(4, 10, tokens: 10, cost: 2.0), makeCell(2, 21, tokens: 10, cost: 2.0)]

        let peak = HeatmapMath.peak(in: cells)
        XCTAssertEqual(peak?.weekday, 4)
        XCTAssertEqual(peak?.hour, 10)
    }

    func testIntensityIsSquareRootScaledAndZeroSafe() {
        XCTAssertEqual(HeatmapMath.intensity(cost: 0, maximum: 4), 0)
        XCTAssertEqual(HeatmapMath.intensity(cost: 2, maximum: 0), 0)
        XCTAssertEqual(HeatmapMath.intensity(cost: 4, maximum: 4), 1)
        XCTAssertEqual(HeatmapMath.intensity(cost: 1, maximum: 4), 0.5, accuracy: 0.000_001)
        // Never exceeds 1 even when a cell beats the stated maximum.
        XCTAssertEqual(HeatmapMath.intensity(cost: 16, maximum: 4), 1)
    }

    func testCellOpacityKeepsNonZeroCellsAboveTheFloor() {
        XCTAssertEqual(HeatmapMath.cellOpacity(cost: 0, maximum: 4), 0)
        XCTAssertEqual(HeatmapMath.cellOpacity(cost: 4, maximum: 4), 1, accuracy: 0.000_001)
        // Quarter of the peak → intensity 0.5 → opacity 0.6.
        XCTAssertEqual(HeatmapMath.cellOpacity(cost: 1, maximum: 4), 0.6, accuracy: 0.000_001)
    }

    func testWeekdayLabelsFollowISOOrder() {
        XCTAssertEqual(HeatmapMath.weekdayShortLabels.count, HeatmapMath.weekdayCount)
        XCTAssertEqual(HeatmapMath.weekdayLabel(1), "MON")
        XCTAssertEqual(HeatmapMath.weekdayLabel(7), "SUN")
        XCTAssertEqual(HeatmapMath.weekdayLabel(0), "?")
        XCTAssertEqual(HeatmapMath.weekdayLabel(8), "?")
    }

    func testHourRangeLabelWrapsAtMidnight() {
        XCTAssertEqual(HeatmapMath.hourRangeLabel(hour: 0), "00:00–01:00")
        XCTAssertEqual(HeatmapMath.hourRangeLabel(hour: 21), "21:00–22:00")
        XCTAssertEqual(HeatmapMath.hourRangeLabel(hour: 23), "23:00–00:00")
    }
}
