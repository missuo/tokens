import XCTest
@testable import TokensMenuBarCore

final class FormattingTests: XCTestCase {
    func testCompactTokens() {
        let en = Locale(identifier: "en_US")
        XCTAssertEqual(Formatting.compactTokens(999, locale: en), "999")
        XCTAssertEqual(Formatting.compactTokens(1_200, locale: en), "1.2k")
        XCTAssertEqual(Formatting.compactTokens(1_200_000, locale: en), "1.2m")
        XCTAssertEqual(Formatting.compactTokens(2_000_000_000, locale: en), "2b")
    }

    func testCompactTokensLocaleAware() {
        let zh = Locale(identifier: "zh_Hans_CN")
        XCTAssertEqual(Formatting.compactTokens(999, locale: zh), "999")
        XCTAssertEqual(Formatting.compactTokens(1_200, locale: zh), "1200")
        XCTAssertEqual(Formatting.compactTokens(1_200_000, locale: zh), "120万")
        XCTAssertEqual(Formatting.compactTokens(2_000_000_000, locale: zh), "20亿")
    }

    func testCost() {
        let en = Locale(identifier: "en_US")
        XCTAssertEqual(Formatting.cost(0, locale: en), "$0.00")
        XCTAssertEqual(Formatting.cost(0.004, locale: en), "<$0.01")
        XCTAssertEqual(Formatting.cost(4.2, locale: en), "$4.20")
        XCTAssertEqual(Formatting.cost(999.99, locale: en), "$999.99")
    }

    func testCostCompactForLargeAmounts() {
        let en = Locale(identifier: "en_US")
        XCTAssertEqual(Formatting.cost(1000, locale: en), "$1k")
        XCTAssertEqual(Formatting.cost(24_128.26, locale: en), "$24k")
        XCTAssertEqual(Formatting.cost(1_200_000, locale: en), "$1.2m")
        XCTAssertEqual(Formatting.cost(2_000_000_000, locale: en), "$2b")
    }

    func testCostCompactLocaleAware() {
        let zh = Locale(identifier: "zh_Hans_CN")
        XCTAssertEqual(Formatting.cost(24_128.26, locale: zh), "$2.4万")
        XCTAssertEqual(Formatting.cost(1_200_000, locale: zh), "$120万")
    }

    func testCompactCustomLabelIsLocaleAwareAndCollapsesSingleDate() throws {
        let timezone = try XCTUnwrap(TimeZone(identifier: "America/Los_Angeles"))
        let locale = Locale(identifier: "en_US")

        XCTAssertEqual(
            Formatting.compactDateRange(
                DateSelectionRange(startDate: "2026-08-04", endDate: "2026-08-04"),
                timeZone: timezone,
                locale: locale
            ),
            "8/4/26"
        )
        XCTAssertEqual(
            Formatting.compactDateRange(
                DateSelectionRange(startDate: "2026-08-03", endDate: "2026-08-04"),
                timeZone: timezone,
                locale: locale
            ),
            "8/3/26–8/4/26"
        )
    }

    func testRepeatedFallBackHourLabelsIncludeOffsetsOnlyWhenNeeded() throws {
        let timezone = try XCTUnwrap(TimeZone(identifier: "America/New_York"))
        let buckets = [
            bucket(
                id: "first",
                start: "2026-11-01T01:00:00-04:00",
                end: "2026-11-01T01:00:00-05:00"
            ),
            bucket(
                id: "second",
                start: "2026-11-01T01:00:00-05:00",
                end: "2026-11-01T02:00:00-05:00"
            ),
            bucket(
                id: "third",
                start: "2026-11-01T02:00:00-05:00",
                end: "2026-11-01T03:00:00-05:00"
            ),
        ]

        let labels = Formatting.chartBucketLabels(
            buckets: buckets,
            granularity: .hour,
            timeZone: timezone,
            locale: Locale(identifier: "en_US")
        )

        XCTAssertTrue(labels[0].contains("−04:00"))
        XCTAssertTrue(labels[1].contains("−05:00"))
        XCTAssertFalse(labels[2].contains("−05:00"))
    }

    func testGranularityTitlesAndLabels() throws {
        let timezone = try XCTUnwrap(TimeZone(identifier: "UTC"))
        let locale = Locale(identifier: "en_US")
        let sample = bucket(
            id: "sample",
            start: "2026-07-06T00:00:00Z",
            end: "2026-07-13T00:00:00Z"
        )

        XCTAssertEqual(UsageTimeGranularity.hour.title, "Hourly")
        XCTAssertEqual(UsageTimeGranularity.day.title, "Daily")
        XCTAssertEqual(UsageTimeGranularity.naturalWeek.title, "Weekly")
        XCTAssertEqual(UsageTimeGranularity.naturalMonth.title, "Monthly")
        XCTAssertEqual(
            Formatting.chartBucketLabels(
                buckets: [sample],
                granularity: .naturalWeek,
                timeZone: timezone,
                locale: locale
            ),
            ["Jul 6"]
        )
    }

