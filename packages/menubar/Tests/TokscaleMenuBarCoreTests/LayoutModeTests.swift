import XCTest
@testable import TokscaleMenuBarCore

final class LayoutModeTests: XCTestCase {
    func testDefaultsToPaged() {
        XCTAssertEqual(LayoutMode.default, .paged)
        XCTAssertEqual(LayoutMode(storedValue: nil), .paged)
        XCTAssertEqual(LayoutMode(storedValue: "bogus"), .paged)
    }

    func testRoundTripsKnownValues() {
        for mode in LayoutMode.allCases {
            XCTAssertEqual(LayoutMode(storedValue: mode.rawValue), mode)
        }
    }

    func testTitles() {
        XCTAssertEqual(LayoutMode.single.title, "Single")
        XCTAssertEqual(LayoutMode.paged.title, "Paged")
    }
}
