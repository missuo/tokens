import XCTest
@testable import TokensMenuBarCore

final class DateRangePickerTests: XCTestCase {
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

    func testFirstClickAfterCompleteRangeRestartsAtClickedDay() {
        let completed = DateSelectionRange(startDate: "2026-08-03", endDate: "2026-08-07")

        let result = DateRangeSelectionCycle.reduce(
            clicked: "2026-08-10",
            previous: completed,
            phase: .complete
        )

        XCTAssertEqual(
            result.selection,
            DateSelectionRange(startDate: "2026-08-10", endDate: "2026-08-10")
        )
        XCTAssertEqual(result.phase, .awaitingEnd)
    }

    func testSecondClickCompletesRangeForward() {
        let start = DateSelectionRange(startDate: "2026-08-03", endDate: "2026-08-03")

        let result = DateRangeSelectionCycle.reduce(
            clicked: "2026-08-07",
            previous: start,
            phase: .awaitingEnd
        )

        XCTAssertEqual(
            result.selection,
            DateSelectionRange(startDate: "2026-08-03", endDate: "2026-08-07")
        )
        XCTAssertEqual(result.phase, .complete)
    }

    func testSecondClickBeforeAnchorSwapsEndpoints() {
        let start = DateSelectionRange(startDate: "2026-08-07", endDate: "2026-08-07")

        let result = DateRangeSelectionCycle.reduce(
            clicked: "2026-08-03",
            previous: start,
            phase: .awaitingEnd
        )

        XCTAssertEqual(
            result.selection,
            DateSelectionRange(startDate: "2026-08-03", endDate: "2026-08-07")
        )
        XCTAssertEqual(result.phase, .complete)
        XCTAssertTrue(result.selection.isOrdered)
    }

    func testSecondClickOnSameDayCompletesSingleDayRange() {
        let start = DateSelectionRange(startDate: "2026-08-03", endDate: "2026-08-03")

        let result = DateRangeSelectionCycle.reduce(
            clicked: "2026-08-03",
            previous: start,
            phase: .awaitingEnd
        )

        XCTAssertEqual(result.selection, start)
        XCTAssertEqual(result.phase, .complete)
    }

    func testThirdClickStartsOverFromClickedDay() {
        // Click 1 restarts at the clicked day, click 2 completes, click 3 restarts.
        var selection = DateSelectionRange(startDate: "2026-08-03", endDate: "2026-08-07")
        var phase: CyclePhase = .complete

        var result = DateRangeSelectionCycle.reduce(
            clicked: "2026-08-20",
            previous: selection,
            phase: phase
        )
        selection = result.selection
        phase = result.phase
        XCTAssertEqual(selection, DateSelectionRange(startDate: "2026-08-20", endDate: "2026-08-20"))
        XCTAssertEqual(phase, .awaitingEnd)

        result = DateRangeSelectionCycle.reduce(
            clicked: "2026-08-15",
            previous: selection,
            phase: phase
        )
        selection = result.selection
        phase = result.phase
        XCTAssertEqual(selection, DateSelectionRange(startDate: "2026-08-15", endDate: "2026-08-20"))
        XCTAssertEqual(phase, .complete)

        result = DateRangeSelectionCycle.reduce(
            clicked: "2026-08-01",
            previous: selection,
            phase: phase
        )
        XCTAssertEqual(result.selection, DateSelectionRange(startDate: "2026-08-01", endDate: "2026-08-01"))
        XCTAssertEqual(result.phase, .awaitingEnd)
    }

    // MARK: - Month grid

    func testMonthGridAlignsDaysToWeekStart() throws {
        let timezone = try XCTUnwrap(TimeZone(identifier: "America/Los_Angeles"))
        let august = try DateRangePickerConversion.date(from: "2026-08-15", timeZone: timezone)

        let grid = DateRangePickerConversion.monthGrid(
            for: august,
            timeZone: timezone,
            locale: Locale(identifier: "en_US")
        )

        // 2026-08-01 is a Saturday; with a Sunday week start the grid opens on
        // 2026-07-26 and needs six rows (42 cells) to cover the month.
        XCTAssertEqual(grid.days.count, 42)
        XCTAssertEqual(grid.days[0].civilDate, "2026-07-26")
        XCTAssertFalse(grid.days[0].isInMonth)
        XCTAssertEqual(grid.days[6].civilDate, "2026-08-01")
        XCTAssertTrue(grid.days[6].isInMonth)
        XCTAssertEqual(grid.days[6].dayNumber, 1)
        XCTAssertEqual(grid.days.last?.civilDate, "2026-09-05")
        XCTAssertEqual(grid.title, "August 2026")
        XCTAssertEqual(grid.weekdaySymbols.first, "S")
        XCTAssertEqual(
            grid.monthStart,
            try DateRangePickerConversion.date(from: "2026-08-01", timeZone: timezone)
        )
    }

    func testMonthGridFitsExactWeeksWithoutPaddingRows() throws {
        let timezone = try XCTUnwrap(TimeZone(identifier: "UTC"))
        // 2026-02-01 is a Sunday and February has 28 days: exactly four rows.
        let february = try DateRangePickerConversion.date(from: "2026-02-10", timeZone: timezone)

        let grid = DateRangePickerConversion.monthGrid(
            for: february,
            timeZone: timezone,
            locale: Locale(identifier: "en_US")
        )

        XCTAssertEqual(grid.days.count, 28)
        XCTAssertEqual(grid.days.first?.civilDate, "2026-02-01")
        XCTAssertEqual(grid.days.last?.civilDate, "2026-02-28")
        XCTAssertTrue(grid.days.allSatisfy(\.isInMonth))
    }

    func testMonthShiftCrossesYearBoundary() throws {
        let timezone = try XCTUnwrap(TimeZone(identifier: "Asia/Tokyo"))
        let january = try DateRangePickerConversion.date(from: "2026-01-01", timeZone: timezone)

        let forward = DateRangePickerConversion.shiftingMonth(january, by: 1, timeZone: timezone)
        let backward = DateRangePickerConversion.shiftingMonth(january, by: -1, timeZone: timezone)

        XCTAssertEqual(
            DateRangePickerConversion.civilDate(from: forward, timeZone: timezone),
            "2026-02-01"
        )
        XCTAssertEqual(
            DateRangePickerConversion.civilDate(from: backward, timeZone: timezone),
            "2025-12-01"
        )
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
