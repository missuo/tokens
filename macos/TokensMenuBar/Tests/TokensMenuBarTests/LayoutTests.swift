import AppKit
import XCTest
@testable import TokensMenuBarCore

final class LayoutTests: XCTestCase {
    func testPanelMaxHeightIsEightyPercentOfVisibleFrame() {
        let screen = NSScreen.main
        let expectedVisible = screen?.visibleFrame.height ?? 900
        let expected = floor(expectedVisible * 0.80)
        XCTAssertEqual(MenuBarLayout.panelMaxHeight(screen: screen), expected)
        XCTAssertEqual(MenuBarLayout.panelMaxHeightScreenFraction, 0.80, accuracy: 0.0001)
    }

    func testPanelMaxHeightFallsBackWithoutScreen() {
        XCTAssertEqual(MenuBarLayout.panelMaxHeight(screen: nil), floor(900 * 0.80))
    }

    }
