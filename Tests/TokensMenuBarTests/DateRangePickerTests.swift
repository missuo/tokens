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

    // MARK: - Two-click selection cycle

    private typealias CyclePhase = DateRangeSelectionCycle.Phase

    func testCycleStartsAwaitingEndForSingleDayAndCompleteForRange() {
        XCTAssertEqual(
            DateRangeSelectionCycle.initialPhase(
                for: DateSelectionRange(startDate: "2026-08-05", endDate: "2026-08-05")
            ),
            .awaitingEnd
        )
        XCTAssertEqual(
            DateRangeSelectionCycle.initialPhase(
                for: DateSelectionRange(startDate: "2026-08-01", endDate: "2026-08-05")
            ),
            .complete
        )
    }

    func testFirstClickPlacesStartPoint() {
        let previous = DateSelectionRange(startDate: "2026-08-01", endDate: "2026-08-01")
        let reported = DateSelectionRange(startDate: "2026-08-03", endDate: "2026-08-03")

        let result = DateRangeSelectionCycle.reduce(
            reported: reported,
            previous: previous,
            phase: .awaitingEnd
        )

        XCTAssertEqual(result.selection, reported)
        XCTAssertEqual(result.phase, .awaitingEnd)
        XCTAssertFalse(result.reanchor)
    }

    func testSecondClickCompletesRange() {
        let start = DateSelectionRange(startDate: "2026-08-03", endDate: "2026-08-03")
        let reported = DateSelectionRange(startDate: "2026-08-03", endDate: "2026-08-07")

        let result = DateRangeSelectionCycle.reduce(
            reported: reported,
            previous: start,
            phase: .awaitingEnd
        )

        XCTAssertEqual(result.selection, reported)
        XCTAssertEqual(result.phase, .complete)
        XCTAssertFalse(result.reanchor)
    }

    func testThirdClickRestartsFromClickedDayWhenNativeControlExtends() {
        let completed = DateSelectionRange(startDate: "2026-08-03", endDate: "2026-08-07")
        // Native control kept the old anchor and extended the range instead of
        // restarting: the cycle must restart at the moved endpoint.
        let reported = DateSelectionRange(startDate: "2026-08-03", endDate: "2026-08-10")

        let result = DateRangeSelectionCycle.reduce(
            reported: reported,
            previous: completed,
            phase: .complete
        )

        XCTAssertEqual(
            result.selection,
            DateSelectionRange(startDate: "2026-08-10", endDate: "2026-08-10")
        )
        XCTAssertEqual(result.phase, .awaitingEnd)
        XCTAssertTrue(result.reanchor)
    }

    func testThirdClickBeforeOldAnchorRestartsAtEarlierDay() {
        let completed = DateSelectionRange(startDate: "2026-08-03", endDate: "2026-08-07")
        let reported = DateSelectionRange(startDate: "2026-08-01", endDate: "2026-08-07")

        let result = DateRangeSelectionCycle.reduce(
            reported: reported,
            previous: completed,
            phase: .complete
        )

        XCTAssertEqual(
            result.selection,
            DateSelectionRange(startDate: "2026-08-01", endDate: "2026-08-01")
        )
        XCTAssertEqual(result.phase, .awaitingEnd)
        XCTAssertTrue(result.reanchor)
    }

    func testThirdClickPassesThroughWhenNativeControlAlreadyReanchors() {
        let completed = DateSelectionRange(startDate: "2026-08-03", endDate: "2026-08-07")
        let reported = DateSelectionRange(startDate: "2026-08-10", endDate: "2026-08-10")

        let result = DateRangeSelectionCycle.reduce(
            reported: reported,
            previous: completed,
            phase: .complete
        )

        XCTAssertEqual(result.selection, reported)
        XCTAssertEqual(result.phase, .awaitingEnd)
        XCTAssertFalse(result.reanchor)
    }

    func testClickingSameDayTwiceStaysSingleDay() {
        let start = DateSelectionRange(startDate: "2026-08-03", endDate: "2026-08-03")

        let result = DateRangeSelectionCycle.reduce(
            reported: start,
            previous: start,
            phase: .awaitingEnd
        )

        XCTAssertEqual(result.selection, start)
        XCTAssertEqual(result.phase, .awaitingEnd)
        XCTAssertFalse(result.reanchor)
    }

    // MARK: - Committed draft selection

    func testCommittedSelectionNilForUnorderedDraft() {
        let draft = DateSelectionRange(startDate: "2026-08-10", endDate: "2026-08-01")
        XCTAssertNil(DateRangePickerConversion.committedSelection(for: draft, today: draft))
    }

    func testCommittedSelectionMapsExactlyTodayToTodayPreset() {
        let today = DateRangePickerConversion.today(
            now: Date(timeIntervalSince1970: 1_700_000_000),
            timeZone: TimeZone(identifier: "Asia/Shanghai")!
        )
        let draft = today
        XCTAssertEqual(
            DateRangePickerConversion.committedSelection(for: draft, today: today),
            .preset(.today)
        )
    }

    func testCommittedSelectionKeepsSingleNonTodayDayAsCustomRange() {
        let draft = DateSelectionRange(startDate: "2026-08-05", endDate: "2026-08-05")
        let today = DateSelectionRange(startDate: "2026-08-06", endDate: "2026-08-06")
        XCTAssertEqual(
            DateRangePickerConversion.committedSelection(for: draft, today: today),
            .custom(draft)
        )
    }

    func testCommittedSelectionKeepsMultiDayRangeAsCustom() {
        let draft = DateSelectionRange(startDate: "2026-08-01", endDate: "2026-08-05")
        let today = DateSelectionRange(startDate: "2026-08-06", endDate: "2026-08-06")
        XCTAssertEqual(
            DateRangePickerConversion.committedSelection(for: draft, today: today),
            .custom(draft)
        )
    }
}
