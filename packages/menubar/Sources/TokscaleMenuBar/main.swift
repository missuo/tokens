import AppKit
import SwiftUI
import TokscaleMenuBarCore

@MainActor
final class TokensMenuBarState: ObservableObject {
    @Published var summary: TokscaleSummary?
    @Published var errorMessage: String?
    @Published var isRefreshing = false
    @Published var refreshStatus: String?
}

@MainActor
final class MenuBarController: NSObject, NSApplicationDelegate {
    private let store = TokscaleSummaryStore()
    private let viewState = TokensMenuBarState()
    private let popoverContentSize = NSSize(width: 500, height: 580)
    private var statusItem: NSStatusItem?
    private let popover = NSPopover()
    private var hostingController: NSHostingController<TokensPopoverView>?
    private var refreshTimer: Timer?

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        popover.behavior = .transient
        popover.animates = false
        popover.contentSize = popoverContentSize

        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        statusItem = item
        if let button = item.button {
            button.font = NSFont.monospacedDigitSystemFont(
                ofSize: NSFont.systemFontSize,
                weight: .regular
            )
            button.toolTip = "Tokens"
            if let image = NSImage(systemSymbolName: "chart.bar.xaxis", accessibilityDescription: "Tokens") {
                image.isTemplate = true
                button.image = image
                button.imagePosition = .imageLeading
            }
            button.target = self
            button.action = #selector(togglePopover)
        }

        let controller = NSHostingController(
            rootView: TokensPopoverView(
                state: viewState,
                onReload: { [weak self] in self?.reload() },
                onRefreshScan: { [weak self] in self?.refreshScan() },
                onOpenTokensCI: { [weak self] in self?.openTokensCI() },
                onRevealCache: { [weak self] in self?.revealCache() },
                onQuit: { [weak self] in self?.quit() }
            )
        )
        controller.sizingOptions = []
        controller.view.frame = NSRect(origin: .zero, size: popoverContentSize)
        hostingController = controller
        popover.contentViewController = controller

        reload()
        refreshTimer = Timer.scheduledTimer(
            timeInterval: 60,
            target: self,
            selector: #selector(reloadFromTimer),
            userInfo: nil,
            repeats: true
        )
    }

    @objc private func reloadFromTimer() {
        reload()
    }

    @objc private func reloadFromMenu() {
        reload()
    }

    @objc private func togglePopover() {
        guard let button = statusItem?.button else {
            return
        }
        if popover.isShown {
            popover.performClose(nil)
            return
        }
        NSApp.activate(ignoringOtherApps: true)
        popover.contentSize = popoverContentSize
        popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
        popover.contentViewController?.view.window?.makeKey()
    }

    @objc private func openTokensCI() {
        if let url = URL(string: "https://tokens.ci/settings") {
            NSWorkspace.shared.open(url)
        }
    }

    private func revealCache() {
        if FileManager.default.fileExists(atPath: store.summaryURL.path) {
            NSWorkspace.shared.activateFileViewerSelecting([store.summaryURL])
            return
        }
        NSWorkspace.shared.open(store.summaryURL.deletingLastPathComponent())
    }

    private func refreshScan() {
        guard !viewState.isRefreshing else {
            return
        }
        viewState.isRefreshing = true
        viewState.refreshStatus = "Scanning local AI sessions..."
        render()

        DispatchQueue.global(qos: .utility).async { [weak self] in
            let result = Self.runCompanionRefresh()
            DispatchQueue.main.async {
                guard let self else {
                    return
                }
                self.viewState.isRefreshing = false
                self.viewState.refreshStatus = result
                self.reload()
            }
        }
    }

    @objc private func quit() {
        NSApp.terminate(nil)
    }

    private func reload() {
        do {
            viewState.summary = try store.load()
            viewState.errorMessage = nil
        } catch {
            viewState.summary = nil
            viewState.errorMessage = error.localizedDescription
        }
        render()
    }

    private func render() {
        statusItem?.button?.title = viewState.summary?.menuBarTitle ?? "AI Tokens"
        popover.contentSize = popoverContentSize
        hostingController?.view.frame = NSRect(origin: .zero, size: popoverContentSize)
    }

    nonisolated private static func runCompanionRefresh() -> String {
        let process = Process()
        let error = Pipe()
        let directCandidates = [
            "/opt/homebrew/bin/tokens",
            "/usr/local/bin/tokens"
        ]

        if let path = directCandidates.first(where: { FileManager.default.isExecutableFile(atPath: $0) }) {
            process.executableURL = URL(fileURLWithPath: path)
            process.arguments = ["--no-spinner", "companion-summary", "--refresh", "--json"]
        } else {
            process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
            process.arguments = ["tokens", "--no-spinner", "companion-summary", "--refresh", "--json"]
        }
        process.environment = ProcessInfo.processInfo.environment.merging(
            ["PATH": "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"],
            uniquingKeysWith: { _, new in new }
        )
        process.standardError = error

        do {
            try process.run()
            process.waitUntilExit()
        } catch {
            return "Refresh failed: \(error.localizedDescription)"
        }

        if process.terminationStatus == 0 {
            return "Refresh finished."
        }
        let data = error.fileHandleForReading.readDataToEndOfFile()
        let message = String(data: data, encoding: .utf8)?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return "Refresh failed: \(message ?? "exit \(process.terminationStatus)")"
    }
}

let app = NSApplication.shared
let delegate = MenuBarController()
app.delegate = delegate
app.run()
