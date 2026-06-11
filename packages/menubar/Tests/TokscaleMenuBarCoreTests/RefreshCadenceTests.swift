import XCTest

@testable import TokscaleMenuBarCore

final class RefreshCadenceTests: XCTestCase {
    func testMinimumIntervalForEachCadence() {
        XCTAssertNil(RefreshCadence.off.minimumInterval)
        XCTAssertEqual(RefreshCadence.everyMinute.minimumInterval, 60)
        XCTAssertEqual(RefreshCadence.everyFiveMinutes.minimumInterval, 300)
    }

    func testDefaultsToEveryMinuteForMissingOrUnknownStoredValue() {
        XCTAssertEqual(RefreshCadence(storedValue: nil), .everyMinute)
        XCTAssertEqual(RefreshCadence(storedValue: "bogus"), .everyMinute)
    }

    func testRoundTripsThroughStoredValue() {
        for cadence in RefreshCadence.allCases {
            XCTAssertEqual(RefreshCadence(storedValue: cadence.rawValue), cadence)
        }
    }

    func testOpenRefreshScanPolicyNeverStartsHistoryScanAutomatically() throws {
        let policy = OpenRefreshScanPolicy()
        let now = try Self.isoDate("2026-06-04T02:30:00Z")

        XCTAssertEqual(
            policy.scan(
                summaryIsMissing: true,
                needsUsageScan: true,
                isBackgroundScanning: false,
                lastTodayScan: .distantPast,
                now: now
            ),
            .none
        )

        XCTAssertEqual(
            policy.scan(
                summaryIsMissing: false,
                needsUsageScan: true,
                isBackgroundScanning: false,
                lastTodayScan: .distantPast,
                now: now
            ),
            .today
        )

        XCTAssertEqual(
            policy.scan(
                summaryIsMissing: false,
                needsUsageScan: true,
                isBackgroundScanning: false,
                lastTodayScan: try Self.isoDate("2026-06-04T02:25:30Z"),
                now: now
            ),
            .none
        )
    }

    private static func isoDate(_ value: String) throws -> Date {
        let formatter = ISO8601DateFormatter()
        return try XCTUnwrap(formatter.date(from: value))
    }
}
