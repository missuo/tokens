import XCTest
@testable import TokensMenuBarCore

final class CostChartTests: XCTestCase {
    private func day(_ date: String, cost: Double, tokens: Int64 = 0) -> DayUsage {
        DayUsage(date: date, tokens: tokens, cost: cost, messages: 0, intensity: 0)
    }

    func testDaysForChart_takesLast14Ascending() {
        let input = (1...20).map { day(String(format: "2026-07-%02d", $0), cost: Double($0)) }
        let out = CostChartMath.daysForChart(from: input, limit: 14)
        XCTAssertEqual(out.count, 14)
        XCTAssertEqual(out.first?.date, "2026-07-07")
        XCTAssertEqual(out.last?.date, "2026-07-20")
    }

    func testDaysForChart_shortSeriesPassthrough() {
        let input = [day("2026-07-25", cost: 1), day("2026-07-26", cost: 2)]
        let out = CostChartMath.daysForChart(from: input, limit: 14)
        XCTAssertEqual(out.map(\.date), ["2026-07-25", "2026-07-26"])
    }

    func testYMax_ceils() {
        XCTAssertEqual(CostChartMath.yMax(costs: [1.2, 5.8, 3]), 6)
        XCTAssertEqual(CostChartMath.yMax(costs: [0, 0]), 1)
    }
}
