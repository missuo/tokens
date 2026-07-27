import XCTest
@testable import TokensMenuBarCore

final class FormattingTests: XCTestCase {
    func testCompactTokens() {
        XCTAssertEqual(Formatting.compactTokens(999), "999")
        XCTAssertEqual(Formatting.compactTokens(1_200), "1.2K")
        XCTAssertEqual(Formatting.compactTokens(1_200_000), "1.2M")
        XCTAssertEqual(Formatting.compactTokens(2_000_000_000), "2B")
    }

    func testCost() {
        XCTAssertEqual(Formatting.cost(0), "$0.00")
        XCTAssertEqual(Formatting.cost(0.004), "<$0.01")
        XCTAssertEqual(Formatting.cost(4.2), "$4.20")
    }

    func testChartDayLabel() {
        XCTAssertEqual(Formatting.chartDayLabel(isoDate: "2026-07-24"), "24")
        XCTAssertEqual(Formatting.chartDayLabel(isoDate: "2026-07-01"), "01")
        XCTAssertEqual(Formatting.chartDayLabel(isoDate: "short"), "short")
    }

    func testMenuBarTitleModes() {
        let report = UsageReport(
            schemaVersion: 1,
            generatedAt: "2026-07-26T00:00:00Z",
            period: "today",
            dateRange: DateRange(start: "2026-07-26", end: "2026-07-26"),
            scan: ScanInfo(mode: "snapshot", forceRescan: false, durationMs: 0, cache: ScanCacheInfo(sourceHits: 0, sourceMisses: 0, snapshotRebuilt: false)),
            summary: UsageSummary(totalTokens: 1_200_000, totalCost: 4.2, messages: 3, activeDays: 1, clients: [], models: []),
            tokenBreakdown: TokenBreakdown(input: 1, output: 1, cacheRead: 0, cacheWrite: 0, reasoning: 0),
            byClient: [],
            byModel: [],
            byDay: [],
            meta: UsageMeta(cliVersion: "1", timezone: "UTC")
        )
        XCTAssertEqual(Formatting.menuBarTitle(report: report, mode: .tokens, missingBinary: false, hasError: false), "1.2M")
        XCTAssertEqual(Formatting.menuBarTitle(report: report, mode: .cost, missingBinary: false, hasError: false), "$4.20")
        XCTAssertEqual(Formatting.menuBarTitle(report: report, mode: .both, missingBinary: false, hasError: false), "1.2M · $4.20")
        XCTAssertEqual(Formatting.menuBarTitle(report: nil, mode: .tokens, missingBinary: true, hasError: false), "tokens?")
    }

    func testDecodeUsageReportFixture() throws {
        let json = """
        {
          "schemaVersion": 1,
          "generatedAt": "2026-07-26T00:00:00Z",
          "period": "today",
          "dateRange": {"start": "2026-07-26", "end": "2026-07-26"},
          "scan": {"mode": "incremental", "forceRescan": false, "durationMs": 10, "cache": {"sourceHits": 0, "sourceMisses": 0, "snapshotRebuilt": true}},
          "summary": {"totalTokens": 1000, "totalCost": 1.5, "messages": 3, "activeDays": 1, "clients": ["claude"], "models": ["opus"]},
          "tokenBreakdown": {"input": 600, "output": 400, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0},
          "byClient": [{"client": "claude", "tokens": 1000, "cost": 1.5, "messages": 3, "share": 1.0, "models": [{"modelId": "opus", "providerId": "anthropic", "tokens": 1000, "cost": 1.5, "messages": 3, "share": 1.0}]}],
          "byModel": [{"modelId": "opus", "providerId": "anthropic", "tokens": 1000, "cost": 1.5, "messages": 3, "share": 1.0, "clients": ["claude"]}],
          "byDay": [{"date": "2026-07-26", "tokens": 1000, "cost": 1.5, "messages": 3, "intensity": 2}],
          "meta": {"cliVersion": "27.0.1", "timezone": "UTC"}
        }
        """.data(using: .utf8)!
        let report = try JSONDecoder().decode(UsageReport.self, from: json)
        XCTAssertEqual(report.summary.totalTokens, 1000)
        XCTAssertEqual(report.byClient.first?.models.first?.modelId, "opus")
    }
}
