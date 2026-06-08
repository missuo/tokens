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
    @Published private(set) var isBackgroundScanning = false
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
        let summaryPath = store.summaryURL.path
        runRefresh(status: "Scanning all AI sessions...") {
            MenuBarModel.runFullScan(summaryPath: summaryPath)
        }
    }

    func refreshQuota(status: String = "Refreshing live quota...", completion: (@MainActor () -> Void)? = nil) {
        let summaryURL = store.summaryURL
        runRefresh(status: status, completion: completion) {
            MenuBarModel.runQuotaRefresh(summaryURL: summaryURL)
        }
    }

    // Full local scan is slow (minutes). It runs on the lowest-priority queue with
    // its own flag so it never blocks the fast quota refresh that keeps the first
    // page current.
    private func backgroundFullScan() {
        guard !isBackgroundScanning else { return }
        isBackgroundScanning = true
        let summaryPath = store.summaryURL.path
        DispatchQueue.global(qos: .background).async { [weak self] in
            _ = MenuBarModel.runFullScan(summaryPath: summaryPath)
            Task { @MainActor in
                guard let self else { return }
                self.isBackgroundScanning = false
                self.reload()
            }
        }
    }

    func refreshOnOpenIfNeeded() {
        // No cache yet: do the first full scan in the background so the popover stays
        // responsive while it runs.
        if summary == nil {
            backgroundFullScan()
            return
        }
        // First page wins: always refresh live quota right away (fast, local), even
        // while a background scan is running, so the glance page is current the
        // instant the popover opens. The full usage scan is slow (minutes), so it
        // runs silently in the background afterwards and is throttled hard. With
        // "Refresh on open = Off" only the background scan is skipped, never quota.
        guard !isRefreshing else { return }
        let cadence = RefreshCadence(
            storedValue: UserDefaults.standard.string(forKey: RefreshCadence.storageKey)
        )
        let allowScan = cadence.minimumInterval != nil
        let needsScan = allowScan
            && !isBackgroundScanning
            && (summary?.needsScanOnOpen(minimumInterval: 600) ?? true)
        refreshQuota { [weak self] in
            if needsScan {
                self?.backgroundFullScan()
            }
        }
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

    private func runRefresh(
        status: String,
        completion: (@MainActor () -> Void)? = nil,
        work: @escaping @Sendable () -> String
    ) {
        guard !isRefreshing else { return }
        isRefreshing = true
        refreshStatus = status
        DispatchQueue.global(qos: .utility).async { [weak self] in
            let result = work()
            Task { @MainActor in
                guard let self else { return }
                self.isRefreshing = false
                self.refreshStatus = result
                self.reload()
                completion?()
            }
        }
    }

    // MARK: - scans (graph + usage -> companion summary)

    /// Full scan: `tokens graph --subagents` for usage/history/subagents, `tokens
    /// usage` for live quota, adapted into the companion summary the store reads.
    nonisolated private static func runFullScan(summaryPath: String) -> String {
        guard let binary = tokensBinaryURL() else {
            return "Refresh failed: tokens command unavailable"
        }
        let started = Date()
        let (graphExec, graphArgs) = graphCommand(binary: binary)
        let graph = runCapturing(
            executableURL: graphExec,
            arguments: graphArgs,
            timeout: 300
        )
        guard graph.ok, let graphData = graph.data, !graphData.isEmpty else {
            return "Refresh failed: \(graph.message ?? "graph scan")"
        }
        let usage = runCapturing(
            executableURL: binary,
            arguments: ["usage", "--json"],
            timeout: 45
        )
        let usageData = (usage.ok ? usage.data : nil).flatMap { $0.isEmpty ? nil : $0 }
        let scanMs = Int(Date().timeIntervalSince(started) * 1000)
        let nowISO = ISO8601DateFormatter().string(from: Date())
        do {
            let companion = try GraphCompanionAdapter.companionJSON(
                graphData: graphData,
                usageData: usageData,
                todayDate: localDateString(),
                summaryPath: summaryPath,
                lastScanDurationMs: scanMs,
                quotaRefreshedAt: usageData != nil ? nowISO : nil
            )
            try writeAtomic(companion, to: URL(fileURLWithPath: summaryPath))
            return "Refresh finished."
        } catch {
            return "Refresh failed: \(error)"
        }
    }

    /// Quota-only refresh: run `tokens usage` and patch the existing summary. Keeps
    /// the previous quota when the fetch returns nothing, so a transient failure
    /// doesn't blank the badge to "No live".
    nonisolated private static func runQuotaRefresh(summaryURL: URL) -> String {
        guard let binary = tokensBinaryURL() else {
            return "Refresh failed: tokens command unavailable"
        }
        guard let existing = try? Data(contentsOf: summaryURL) else {
            // No summary yet — a quota-only refresh has nothing to patch.
            return "Refresh skipped: no summary yet"
        }
        let usage = runCapturing(
            executableURL: binary,
            arguments: ["usage", "--json"],
            timeout: 45
        )
        guard usage.ok, let usageData = usage.data, !usageData.isEmpty else {
            return "Quota unavailable (kept previous)."
        }
        let nowISO = ISO8601DateFormatter().string(from: Date())
        guard let patched = GraphCompanionAdapter.patchedQuota(
            companionData: existing,
            usageData: usageData,
            quotaRefreshedAt: nowISO
        ) else {
            return "Quota unavailable (kept previous)."
        }
        do {
            try writeAtomic(patched, to: summaryURL)
            return "Refresh finished."
        } catch {
            return "Refresh failed: \(error)"
        }
    }

    nonisolated private static func writeAtomic(_ data: Data, to url: URL) throws {
        let tmp = url.appendingPathExtension("tmp")
        try data.write(to: tmp, options: .atomic)
        _ = try FileManager.default.replaceItemAt(url, withItemAt: tmp)
    }

    nonisolated private static func localDateString() -> String {
        let formatter = DateFormatter()
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyy-MM-dd"
        return formatter.string(from: Date())
    }

    /// The full scan reads from an optional `tokens-graph-merged` wrapper when one is
    /// installed (it folds in cross-machine history and scans the merged home),
    /// otherwise a plain local `graph --subagents`. Either way the output is the same
    /// graph JSON the adapter consumes, so the app stays portable without the wrapper.
    nonisolated private static func graphCommand(binary: URL) -> (URL, [String]) {
        let wrapper = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent(".local/bin/tokens-graph-merged", isDirectory: false)
        if FileManager.default.isExecutableFile(atPath: wrapper.path) {
            return (wrapper, [])
        }
        return (binary, ["graph", "--subagents", "--no-spinner"])
    }

    nonisolated private static func tokensBinaryURL() -> URL? {
        // Prefer ~/.local/bin (not a Desktop/Documents TCC-protected folder, so no
        // access prompt). The published Homebrew binary may lag the `--subagents`
        // flag, so the local build installed there is the source of truth.
        let candidates = dedupePaths([
            FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent(".local/bin/tokens", isDirectory: false).path,
            "/opt/homebrew/bin/tokens",
            "/usr/local/bin/tokens",
        ])
        for path in candidates where FileManager.default.isExecutableFile(atPath: path) {
            return URL(fileURLWithPath: path)
        }
        return nil
    }

    nonisolated private static func dedupePaths(_ paths: [String]) -> [String] {
        var seen = Set<String>()
        return paths.filter { seen.insert($0).inserted }
    }

    nonisolated private static func runCapturing(
        executableURL: URL,
        arguments: [String],
        timeout: TimeInterval
    ) -> (data: Data?, ok: Bool, message: String?) {
        let process = Process()
        let out = Pipe()
        let err = Pipe()
        process.executableURL = executableURL
        process.arguments = arguments
        process.environment = ProcessInfo.processInfo.environment.merging(
            ["PATH": "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"],
            uniquingKeysWith: { _, new in new }
        )
        process.standardOutput = out
        process.standardError = err

        // Drain stdout on a background queue so a large payload can't deadlock on a
        // full pipe buffer before the process exits.
        let outHandle = out.fileHandleForReading
        let collected = DispatchSemaphore(value: 0)
        let box = DataBox()
        DispatchQueue.global(qos: .utility).async {
            box.data = outHandle.readDataToEndOfFile()
            collected.signal()
        }

        let finished = DispatchSemaphore(value: 0)
        process.terminationHandler = { _ in finished.signal() }
        do {
            try process.run()
        } catch {
            return (nil, false, error.localizedDescription)
        }
        if finished.wait(timeout: .now() + timeout) == .timedOut {
            process.terminate()
            return (nil, false, "refresh timed out")
        }
        _ = collected.wait(timeout: .now() + 5)
        if process.terminationStatus != 0 {
            let errData = err.fileHandleForReading.readDataToEndOfFile()
            let message = String(data: errData, encoding: .utf8)?
                .trimmingCharacters(in: .whitespacesAndNewlines)
            return (box.data, false, message ?? "exit \(process.terminationStatus)")
        }
        return (box.data, true, nil)
    }
}

/// Reference box so the background read closure can hand the captured bytes back
/// without a captured `var` data race.
private final class DataBox: @unchecked Sendable {
    var data = Data()
}
