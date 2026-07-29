import XCTest
@testable import TokensMenuBarCore

final class ScanIntervalTests: XCTestCase {
    func testPresetStorageRoundTrip() {
        let cases: [ScanIntervalOption] = [
            .fifteenMinutes, .oneHour, .sixHours, .twelveHours, .manual,
        ]
        for option in cases {
            XCTAssertEqual(ScanIntervalOption.fromStorage(option.storageKey), option)
        }
    }

    func testCustomStorageRoundTrip() {
        let option = ScanIntervalOption.custom(minutes: 30)
        XCTAssertEqual(option.storageKey, "custom:30")
        XCTAssertEqual(ScanIntervalOption.fromStorage("custom:30"), .custom(minutes: 30))
    }

    func testLegacyTwentyFourHoursMapsToCustom() {
        XCTAssertEqual(ScanIntervalOption.fromStorage("24h"), .custom(minutes: 1440))
    }

    func testUnknownStorageFallsBackToTwelveHours() {
        XCTAssertEqual(ScanIntervalOption.fromStorage(nil), .twelveHours)
        XCTAssertEqual(ScanIntervalOption.fromStorage(""), .twelveHours)
        XCTAssertEqual(ScanIntervalOption.fromStorage("nope"), .twelveHours)
    }

    func testTimeIntervals() {
        XCTAssertEqual(ScanIntervalOption.fifteenMinutes.timeInterval, 15 * 60)
        XCTAssertEqual(ScanIntervalOption.oneHour.timeInterval, 3600)
        XCTAssertEqual(ScanIntervalOption.sixHours.timeInterval, 6 * 3600)
        XCTAssertEqual(ScanIntervalOption.twelveHours.timeInterval, 12 * 3600)
        XCTAssertNil(ScanIntervalOption.manual.timeInterval)
        XCTAssertEqual(ScanIntervalOption.custom(minutes: 30).timeInterval, 30 * 60)
    }

    func testClampMinutes() {
        XCTAssertEqual(ScanIntervalOption.clampMinutes(1), 5)
        XCTAssertEqual(ScanIntervalOption.clampMinutes(30), 30)
        XCTAssertEqual(ScanIntervalOption.clampMinutes(10_000), 24 * 60)
    }

    func testStepLadderMinutes() {
        XCTAssertEqual(
            ScanIntervalOption.steppedCustomMinutes(from: 15, unit: .minutes, direction: 1),
            30
        )
        XCTAssertEqual(
            ScanIntervalOption.steppedCustomMinutes(from: 15, unit: .minutes, direction: -1),
            5
        )
        // At top of minute ladder
        XCTAssertEqual(
            ScanIntervalOption.steppedCustomMinutes(from: 60, unit: .minutes, direction: 1),
            60
        )
    }

    func testStepLadderHours() {
        XCTAssertEqual(
            ScanIntervalOption.steppedCustomMinutes(from: 60, unit: .hours, direction: 1),
            120
        )
        XCTAssertEqual(
            ScanIntervalOption.steppedCustomMinutes(from: 1440, unit: .hours, direction: 1),
            1440
        )
    }

    func testChipMatches() {
        XCTAssertTrue(ScanIntervalChip.fifteenMinutes.matches(.fifteenMinutes))
        XCTAssertTrue(ScanIntervalChip.off.matches(.manual))
        XCTAssertTrue(ScanIntervalChip.custom.matches(.custom(minutes: 45)))
        XCTAssertFalse(ScanIntervalChip.oneHour.matches(.sixHours))
    }

    func testPreferredUnit() {
        XCTAssertEqual(ScanIntervalCustomUnit.preferred(forMinutes: 30), .minutes)
        XCTAssertEqual(ScanIntervalCustomUnit.preferred(forMinutes: 120), .hours)
        XCTAssertEqual(ScanIntervalCustomUnit.preferred(forMinutes: 90), .minutes)
    }
}
