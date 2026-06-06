import XCTest
@testable import TokscaleMenuBarCore

final class LayoutModeTests: XCTestCase {
    func testDefaultsToSingle() {
        XCTAssertEqual(LayoutMode.default, .single)
        XCTAssertEqual(LayoutMode(storedValue: nil), .single)
        XCTAssertEqual(LayoutMode(storedValue: "bogus"), .single)
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
