import AppKit
import SwiftUI

@main
struct TokensMenuBarApp: App {
    @StateObject private var model = MenuBarModel()

    var body: some Scene {
        MenuBarExtra {
            TokensPopoverView(model: model)
                .environment(\.panelVisible, model.isPanelVisible)
                .onAppear {
                    model.panelDidShow()
                    model.refreshOnOpenIfNeeded()
                }
                // onAppear only fires once for a MenuBarExtra(.window) — the content
                // view stays alive across open/close. Refresh on every open by also
                // listening for the popover window becoming key. refreshOnOpenIfNeeded
                // is throttled, so frequent opens don't trigger redundant scans.
                .onReceive(
                    NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)
                ) { _ in
                    model.panelDidShow()
                    model.refreshOnOpenIfNeeded()
                }
                // Pause the looping animations whenever the panel goes away —
                // resign-key fires when the popover closes from clicking
                // elsewhere, will-close when it's dismissed outright.
                .onReceive(
                    NotificationCenter.default.publisher(for: NSWindow.didResignKeyNotification)
                ) { _ in
                    model.panelDidHide()
                }
                .onReceive(
                    NotificationCenter.default.publisher(for: NSWindow.willCloseNotification)
                ) { _ in
                    model.panelDidHide()
                }
                // Key/close don't cover every dismissal path (toggling via the
                // status item orders the panel out without closing it), but
                // ordering in/out always flips occlusion. The status item's own
                // window stays visible forever, so it never emits this.
                .onReceive(
                    NotificationCenter.default.publisher(
                        for: NSWindow.didChangeOcclusionStateNotification)
                ) { note in
                    guard let window = note.object as? NSWindow else { return }
                    if window.occlusionState.contains(.visible) {
                        model.panelDidShow()
                    } else {
                        model.panelDidHide()
                    }
                }
        } label: {
            MenuBarLabelView(image: model.menuBarImage)
        }
        .menuBarExtraStyle(.window)
    }
}
