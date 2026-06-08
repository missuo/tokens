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
}
