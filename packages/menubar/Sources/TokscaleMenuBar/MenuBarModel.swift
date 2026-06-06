import AppKit
import SwiftUI
import TokscaleMenuBarCore

@MainActor
final class MenuBarModel: ObservableObject {
    @Published var summary: TokscaleSummary?
    @Published var dashboard: TokscaleDashboardModel?
    @Published private(set) var menuBarImage: NSImage?
    @Published var errorMessage: String?
    @Published var isRefreshing = false
    @Published var refreshStatus: String?

    private let store = TokscaleSummaryStore()
    private var refreshTimer: Timer?
    private var lastAutoRefresh = Date()

    init() {
        reload()
        refreshTimer = Timer.scheduledTimer(withTimeInterval: 60, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.tick() }
        }
    }

    func reload() {
        do {
            let loaded = try store.load()
            summary = loaded
            dashboard = loaded.map { TokscaleDashboardModel(summary: $0) }
            menuBarImage = MenuBarBadgeRenderer.image(for: loaded)
            errorMessage = nil
        } catch {
            summary = nil
            dashboard = nil
            menuBarImage = nil
            errorMessage = error.localizedDescription
        }
    }

    private func tick() {
        reload()
        let auto = AutoRefresh(storedValue: UserDefaults.standard.string(forKey: AutoRefresh.storageKey))
        guard !isRefreshing, let interval = auto.interval else {
            return
        }
        if Date().timeIntervalSince(lastAutoRefresh) < interval {
            return
        }
        lastAutoRefresh = Date()
        refreshQuota(status: "Auto-refreshing quota...")
    }

    func refreshScan() {
        runRefresh(
            status: "Scanning local AI sessions...",
            arguments: ["--no-spinner", "companion-summary", "--refresh"]
        )
    }

    func refreshQuota(status: String = "Refreshing live quota...") {
        runRefresh(
            status: status,
            arguments: ["--no-spinner", "companion-summary", "--refresh-quota"]
        )
    }

    func refreshOnOpenIfNeeded() {
        guard !isRefreshing else { return }
        // No cache yet: initialize with a full scan.
        if summary == nil {
            refreshScan()
            return
        }
        let cadence = RefreshCadence(
            storedValue: UserDefaults.standard.string(forKey: RefreshCadence.storageKey)
        )
        guard let minimumInterval = cadence.minimumInterval else { return }
        // Full scan (not quota-only) so tokens / contribution / subagent usage
        // refreshes when the popover opens. Throttled on usage freshness
        // (generatedAt) so frequent background quota refreshes don't mask stale usage.
        guard summary?.needsScanOnOpen(minimumInterval: minimumInterval) ?? true else { return }
        refreshScan()
    }

    func openTokensCI() {
        if let url = URL(string: "https://tokens.ci/settings") {
            NSWorkspace.shared.open(url)
        }
    }

    func revealCache() {
        if FileManager.default.fileExists(atPath: store.summaryURL.path) {
            NSWorkspace.shared.activateFileViewerSelecting([store.summaryURL])
            return
        }
        NSWorkspace.shared.open(store.summaryURL.deletingLastPathComponent())
    }

    func quit() {
        NSApp.terminate(nil)
    }

    private func runRefresh(status: String, arguments: [String]) {
        guard !isRefreshing else { return }
        isRefreshing = true
        refreshStatus = status
        DispatchQueue.global(qos: .utility).async { [weak self] in
            let result = MenuBarModel.runCompanionCommand(arguments: arguments)
            Task { @MainActor in
                guard let self else { return }
                self.isRefreshing = false
                self.refreshStatus = result
                self.reload()
            }
        }
    }

    nonisolated private static func runCompanionCommand(arguments: [String]) -> String {
        var lastFailure: String?
        for path in companionRefreshCandidates() where FileManager.default.isExecutableFile(atPath: path) {
            let result = runCompanionRefreshProcess(
                executableURL: URL(fileURLWithPath: path),
                arguments: arguments
            )
            if result.success {
                return "Refresh finished."
            }
            lastFailure = result.message
        }
        let result = runCompanionRefreshProcess(
            executableURL: URL(fileURLWithPath: "/usr/bin/env"),
            arguments: ["tokens"] + arguments
        )
        if result.success {
            return "Refresh finished."
        }
        return "Refresh failed: \(result.message ?? lastFailure ?? "tokens command unavailable")"
    }

    nonisolated private static func companionRefreshCandidates() -> [String] {
        // Prefer ~/.local/bin (not a Desktop/Documents TCC-protected folder, so no
        // access prompt) where a current build with `companion-summary` is installed;
        // the published Homebrew binary predates that subcommand. Never probe a
        // repo-relative path under ~/Desktop. Falls back to PATH lookup of `tokens`.
        let localBin = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent(".local/bin/tokens", isDirectory: false)
            .path
        return dedupePaths([
            localBin,
            "/opt/homebrew/bin/tokens",
            "/usr/local/bin/tokens",
        ])
    }

    nonisolated private static func dedupePaths(_ paths: [String]) -> [String] {
        var seen = Set<String>()
        return paths.filter { seen.insert($0).inserted }
    }

    nonisolated private static func runCompanionRefreshProcess(
        executableURL: URL,
        arguments: [String]
    ) -> (success: Bool, message: String?) {
        let process = Process()
        let error = Pipe()
        process.executableURL = executableURL
        process.arguments = arguments
        process.environment = ProcessInfo.processInfo.environment.merging(
            ["PATH": "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"],
            uniquingKeysWith: { _, new in new }
        )
        process.standardError = error
        process.standardOutput = FileHandle.nullDevice

        let finished = DispatchSemaphore(value: 0)
        process.terminationHandler = { _ in finished.signal() }

        do {
            try process.run()
        } catch {
            return (false, error.localizedDescription)
        }

        if finished.wait(timeout: .now() + 30) == .timedOut {
            process.terminate()
            return (false, "refresh timed out")
        }

        if process.terminationStatus == 0 {
            return (true, nil)
        }
        let data = error.fileHandleForReading.readDataToEndOfFile()
        let message = String(data: data, encoding: .utf8)?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return (false, message ?? "exit \(process.terminationStatus)")
    }
}