    func testInputCacheRate() {
        XCTAssertEqual(Formatting.inputCacheRate(input: 0, cacheRead: 0), 0, accuracy: 0.0001)
        XCTAssertEqual(Formatting.inputCacheRate(input: 100, cacheRead: 100), 0.5, accuracy: 0.0001)
        XCTAssertEqual(Formatting.inputCacheRate(input: 0, cacheRead: 200), 1.0, accuracy: 0.0001)
        XCTAssertEqual(Formatting.inputCacheRate(input: 200, cacheRead: 0), 0, accuracy: 0.0001)
    }

    func testMenuBarTitleModes() {
        let report = UsageReport.testFixture(totalTokens: 1_200_000, totalCost: 4.2)
        XCTAssertEqual(Formatting.menuBarTitle(report: report, mode: .tokens, missingBinary: false, hasError: false, locale: Locale(identifier: "en_US")), "1.2m")
        XCTAssertEqual(Formatting.menuBarTitle(report: report, mode: .cost, missingBinary: false, hasError: false, locale: Locale(identifier: "en_US")), "$4.20")
        XCTAssertEqual(Formatting.menuBarTitle(report: report, mode: .both, missingBinary: false, hasError: false, locale: Locale(identifier: "en_US")), "1.2m · $4.20")
        XCTAssertEqual(Formatting.menuBarTitle(report: nil, mode: .tokens, missingBinary: true, hasError: false), "tokens?")
    }

    func testTooltipRangeWithinSingleDayKeepsHours() throws {
        let timezone = try XCTUnwrap(TimeZone(identifier: "UTC"))
        let locale = Locale(identifier: "en_US")

        XCTAssertEqual(
            Formatting.chartBucketTooltipRange(
                bucket(
                    id: "hour",
                    start: "2026-08-05T09:00:00Z",
                    end: "2026-08-05T10:00:00Z"
                ),
                timeZone: timezone,
                locale: locale
            ),
            "Aug 5, 09:00 – 10:00"
        )
        XCTAssertEqual(
            Formatting.chartBucketTooltipRange(
                bucket(
                    id: "partial-day",
                    start: "2026-08-05T00:00:00Z",
                    end: "2026-08-05T14:30:00Z"
                ),
                timeZone: timezone,
                locale: locale
            ),
            "Aug 5, 00:00 – 14:30"
        )
    }

    func testTooltipRangeFullDayCollapsesToDate() throws {
        let timezone = try XCTUnwrap(TimeZone(identifier: "UTC"))
        let locale = Locale(identifier: "en_US")

        XCTAssertEqual(
            Formatting.chartBucketTooltipRange(
                bucket(
                    id: "full-day",
                    start: "2026-08-05T00:00:00Z",
                    end: "2026-08-06T00:00:00Z"
                ),
                timeZone: timezone,
                locale: locale
            ),
            "Aug 5"
        )
    }

    func testTooltipRangeMultipleDaysDropsHours() throws {
        let timezone = try XCTUnwrap(TimeZone(identifier: "UTC"))
        let locale = Locale(identifier: "en_US")

        // Full calendar days: the exclusive end lands on the next midnight.
        XCTAssertEqual(
            Formatting.chartBucketTooltipRange(
                bucket(
                    id: "same-month",
                    start: "2026-08-05T00:00:00Z",
                    end: "2026-08-08T00:00:00Z"
                ),
                timeZone: timezone,
                locale: locale
            ),
            "Aug 5 – 7"
        )
        XCTAssertEqual(
            Formatting.chartBucketTooltipRange(
                bucket(
                    id: "partial-days",
                    start: "2026-08-05T09:00:00Z",
                    end: "2026-08-07T14:30:00Z"
                ),
                timeZone: timezone,
                locale: locale
            ),
            "Aug 5 – 7"
        )
        XCTAssertEqual(
            Formatting.chartBucketTooltipRange(
                bucket(
                    id: "cross-month",
                    start: "2026-08-30T00:00:00Z",
                    end: "2026-09-02T00:00:00Z"
                ),
                timeZone: timezone,
                locale: locale
            ),
            "Aug 30 – Sep 1"
        )
        XCTAssertEqual(
            Formatting.chartBucketTooltipRange(
                bucket(
                    id: "cross-year",
                    start: "2025-12-30T00:00:00Z",
                    end: "2026-01-02T00:00:00Z"
                ),
                timeZone: timezone,
                locale: locale
            ),
            "Dec 30 – Jan 1"
        )
    }

