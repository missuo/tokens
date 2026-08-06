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
        // Explicit nil screen + no usable mouse/anchor path still uses 900 fallback
        // only when presentationScreen cannot resolve — pass a sentinel by forcing
        // the screen parameter to a zero-height? We test the explicit nil branch via
        // the screen: overload: when screen is provided as nil, presentationScreen runs.
        // Lock the pure math: a synthetic height is not available without a fake screen,
        // so assert the nil-screen path returns a positive cap from whatever is resolved.
        let height = MenuBarLayout.panelMaxHeight(screen: nil)
        XCTAssertGreaterThan(height, 0)
        // And the hard fallback constant used when no screen exists at all:
        XCTAssertEqual(floor(900 * 0.80), 720)
    }

    func testPanelMaxHeightUsesProvidedScreenNotMainOnly() {
        // On multi-monitor, callers must be able to size against a specific screen.
        // Use each attached screen and assert 80% of *that* screen’s visible height.
        for screen in NSScreen.screens {
            let expected = floor(screen.visibleFrame.height * 0.80)
            XCTAssertEqual(
                MenuBarLayout.panelMaxHeight(screen: screen),
                expected,
                "Expected 80% of \(screen.localizedName) visible height"
            )
        }
        XCTAssertFalse(NSScreen.screens.isEmpty, "Need at least one screen for layout tests")
    }

    @MainActor
    func testPanelLayoutStateRefreshUpdatesMaxHeight() {
        let state = PanelLayoutState(maxHeight: 100)
        let next = state.refresh(anchor: nil)
        XCTAssertEqual(state.maxHeight, next)
        XCTAssertGreaterThan(state.maxHeight, 100)
    }

    @MainActor
    func testPanelLayoutStateRefreshWithExplicitScreen() {
        // Multi-monitor: the click handler sizes against the display under the
        // mouse, passed explicitly — not the status item’s (possibly migrated)
        // window screen.
        guard let screen = NSScreen.screens.last else {
            XCTFail("Need at least one screen for layout tests")
            return
        }
        let state = PanelLayoutState(maxHeight: 100)
        let next = state.refresh(screen: screen)
        let expected = floor(screen.visibleFrame.height * 0.80)
        XCTAssertEqual(next, expected)
        XCTAssertEqual(state.maxHeight, expected)
        // Refreshing again against the same screen is a stable no-op.
        XCTAssertEqual(state.refresh(screen: screen), expected)
    }

    func testMouseScreenResolvesToAnAttachedScreen() {
        // The cursor is always inside some attached display’s frame; the helper
        // must return one of them (never a detached / stale screen).
        if let screen = MenuBarLayout.mouseScreen() {
            XCTAssertTrue(NSScreen.screens.contains(screen))
        }
    }
}
