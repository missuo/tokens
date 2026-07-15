import XCTest

@testable import TokscaleMenuBarCore

final class BackgroundScanPolicyTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_800_000_000)

    func testRunsFullScanWhenHistoryIsDueAndRetryWindowElapsed() {
        XCTAssertEqual(
            BackgroundScanPolicy.nextAction(
                now: now,
                lastHistorySuccess: now.addingTimeInterval(-86_400),
                lastHistoryAttempt: now.addingTimeInterval(-21_600),
                lastTodayScan: now
            ),
            .full
        )
    }

    func testRunsTodayScanWhileFailedFullScanIsBackingOff() {
        XCTAssertEqual(
            BackgroundScanPolicy.nextAction(
                now: now,
                lastHistorySuccess: now.addingTimeInterval(-86_400),
                lastHistoryAttempt: now.addingTimeInterval(-60),
                lastTodayScan: now.addingTimeInterval(-600)
            ),
            .today
        )
    }

    func testFailedHistoryRefreshBacksOffForSixHours() {
        XCTAssertEqual(
            BackgroundScanPolicy.nextAction(
                now: now,
                lastHistorySuccess: now.addingTimeInterval(-86_400),
                lastHistoryAttempt: now.addingTimeInterval(-21_599),
                lastTodayScan: now
            ),
            .none
        )
    }

    func testDoesNothingWhenNeitherScanIsDue() {
        XCTAssertEqual(
            BackgroundScanPolicy.nextAction(
                now: now,
                lastHistorySuccess: now,
                lastHistoryAttempt: now,
                lastTodayScan: now
            ),
            .none
        )
    }

    func testRestoresTodayAndHistoryDatesFromSummary() throws {
        let restored = try XCTUnwrap(
            BackgroundScanPolicy.restoredScanDates(
                generatedAt: "2026-07-13T03:01:02.123Z",
                historyGeneratedAt: "2026-07-13T02:00:00Z"
            )
        )

        XCTAssertEqual(restored.today.timeIntervalSince1970, 1_783_911_662.123, accuracy: 0.001)
        XCTAssertEqual(restored.history.timeIntervalSince1970, 1_783_908_000, accuracy: 0.001)
    }

    func testRestoredHistoryFallsBackToGeneratedAtForLegacySummary() throws {
        let restored = try XCTUnwrap(
            BackgroundScanPolicy.restoredScanDates(
                generatedAt: "2026-07-13T03:01:02Z",
                historyGeneratedAt: nil
            )
        )

        XCTAssertEqual(restored.history, restored.today)
    }

    func testQuotaOpenRefreshCoalescesRapidOpenEvents() {
        XCTAssertTrue(
            BackgroundScanPolicy.shouldRefreshQuotaOnOpen(
                needsRefresh: true,
                isRefreshing: false,
                now: now,
                lastAttempt: .distantPast
            )
        )
        for seconds in 1...10 {
            XCTAssertFalse(
                BackgroundScanPolicy.shouldRefreshQuotaOnOpen(
                    needsRefresh: true,
                    isRefreshing: false,
                    now: now.addingTimeInterval(TimeInterval(seconds)),
                    lastAttempt: now
                )
            )
        }
        XCTAssertTrue(
            BackgroundScanPolicy.shouldRefreshQuotaOnOpen(
                needsRefresh: true,
                isRefreshing: false,
                now: now.addingTimeInterval(120),
                lastAttempt: now
            )
        )
    }

    func testRemoteHistoryFailureFallsBackToFullLocalScan() {
        XCTAssertEqual(
            BackgroundScanPolicy.actionAfterRemoteHistorySync(succeeded: false),
            .full
        )
    }

    func testRemoteHistorySuccessOnlyNeedsTodayScan() {
        XCTAssertEqual(
            BackgroundScanPolicy.actionAfterRemoteHistorySync(succeeded: true),
            .today
        )
    }
}
