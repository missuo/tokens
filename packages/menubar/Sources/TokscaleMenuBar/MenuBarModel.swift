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

    init() {
        reload()
        refreshTimer = Timer.scheduledTimer(withTimeInterval: 60, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.reload() }
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

    func refreshScan() {
        runRefresh(
            status: "Scanning local AI sessions...",
            arguments: ["--no-spinner", "companion-summary", "--refresh", "--json"]
        )
    }

    func refreshQuota(status: String = "Refreshing live quota...") {
        runRefresh(
            status: status,
            arguments: ["--no-spinner", "companion-summary", "--refresh-quota", "--json"]
        )
    }

    func refreshQuotaOnOpenIfNeeded() {
        guard !isRefreshing else { return }
        let cadence = RefreshCadence(
            storedValue: UserDefaults.standard.string(forKey: RefreshCadence.storageKey)
        )
        guard let minimumInterval = cadence.minimumInterval else { return }
        guard summary?.needsRefreshOnOpen(minimumInterval: minimumInterval) ?? true else { return }
        refreshQuota(status: "Refreshing quota on open...")
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
        // Only stable install locations. Never probe a repo-relative path: when the
        // .app lives under ~/Desktop, touching it trips the macOS Desktop-access prompt
        // and uses a slow unsigned debug binary. Falls back to PATH lookup of `tokens`.
        dedupePaths([
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

        do {
            try process.run()
            process.waitUntilExit()
        } catch {
            return (false, error.localizedDescription)
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
