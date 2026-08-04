import AppKit
import SwiftUI

public enum MenuBarLayout {
    public static let panelWidth: CGFloat = 400
    /// Cap the whole popover at this fraction of the screen’s visible height.
    public static let panelMaxHeightScreenFraction: CGFloat = 0.80
    public static let horizontalPadding: CGFloat = 18
    public static let sectionSpacing: CGFloat = 22
    /// CLIENT / PROJECT / MODEL lists: collapsed page size; chevron loads another page.
    public static let listPageSize = 5
    /// Nested models under each PROJECT row: collapsed page size; More loads another page.
    public static let projectModelPageSize = 3
    public static let shareBarHeight: CGFloat = 2
    public static let chartHeight: CGFloat = 128
    /// Fallback when content has not been measured yet (loading / first paint).
    public static let fallbackContentHeight: CGFloat = 280

    /// Screen where the menu bar panel should size itself.
    /// Prefer the status-item / anchor window’s screen; else the screen under the
    /// mouse (click target on multi-monitor); else `NSScreen.main`.
    public static func presentationScreen(anchor: NSView? = nil) -> NSScreen? {
        if let screen = anchor?.window?.screen {
            return screen
        }
        if let frame = anchor?.window?.frame {
            if let screen = NSScreen.screens.first(where: { $0.frame.intersects(frame) }) {
                return screen
            }
        }
        let mouse = NSEvent.mouseLocation
        if let screen = NSScreen.screens.first(where: { NSMouseInRect(mouse, $0.frame, false) }) {
            return screen
        }
        return NSScreen.main
    }

    /// Max total panel height = 80% of the *presentation* screen’s visible frame
    /// (menu bar / dock excluded). Pass `anchor` (status item button) so multi-monitor
    /// setups size against the display where the user opened the panel — not always main.
    public static func panelMaxHeight(screen: NSScreen? = nil, anchor: NSView? = nil) -> CGFloat {
        let resolved = screen ?? presentationScreen(anchor: anchor)
        let visible = resolved?.visibleFrame.height ?? 900
        return floor(visible * panelMaxHeightScreenFraction)
    }
}

/// Live max panel height for the display currently presenting the menu.
/// AppDelegate refreshes this from the status-item screen on open / resize.
@MainActor
public final class PanelLayoutState: ObservableObject {
    @Published public private(set) var maxHeight: CGFloat

    public init(maxHeight: CGFloat = MenuBarLayout.panelMaxHeight()) {
        self.maxHeight = maxHeight
    }

    /// Recompute from the status-item (or mouse) screen; no-op if unchanged.
    @discardableResult
    public func refresh(anchor: NSView? = nil) -> CGFloat {
        let next = MenuBarLayout.panelMaxHeight(anchor: anchor)
        if abs(next - maxHeight) > 0.5 {
            maxHeight = next
        }
        return maxHeight
    }
}

