import Foundation
import XCTest
@testable import TokensMenuBarCore

@MainActor
final class UsageStoreTests: XCTestCase {
    func testLaunchAlwaysDefaultsToTodayEvenWithLegacyStoredPeriod() throws {
        let suiteName = "UsageStoreTests.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        defaults.set("all", forKey: "lastPeriod")

        let store = UsageStore(
            settings: AppSettings(defaults: defaults),
            binaryResolver: { _ in "/tmp/tokens" },
            reportFetcher: { _, _, _ in throw TestError.unused }
        )

        XCTAssertEqual(store.selection, .preset(.today))
    }

    func testStoreRejectsFetcherReportForDifferentSelection() async throws {
        let wrongReport = makeReport(selection: .preset(.days7), totalTokens: 7)
        let store = UsageStore(
            binaryResolver: { _ in "/tmp/tokens" },
            reportFetcher: { _, _, _ in wrongReport }
        )

        store.bootstrap()
        try await Task.sleep(nanoseconds: 40_000_000)

        XCTAssertNil(store.report)
        XCTAssertNotNil(store.lastError)
        XCTAssertFalse(store.isLoading)
    }

    func testFailedRangeSwitchExplicitlyReportsStalePreviousData() async throws {
        let todayReport = makeReport(selection: .preset(.today), totalTokens: 1)
        let store = UsageStore(
            binaryResolver: { _ in "/tmp/tokens" },
            reportFetcher: { selection, _, _ in
                if selection == .preset(.today) { return todayReport }
                throw TestError.failedRange
            }
        )

        store.bootstrap()
        try await Task.sleep(nanoseconds: 40_000_000)
        XCTAssertFalse(store.isShowingStaleReport)

        store.setPeriod(.days7)
        try await Task.sleep(nanoseconds: 40_000_000)

        XCTAssertEqual(store.selection, .preset(.days7))
        XCTAssertEqual(store.report?.selection, .preset(.today))
        XCTAssertTrue(store.isShowingStaleReport)
        XCTAssertNotNil(store.lastError)
    }

    func testLateStaleResponseCannotOverwriteNewerSelection() async throws {
        let todayReport = makeReport(selection: .preset(.today), totalTokens: 1)
        let sevenDayReport = makeReport(selection: .preset(.days7), totalTokens: 7)
        let store = UsageStore(
            binaryResolver: { _ in "/tmp/tokens" },
            reportFetcher: { selection, _, _ in
                switch selection {
                case .preset(.today):
                    Thread.sleep(forTimeInterval: 0.16)
                    return todayReport
                case .preset(.days7):
                    Thread.sleep(forTimeInterval: 0.02)
                    return sevenDayReport
                default:
                    throw TestError.unused
                }
            }
        )

        store.bootstrap()
        try await Task.sleep(nanoseconds: 15_000_000)
        store.setPeriod(.days7)
        try await Task.sleep(nanoseconds: 260_000_000)

        XCTAssertEqual(store.selection, .preset(.days7))
        XCTAssertEqual(store.report?.selection, .preset(.days7))
        XCTAssertEqual(store.report?.summary.totalTokens, 7)
        XCTAssertNil(store.lastError)
        XCTAssertFalse(store.isLoading)
    }

    func testLoadAdvancedReportAlwaysFetchesFixedThirtyDayWindow() async throws {
        let thirtyDayReport = makeReport(selection: .preset(.days30), totalTokens: 30)
        let store = UsageStore(
            binaryResolver: { _ in "/tmp/tokens" },
            reportFetcher: { selection, _, _ in
                switch selection {
                case .preset(.days30):
                    return thirtyDayReport
                default:
                    throw TestError.unused
                }
            }
        )

        store.loadAdvancedReport()
        try await Task.sleep(nanoseconds: 60_000_000)

        XCTAssertEqual(store.advancedReport?.selection, .preset(.days30))
        XCTAssertEqual(store.advancedReport?.summary.totalTokens, 30)
        XCTAssertNil(store.report)
    }

    func testLoadAdvancedReportRejectsMismatchedSelection() async throws {
        let wrongReport = makeReport(selection: .preset(.today), totalTokens: 1)
        let store = UsageStore(
            binaryResolver: { _ in "/tmp/tokens" },
            reportFetcher: { _, _, _ in wrongReport }
        )

        store.loadAdvancedReport()
        try await Task.sleep(nanoseconds: 60_000_000)

        XCTAssertNil(store.advancedReport)
    }

    private func makeReport(selection: UsageSelection, totalTokens: Int64) -> UsageReport {
        UsageReport(
            schemaVersion: 3,
            generatedAt: "2026-08-05T00:00:00Z",
            selection: selection,
            dateRange: UsageDateRange(
                startDate: "2026-08-04",
                endDate: "2026-08-04",
                timezone: "UTC"
            ),
            scan: ScanInfo(
                mode: "snapshot",
                forceRescan: false,
                durationMs: 0,
                cache: ScanCacheInfo(
                    sourceHits: 0,
                    sourceMisses: 0,
                    snapshotRebuilt: false,
                    snapshotSchemaVersion: 3
                )
            ),
            summary: UsageSummary(
                totalTokens: totalTokens,
                totalCost: Double(totalTokens),
                messages: Int32(totalTokens),
                activeDays: 1,
                clients: [],
                models: []
            ),
            tokenBreakdown: TokenBreakdown(
                input: totalTokens,
                output: 0,
                cacheRead: 0,
                cacheWrite: 0,
                reasoning: 0
            ),
            byClient: [],
            byProject: [],
            byModel: [],
            timeSeries: UsageTimeSeries(
                granularity: .hour,
                selectionStart: "2026-08-04T00:00:00Z",
                buckets: [],
                unplaced: UsageTotals(tokens: totalTokens, cost: Double(totalTokens), messages: Int32(totalTokens))
            ),
            weekdayHour: nil,
            meta: UsageMeta(cliVersion: "test", timezone: "UTC", reportContract: "v3")
        )
    }

    private enum TestError: Error {
        case unused
        case failedRange
    }
}
