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
    /// Nested models under each PROJECT row: collapsed page size; expand control loads another page.
    public static let projectModelPageSize = 3
    public static let shareBarHeight: CGFloat = 2
    public static let chartHeight: CGFloat = 128
    /// Fallback when content has not been measured yet (loading / first paint).
    public static let fallbackContentHeight: CGFloat = 280

    /// Screen under the mouse cursor — the display whose menu bar received the
    /// click. More reliable than `anchor.window.screen` at click time: once the
    /// popover takes key focus, macOS can migrate the active menu bar (and the
    /// status item’s window) to another display mid-presentation.
    public static func mouseScreen() -> NSScreen? {
        let mouse = NSEvent.mouseLocation
        return NSScreen.screens.first(where: { NSMouseInRect(mouse, $0.frame, false) })
    }

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
        if let screen = mouseScreen() {
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
/// AppDelegate sizes this from the clicked display (mouse screen) before show;
/// the screen-parameters path re-clamps from the status-item anchor.
@MainActor
public final class PanelLayoutState: ObservableObject {
    @Published public private(set) var maxHeight: CGFloat
    @Published public private(set) var presentationGeneration = 0

    public init(maxHeight: CGFloat = MenuBarLayout.panelMaxHeight()) {
        self.maxHeight = maxHeight
    }

    public func willPresent() {
        presentationGeneration &+= 1
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

    /// Recompute against an explicit presentation screen (the display whose
    /// menu bar was clicked); no-op if unchanged.
    @discardableResult
    public func refresh(screen: NSScreen) -> CGFloat {
        let next = MenuBarLayout.panelMaxHeight(screen: screen)
        if abs(next - maxHeight) > 0.5 {
            maxHeight = next
        }
        return maxHeight
    }
}

