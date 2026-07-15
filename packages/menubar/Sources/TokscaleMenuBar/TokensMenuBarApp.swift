import AppKit
import SwiftUI

@main
struct TokensMenuBarApp: App {
    @StateObject private var model = MenuBarModel()

    init() {
        // Menu-bar-only app: keep the status item, stay out of the Dock.
        NSApplication.shared.setActivationPolicy(.accessory)
    }

    var body: some Scene {
        MenuBarExtra {
            TokensPopoverView(model: model)
                .onAppear {
                    model.refreshOnOpenIfNeeded()
                }
                // onAppear only fires once for a MenuBarExtra(.window) — the content
                // view stays alive across open/close. Refresh on every open by also
                // listening for the popover window becoming key. refreshOnOpenIfNeeded
                // is throttled, so frequent opens don't trigger redundant scans.
                .onReceive(
                    NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)
                ) { _ in
                    model.refreshOnOpenIfNeeded()
                }
        } label: {
            MenuBarLabelView(image: model.menuBarImage)
        }
        .menuBarExtraStyle(.window)
    }
}
