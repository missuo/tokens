import XCTest

@testable import TokscaleMenuBarCore

final class ContributionCalendarGridTests: XCTestCase {
    func testBuildsExactly365CalendarDaysEndingAtExplicitDate() throws {
        let grid = ContributionCalendarGrid.build(
            days: [
                (date: "2025-07-13", costUsd: 999),
                (date: "2025-07-14", costUsd: 1),
                (date: "2026-07-13", costUsd: 100),
                (date: "2026-07-14", costUsd: 999),
            ],
            endDate: "2026-07-13"
        )
        let cells = grid.flatMap { $0 }

        XCTAssertEqual(grid.count, 53)
        XCTAssertTrue(grid.allSatisfy { $0.count == 7 })
        XCTAssertEqual(cells.count, 371)
        XCTAssertEqual(cells[0], -1)
        XCTAssertEqual(cells[1], 1)
        XCTAssertEqual(cells[365], 4)
        XCTAssertEqual(Array(cells[366...370]), [-1, -1, -1, -1, -1])
    }

    func testEndDateDoesNotCollapseToLastActiveDay() {
        let grid = ContributionCalendarGrid.build(
            days: [(date: "2026-07-03", costUsd: 10)],
            endDate: "2026-07-13"
        )
        let cells = grid.flatMap { $0 }

        XCTAssertEqual(cells[355], 1)
        XCTAssertEqual(cells[365], 0)
        XCTAssertEqual(Array(cells.suffix(5)), [-1, -1, -1, -1, -1])
    }
}
