import XCTest
@testable import TokensMenuBarCore

final class ReportV3Tests: XCTestCase {
    func testDecodesApprovedTodayFixture() throws {
        let report = try decodeFixture("report-v3-today.json")

        XCTAssertEqual(report.schemaVersion, 3)
        XCTAssertEqual(report.selection, .preset(.today))
        XCTAssertEqual(report.dateRange.startDate, "2026-08-04")
        XCTAssertEqual(report.dateRange.endDate, "2026-08-04")
        XCTAssertEqual(report.dateRange.timezone, "America/Los_Angeles")
        XCTAssertEqual(report.scan.cache.snapshotSchemaVersion, 3)
        XCTAssertEqual(report.meta.reportContract, "v3")
        XCTAssertEqual(report.byClient.first?.models.first?.modelId, "claude-sonnet-5")
        XCTAssertEqual(report.timeSeries.granularity, .hour)
        XCTAssertEqual(report.timeSeries.buckets.count, 12)
        XCTAssertEqual(report.timeSeries.buckets.filter(\.contextOnly).count, 10)
        XCTAssertEqual(report.timeSeries.buckets.last?.active, true)
        XCTAssertEqual(report.timeSeries.unplaced.tokens, 12_000)
        XCTAssertEqual(report.timeSeries.unplaced.cost, 0.18, accuracy: 0.000_001)

        // Weekday × hour grid: 2026-08-04 is a Tuesday with two placed hours.
        XCTAssertEqual(report.weekdayHour?.count, 168)
        let tuesdayZero = report.weekdayHour?.first { $0.weekday == 2 && $0.hour == 0 }
        XCTAssertEqual(tuesdayZero?.tokens, 120_000)
        XCTAssertEqual(tuesdayZero?.cost ?? -1, 1.82, accuracy: 0.000_001)
        let placedTokens = report.weekdayHour?.reduce(Int64(0)) { $0 + $1.tokens } ?? -1
        XCTAssertEqual(placedTokens + report.timeSeries.unplaced.tokens, report.summary.totalTokens)
    }

    func testDecodesApprovedNaturalWeekAndCustomFixtures() throws {
        let thirtyDays = try decodeFixture("report-v3-30d.json")
        XCTAssertEqual(thirtyDays.selection, .preset(.days30))
        XCTAssertEqual(thirtyDays.timeSeries.granularity, .naturalWeek)
        XCTAssertEqual(thirtyDays.timeSeries.buckets.last?.incompleteEdge, true)
        XCTAssertEqual(thirtyDays.timeSeries.buckets.last?.active, true)

        let custom = try decodeFixture("report-v3-custom-historical.json")
        XCTAssertEqual(
            custom.selection,
            .custom(DateSelectionRange(startDate: "2026-06-01", endDate: "2026-06-05"))
        )
        XCTAssertEqual(custom.timeSeries.granularity, .day)
        XCTAssertEqual(custom.timeSeries.buckets.count, 5)
        XCTAssertTrue(custom.timeSeries.buckets.allSatisfy { !$0.contextOnly })
        // Custom fixture weekdayHour conserves the summary (same filled totals as 30d).
        XCTAssertEqual(custom.weekdayHour?.count, 168)
        let customPlacedTokens = custom.weekdayHour?.reduce(Int64(0)) { $0 + $1.tokens } ?? -1
        XCTAssertEqual(
            customPlacedTokens + custom.timeSeries.unplaced.tokens,
            custom.summary.totalTokens
        )
        XCTAssertEqual(thirtyDays.weekdayHour?.count, 168)
        let thirtyPlacedTokens = thirtyDays.weekdayHour?.reduce(Int64(0)) { $0 + $1.tokens } ?? -1
        XCTAssertEqual(
            thirtyPlacedTokens + thirtyDays.timeSeries.unplaced.tokens,
            thirtyDays.summary.totalTokens
        )
    }

    func testSelectedRollupExcludesContextAndIncludesUnplaced() throws {
        let report = try decodeFixture("report-v3-today.json")
        let selected = report.timeSeries.buckets.filter { !$0.contextOnly }
        let bucketTokens = selected.reduce(Int64(0)) { $0 + $1.totals.tokens }
        let bucketCost = selected.reduce(0.0) { $0 + $1.totals.cost }
        let bucketMessages = selected.reduce(Int32(0)) { $0 + $1.totals.messages }

        XCTAssertEqual(bucketTokens + report.timeSeries.unplaced.tokens, report.summary.totalTokens)
        XCTAssertEqual(bucketCost + report.timeSeries.unplaced.cost, report.summary.totalCost, accuracy: 0.000_001)
        XCTAssertEqual(bucketMessages + report.timeSeries.unplaced.messages, report.summary.messages)
    }

    private func decodeFixture(_ name: String) throws -> UsageReport {
        let testsDirectory = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
        let fixture = testsDirectory
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("docs/wayfinder/time-range-cost-chart/prototypes/report-cache-contract/fixtures")
            .appendingPathComponent(name)
        let data = try Data(contentsOf: fixture)
        return try JSONDecoder().decode(UsageReport.self, from: data)
    }
}
