import XCTest
@testable import TokensMenuBarCore

final class DateRangePickerTests: XCTestCase {
    func testSingleDateRoundTripsAsZeroDuration() throws {
        let timezone = try XCTUnwrap(TimeZone(identifier: "America/Los_Angeles"))
        let range = DateSelectionRange(startDate: "2026-08-04", endDate: "2026-08-04")

        let picker = try DateRangePickerConversion.pickerValues(for: range, timeZone: timezone)
        XCTAssertEqual(picker.timeInterval, 0, accuracy: 0.001)
        XCTAssertEqual(
            DateRangePickerConversion.selection(
                dateValue: picker.dateValue,
                timeInterval: picker.timeInterval,
                timeZone: timezone
            ),
            range
        )
    }

    func testSpringForwardRangeUsesCalendarDaysNot86400SecondAssumption() throws {
        let timezone = try XCTUnwrap(TimeZone(identifier: "America/Los_Angeles"))
        let range = DateSelectionRange(startDate: "2026-03-07", endDate: "2026-03-09")

        let picker = try DateRangePickerConversion.pickerValues(for: range, timeZone: timezone)
        XCTAssertEqual(picker.timeInterval, 47 * 60 * 60, accuracy: 0.001)
        XCTAssertEqual(
            DateRangePickerConversion.selection(
                dateValue: picker.dateValue,
                timeInterval: picker.timeInterval,
                timeZone: timezone
            ),
            range
        )
    }

    func testFallBackRangePreservesRepeatedHourDay() throws {
        let timezone = try XCTUnwrap(TimeZone(identifier: "America/Los_Angeles"))
        let range = DateSelectionRange(startDate: "2026-10-31", endDate: "2026-11-02")

        let picker = try DateRangePickerConversion.pickerValues(for: range, timeZone: timezone)
        XCTAssertEqual(picker.timeInterval, 49 * 60 * 60, accuracy: 0.001)
        XCTAssertEqual(
            DateRangePickerConversion.selection(
                dateValue: picker.dateValue,
                timeInterval: picker.timeInterval,
                timeZone: timezone
            ),
            range
        )
    }

    func testPresetDraftRangesUseInclusiveReportingDates() throws {
        let timezone = try XCTUnwrap(TimeZone(identifier: "America/Los_Angeles"))
        let now = ISO8601DateFormatter().date(from: "2026-08-05T19:00:00Z")!

        XCTAssertEqual(
            DateRangePickerConversion.range(for: .today, now: now, timeZone: timezone),
            DateSelectionRange(startDate: "2026-08-05", endDate: "2026-08-05")
        )
        XCTAssertEqual(
            DateRangePickerConversion.range(for: .days7, now: now, timeZone: timezone),
            DateSelectionRange(startDate: "2026-07-30", endDate: "2026-08-05")
        )
        XCTAssertEqual(
            DateRangePickerConversion.range(for: .days30, now: now, timeZone: timezone),
            DateSelectionRange(startDate: "2026-07-07", endDate: "2026-08-05")
        )
        XCTAssertNil(
            DateRangePickerConversion.range(for: .all, now: now, timeZone: timezone)
        )
    }

    func testTodayMaximumIsEndOfReportingDay() throws {
        let timezone = try XCTUnwrap(TimeZone(identifier: "Asia/Tokyo"))
        let now = ISO8601DateFormatter().date(from: "2026-08-04T16:30:00Z")!
        let maxDate = try XCTUnwrap(DateRangePickerConversion.maximumDate(now: now, timeZone: timezone))
        let calendar = DateRangePickerConversion.calendar(timeZone: timezone)

        XCTAssertEqual(calendar.component(.day, from: maxDate), 5)
        XCTAssertEqual(calendar.component(.hour, from: maxDate), 23)
    }
}
