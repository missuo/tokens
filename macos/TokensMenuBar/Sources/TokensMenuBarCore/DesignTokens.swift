import AppKit
import SwiftUI

public enum MenuBarLayout {
    public static let panelWidth: CGFloat = 400
    /// Cap the whole popover at this fraction of the screen’s visible height.
    public static let panelMaxHeightScreenFraction: CGFloat = 0.80
    public static let horizontalPadding: CGFloat = 18
    public static let sectionSpacing: CGFloat = 22
    public static let nestedListMaxHeight: CGFloat = 168
    public static let nestedListThreshold = 8
    public static let shareBarHeight: CGFloat = 2
    public static let chartHeight: CGFloat = 128
    /// Fallback when content has not been measured yet (loading / first paint).
    public static let fallbackContentHeight: CGFloat = 280

    /// Max total panel height = 80% of the screen’s visible frame (menu bar / dock excluded).
    public static func panelMaxHeight(screen: NSScreen? = NSScreen.main) -> CGFloat {
        let visible = screen?.visibleFrame.height ?? 900
        return floor(visible * panelMaxHeightScreenFraction)
    }
}

/// Period-switch motion: height spring + content crossfade (synced, not sequential).
public enum MenuBarMotion {
    /// Popover / body height — stable spring, little overshoot.
    public static let heightSpring: Animation = .spring(response: 0.36, dampingFraction: 0.9)
    /// TOTAL / lists body opacity while period content swaps.
    public static let contentCrossfade: Animation = .easeInOut(duration: 0.2)
    /// AppKit popover `contentSize` duration (pairs with `heightSpring`).
    public static let popoverSizeDuration: TimeInterval = 0.34
}