    func testTooltipRangeMidnightExclusiveHourKeepsLastCoveredDay() throws {
        let timezone = try XCTUnwrap(TimeZone(identifier: "UTC"))
        let locale = Locale(identifier: "en_US")

        // Exclusive end is the next day's midnight; last covered moment is still
        // Aug 5, so the same-day branch formats end as 00:00 (not a reversed range).
        XCTAssertEqual(
            Formatting.chartBucketTooltipRange(
                bucket(
                    id: "midnight-exclusive",
                    start: "2026-08-05T23:00:00Z",
                    end: "2026-08-06T00:00:00Z"
                ),
                timeZone: timezone,
                locale: locale
            ),
            "Aug 5, 23:00 – 00:00"
        )
    }

    func testTooltipRangeNonUTCExclusiveEndBoundaries() throws {
        let timezone = try XCTUnwrap(TimeZone(identifier: "America/Los_Angeles"))
        let locale = Locale(identifier: "en_US")

        // DST spring-forward day (23 real hours) still collapses to the date alone.
        XCTAssertEqual(
            Formatting.chartBucketTooltipRange(
                bucket(
                    id: "la-full-day-dst",
                    start: "2026-03-08T00:00:00-08:00",
                    end: "2026-03-09T00:00:00-07:00"
                ),
                timeZone: timezone,
                locale: locale
            ),
            "Mar 8"
        )

        // Local midnight-crossing hour bucket: exclusive end is next local midnight.
        XCTAssertEqual(
            Formatting.chartBucketTooltipRange(
                bucket(
                    id: "la-midnight-hour",
                    start: "2026-08-05T23:00:00-07:00",
                    end: "2026-08-06T00:00:00-07:00"
                ),
                timeZone: timezone,
                locale: locale
            ),
            "Aug 5, 23:00 – 00:00"
        )
    }

    func testTooltipRangeDateComponentsFollowLocale() throws {
        let timezone = try XCTUnwrap(TimeZone(identifier: "UTC"))
        let locale = Locale(identifier: "fr_FR")

        let label = Formatting.chartBucketTooltipRange(
            bucket(
                id: "fr-full-day",
                start: "2026-08-05T00:00:00Z",
                end: "2026-08-06T00:00:00Z"
            ),
            timeZone: timezone,
            locale: locale
        )

        // Localized "MMM d" for fr_FR renders day-first month names (e.g. "5 août").
        XCTAssertTrue(
            label.localizedCaseInsensitiveContains("août"),
            "expected French month name in \(label)"
        )
        XCTAssertTrue(label.contains("5"), "expected day 5 in \(label)")
    }

    private func bucket(id: String, start: String, end: String) -> UsageTimeBucket {
        UsageTimeBucket(
            id: id,
            nominalStart: start,
            nominalEndExclusive: end,
            coveredStart: start,
            coveredEndExclusive: end,
            totals: UsageTotals(tokens: 0, cost: 0, messages: 0),
            contextOnly: false,
            incompleteEdge: false,
            active: false
        )
    }
}

private extension UsageReport {
    static func testFixture(totalTokens: Int64, totalCost: Double) -> UsageReport {
        UsageReport(
            schemaVersion: 3,
            generatedAt: "2026-08-05T00:00:00Z",
            selection: .preset(.today),
            dateRange: UsageDateRange(startDate: "2026-08-05", endDate: "2026-08-05", timezone: "UTC"),
            scan: ScanInfo(
                mode: "snapshot",
                forceRescan: false,
                durationMs: 0,
                cache: ScanCacheInfo(sourceHits: 0, sourceMisses: 0, snapshotRebuilt: false, snapshotSchemaVersion: 3)
            ),
            summary: UsageSummary(totalTokens: totalTokens, totalCost: totalCost, messages: 3, activeDays: 1, clients: [], models: []),
            tokenBreakdown: TokenBreakdown(input: totalTokens, output: 0, cacheRead: 0, cacheWrite: 0, reasoning: 0),
            byClient: [],
            byProject: [],
            byModel: [],
            timeSeries: UsageTimeSeries(
                granularity: .hour,
                selectionStart: "2026-08-05T00:00:00Z",
                buckets: [],
                unplaced: UsageTotals(tokens: 0, cost: 0, messages: 0)
            ),
            meta: UsageMeta(cliVersion: "test", timezone: "UTC", reportContract: "v3")
        )
    }
}
