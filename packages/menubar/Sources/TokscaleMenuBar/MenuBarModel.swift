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
    // The MenuBarExtra(.window) content view stays alive while the panel is
    // closed, so its repeat-forever animations would keep driving full-rate
    // render passes forever (~13% CPU measured idle). Views gate their looping
    // animations on this so a hidden panel costs nothing.
    @Published private(set) var isPanelVisible = false

    private let store = TokscaleSummaryStore()
    private var refreshTimer: Timer?
    private var lastAutoRefresh = Date()
    // Two-tier scan throttles: today's spend is cheap to rescan (`--today-only`,
    // ~2s) so it refreshes often; the full history scan is expensive (merged, tens
    // of seconds) so it runs at most daily. A full scan also covers today, so it
    // bumps both stamps.
    private var lastTodayScan = Date.distantPast
    private var lastHistoryScan = Date.distantPast

    init() {
        reload()
        refreshTimer = Timer.scheduledTimer(withTimeInterval: 60, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.tick() }
        }
        // Let the system coalesce the wakeup with others instead of waking the
        // CPU at the exact second; the tick doesn't need to be punctual.
        refreshTimer?.tolerance = 10
    }

    func panelDidShow() {
        isPanelVisible = true
    }

    func panelDidHide() {
        isPanelVisible = false
    }

    func reload() {
        do {
            let loaded = try store.load()
            // Skip the dashboard rebuild + badge re-render when the data version is
            // unchanged — the 60s tick reloads even when nothing was scanned, and
            // rebuilding the model + rendering the NSImage every minute is wasteful.
            if let loaded, let current = summary, errorMessage == nil,
                loaded.generatedAt == current.generatedAt,
                loaded.health.quotaRefreshedAt == current.health.quotaRefreshedAt
            {
                return
            }
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
            let result = MenuBarModel.runFullScan(summaryPath: summaryPath)
            Task { @MainActor in
                guard let self else { return }
                self.isBackgroundScanning = false
                self.refreshStatus = result
                self.reload()
                if result.hasPrefix("Refresh failed") {
                    if self.summary == nil {
                        self.errorMessage = result
                    }
                    return
                }
                self.lastHistoryScan = Date()
                // The merged history scan owns all-time/history, but its per-client
                // today work time can be noisy (cross-machine duplicates / stray
                // timestamps). Chain a local today-only scan to fill in accurate
                // work time + today's spend; it reloads once done, so the panel
                // never flashes an empty work-time line between the two scans.
                self.backgroundTodayScan()
            }
        }
    }

    // Cheap today-only rescan: re-reads just today's files (~2s) and patches the
    // today figures + work time over the existing summary, leaving history alone.
    private func backgroundTodayScan() {
        guard !isBackgroundScanning else { return }
        isBackgroundScanning = true
        let summaryURL = store.summaryURL
        DispatchQueue.global(qos: .background).async { [weak self] in
            let result = MenuBarModel.runTodayScan(summaryURL: summaryURL)
            Task { @MainActor in
                guard let self else { return }
                self.isBackgroundScanning = false
                self.refreshStatus = result
                self.lastTodayScan = Date()
                self.reload()
            }
        }
    }

    func refreshOnOpenIfNeeded() {
        let cadence = RefreshCadence(
            storedValue: UserDefaults.standard.string(forKey: RefreshCadence.storageKey)
        )
        guard let interval = cadence.minimumInterval else {
            return
        }
        // No cache yet: do the first full scan in the background so the popover stays
        // responsive while it runs.
        if summary == nil {
            backgroundFullScan()
            return
        }
        // First page wins: refresh live quota when the open cadence says the cached
        // summary is old enough, then pick the cheapest scan that's due.
        guard !isRefreshing else { return }
        let needsQuotaRefresh = summary?.needsRefreshOnOpen(minimumInterval: interval) ?? true
        let needsUsageScan = summary?.needsScanOnOpen(minimumInterval: interval) ?? true
        let runDueScan: @MainActor () -> Void = { [weak self] in
            guard let self, needsUsageScan, !self.isBackgroundScanning else {
                return
            }
            let now = Date()
            if now.timeIntervalSince(self.lastHistoryScan) > 86_400 {
                self.backgroundFullScan()
            } else if now.timeIntervalSince(self.lastTodayScan) > 600 {
                self.backgroundTodayScan()
            }
        }
        if needsQuotaRefresh {
            refreshQuota(completion: runDueScan)
        } else {
            runDueScan()
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

    /// Cheap today-only refresh: rescan just today's files (`graph --today-only`,
    /// ~2s) and patch today's spend + work time over the existing summary, leaving
    /// the slow-to-scan history untouched. Stays on the local binary (today's data
    /// is all local), so it skips the merged-home wrapper the full scan uses.
    nonisolated private static func runTodayScan(summaryURL: URL) -> String {
        guard let binary = tokensBinaryURL() else {
            return "Refresh failed: tokens command unavailable"
        }
        guard let existing = try? Data(contentsOf: summaryURL), !existing.isEmpty else {
            return "Today refresh skipped: no summary yet"
        }
        let graph = runCapturing(
            executableURL: binary,
            arguments: ["graph", "--today-only", "--work-time", "--no-spinner"],
            timeout: 60
        )
        guard graph.ok, let graphData = graph.data, !graphData.isEmpty else {
            return "Today refresh failed: \(graph.message ?? "graph scan")"
        }
        guard
            let patched = GraphCompanionAdapter.patchTodayData(
                companionData: existing,
                todayGraphData: graphData,
                todayDate: localDateString()
            )
        else {
            return "Today refresh failed: patch"
        }
        do {
            try writeAtomic(patched, to: summaryURL)
            return "Refresh finished."
        } catch {
            return "Today refresh failed: \(error)"
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
        for wrapper in graphWrapperCandidates() where FileManager.default.isExecutableFile(atPath: wrapper.path) {
            return (wrapper, [])
        }
        return (binary, ["graph", "--subagents", "--work-time", "--no-spinner"])
    }

    nonisolated private static func tokensBinaryURL() -> URL? {
        // Prefer the bundled CLI so the app cannot silently bind to an older Homebrew
        // or ~/.local/bin build that lacks graph flags the menu bar depends on.
        let candidates = dedupePaths([
            Bundle.main.executableURL?
                .deletingLastPathComponent()
                .appendingPathComponent("tokens", isDirectory: false)
                .path,
            localTokensPath(),
            "/opt/homebrew/bin/tokens",
            "/usr/local/bin/tokens",
        ].compactMap { $0 })
        for path in candidates where FileManager.default.isExecutableFile(atPath: path) {
            if tokensCandidateSupportsGraph(path) {
                return URL(fileURLWithPath: path)
            }
        }
        return nil
    }

    nonisolated private static func graphWrapperCandidates() -> [URL] {
        let bundled = Bundle.main.executableURL?
            .deletingLastPathComponent()
            .appendingPathComponent("tokens-graph-merged", isDirectory: false)
        let local = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent(".local/bin/tokens-graph-merged", isDirectory: false)
        return [bundled, local].compactMap { $0 }.filter { wrapper in
            if wrapper == local {
                return tokensCandidateSupportsGraph(localTokensPath())
            }
            return true
        }
    }

    nonisolated private static func localTokensPath() -> String {
        FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent(".local/bin/tokens", isDirectory: false)
            .path
    }

    nonisolated private static func tokensCandidateSupportsGraph(_ path: String) -> Bool {
        let result = runCapturing(
            executableURL: URL(fileURLWithPath: path),
            arguments: ["graph", "--help"],
            timeout: 5
        )
        guard result.ok, let data = result.data, let help = String(data: data, encoding: .utf8) else {
            return false
        }
        return help.contains("--subagents") && help.contains("--work-time")
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
