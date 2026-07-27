import AppKit
import Combine
import SwiftUI
import TokensMenuBarCore

@main
enum TokensMenuBarMain {
    static func main() {
        let app = NSApplication.shared
        let delegate = AppDelegate()
        app.delegate = delegate
        app.setActivationPolicy(.accessory)
        app.run()
    }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private var statusItem: NSStatusItem!
    private var popover: NSPopover!
    private var store: UsageStore!
    private var settingsWindow: NSWindow?
    private var cancellables = Set<AnyCancellable>()

    func applicationDidFinishLaunching(_ notification: Notification) {
        store = UsageStore()

        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        if let button = statusItem.button {
            button.title = " …"
            button.action = #selector(togglePopover(_:))
            button.target = self
        }
        store.attachStatusItem(statusItem)

        let root = MenuPanelView(store: store, settings: store.settings) { [weak self] size in
            self?.updatePopoverSize(size)
        }
        let hosting = NSHostingController(rootView: root)

        popover = NSPopover()
        // Initial size; shrink-wraps to content up to 80% of screen via onIdealSizeChange.
        let initialHeight = min(680, MenuBarLayout.panelMaxHeight())
        popover.contentSize = NSSize(width: MenuBarLayout.panelWidth, height: initialHeight)
        popover.behavior = .transient
        popover.animates = true
        popover.contentViewController = hosting

        store.$showSettings
            .receive(on: RunLoop.main)
            .sink { [weak self] show in
                if show {
                    self?.presentSettings()
                }
            }
            .store(in: &cancellables)

        store.bootstrap()
    }

    @objc private func togglePopover(_ sender: Any?) {
        guard let button = statusItem.button else { return }
        if popover.isShown {
            popover.performClose(sender)
        } else {
            popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
            popover.contentViewController?.view.window?.makeKey()
        }
    }

    private func presentSettings() {
        store.showSettings = false
        let view = SettingsView(store: store, settings: store.settings)
        let hosting = NSHostingController(rootView: view)
        let window = settingsWindow ?? NSWindow(contentViewController: hosting)
        window.contentViewController = hosting
        window.title = "Settings"
        window.styleMask = [.titled, .closable]
        window.setContentSize(NSSize(width: 420, height: 360))
        window.center()
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        settingsWindow = window
    }

    /// Fit popover to content height, never taller than 80% of the screen.
    private func updatePopoverSize(_ size: CGSize) {
        let width = max(size.width, MenuBarLayout.panelWidth)
        let maxH = MenuBarLayout.panelMaxHeight()
        let height = min(max(size.height, 120), maxH)
        let next = NSSize(width: width, height: height)
        guard abs(popover.contentSize.height - next.height) > 0.5
            || abs(popover.contentSize.width - next.width) > 0.5
        else { return }
        popover.contentSize = next
    }
}
