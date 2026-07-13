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
    // Two-tier scan throttles: today's spend is cheap to rescan (`--today-only`,
    // ~2s) so it refreshes often; the full history scan is expensive (merged, tens
    // of seconds) so it runs at most daily. A full scan also covers today, so it
    // bumps both stamps.
    private var lastTodayScan = Date.distantPast
    private var lastHistoryScan = Date.distantPast
    private var lastHistoryAttempt = Date.distantPast
    private var lastQuotaOpenAttempt = Date.distantPast
    private static let lastHistoryAttemptKey = "tokens.menubar.lastHistoryAttempt"
    nonisolated private static let summaryMutations = SummaryMutationCoordinator()

    init() {
        reload()
        if let restored = summary.flatMap({
            BackgroundScanPolicy.restoredScanDates(
                generatedAt: $0.generatedAt,
                historyGeneratedAt: $0.health.historyGeneratedAt
            )
        }) {
            lastTodayScan = restored.today
            lastHistoryScan = restored.history
        }
        if let storedAttempt = UserDefaults.standard.object(
            forKey: Self.lastHistoryAttemptKey
        ) as? Date {
            lastHistoryAttempt = storedAttempt
        }
        if summary != nil {
            backgroundTodayScan()
        }
        refreshTimer = Timer.scheduledTimer(withTimeInterval: 60, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.tick() }
        }
        // Let the system coalesce the wakeup with others instead of waking the
        // CPU at the exact second; the tick doesn't need to be punctual.
        refreshTimer?.tolerance = 10
    }

    func reload() {
        do {
            let loaded = try store.load()
            // Skip the dashboard rebuild + badge re-render when the data version is
            // unchanged — the 60s tick reloads even when nothing was scanned, and
            // rebuilding the model + rendering the NSImage every minute is wasteful.
            if let loaded, let current = summary, errorMessage == nil, loaded == current {
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
        if !isRefreshing, !isBackgroundScanning {
            let now = Date()
            switch BackgroundScanPolicy.nextAction(
                now: now,
                lastHistorySuccess: lastHistoryScan,
                lastHistoryAttempt: lastHistoryAttempt,
                lastTodayScan: lastTodayScan
            ) {
            case .full:
                backgroundHistoryRefresh()
            case .today:
                backgroundTodayScan()
            case .none:
                break
            }
        }
        let auto = AutoRefresh(
            storedValue: UserDefaults.standard.string(forKey: AutoRefresh.storageKey))
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
        refreshStatus =
            summary == nil
            ? "Building local history cache..."
            : "Syncing compact history..."
        backgroundHistoryRefresh()
    }

    func refreshQuota(
        status: String = "Refreshing live quota...", completion: (@MainActor () -> Void)? = nil
    ) {
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
        recordHistoryAttempt()
        let summaryPath = store.summaryURL.path
        DispatchQueue.global(qos: .background).async { [weak self] in
            let result = MenuBarModel.runFullScan(summaryPath: summaryPath)
            Task { @MainActor in
                guard let self else { return }
                self.isBackgroundScanning = false
                guard result.succeeded else {
                    self.refreshStatus = result.message
                    return
                }
                self.lastHistoryScan = Date()
                self.refreshStatus = "Local history refreshed."
                // The merged history scan owns all-time/history, but its per-client
                // today work time can be noisy (cross-machine duplicates / stray
                // timestamps). Chain a local today-only scan to fill in accurate
                // work time + today's spend; it reloads once done, so the panel
                // never flashes an empty work-time line between the two scans.
                self.backgroundTodayScan()
            }
        }
    }

    private func backgroundHistoryRefresh() {
        if summary == nil {
            backgroundFullScan()
        } else {
            backgroundRemoteHistorySync()
        }
    }

    private func backgroundRemoteHistorySync() {
        guard !isBackgroundScanning else { return }
        isBackgroundScanning = true
        recordHistoryAttempt()
        let summaryURL = store.summaryURL
        DispatchQueue.global(qos: .background).async { [weak self] in
            let result = MenuBarModel.runRemoteHistorySync(summaryURL: summaryURL)
            Task { @MainActor in
                guard let self else { return }
                self.isBackgroundScanning = false
                if result.succeeded {
                    self.lastHistoryScan = Date()
                    self.refreshStatus = "History refreshed."
                } else {
                    self.refreshStatus = result.message
                }
                switch BackgroundScanPolicy.actionAfterRemoteHistorySync(
                    succeeded: result.succeeded
                ) {
                case .full:
                    self.backgroundFullScan()
                case .today:
                    self.backgroundTodayScan()
                case .none:
                    break
                }
            }
        }
    }

    private func recordHistoryAttempt() {
        let now = Date()
        lastHistoryAttempt = now
        UserDefaults.standard.set(now, forKey: Self.lastHistoryAttemptKey)
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
                if result.succeeded {
                    self.lastTodayScan = Date()
                } else {
                    self.refreshStatus = result.message
                }
                self.reload()
            }
        }
    }

    func refreshOnOpenIfNeeded() {
        // No cache yet: do the first full scan in the background so the popover stays
        // responsive while it runs.
        if summary == nil {
            backgroundHistoryRefresh()
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
        let refreshUsageIfNeeded: @MainActor () -> Void = { [weak self] in
            guard let self, cadence.minimumInterval != nil, !self.isBackgroundScanning else {
                return
            }
            let now = Date()
            switch BackgroundScanPolicy.nextAction(
                now: now,
                lastHistorySuccess: self.lastHistoryScan,
                lastHistoryAttempt: self.lastHistoryAttempt,
                lastTodayScan: self.lastTodayScan
            ) {
            case .full:
                self.backgroundHistoryRefresh()
            case .today:
                self.backgroundTodayScan()
            case .none:
                break
            }
        }
        let now = Date()
        if BackgroundScanPolicy.shouldRefreshQuotaOnOpen(
            needsRefresh: summary?.needsRefreshOnOpen(
                now: now,
                minimumInterval: BackgroundScanPolicy.quotaOpenRefreshInterval
            ) == true,
            isRefreshing: isRefreshing,
            now: now,
            lastAttempt: lastQuotaOpenAttempt
        ) {
            lastQuotaOpenAttempt = now
            refreshQuota(completion: refreshUsageIfNeeded)
        } else {
            refreshUsageIfNeeded()
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
    nonisolated private static func runFullScan(summaryPath: String) -> FullScanResult {
        guard let binary = tokensBinaryURL() else {
            return .failure("Refresh failed: tokens command unavailable")
        }
        let started = Date()
        var graphData: Data?
        var graphFailure = "graph scan"
        for command in graphCommands(binary: binary) {
            let graph = runCapturing(
                executableURL: command.executableURL,
                arguments: command.arguments,
                timeout: 300
            )
            if graph.ok, let data = graph.data, !data.isEmpty,
                GraphCompanionAdapter.isValidGraphData(data)
            {
                graphData = data
                break
            }
            graphFailure = graph.ok
                ? "invalid graph output"
                : graph.message ?? "graph scan"
        }
        guard let graphData else {
            return .failure("Refresh failed: \(graphFailure)")
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
            return summaryMutations.replace(
                summaryURL: URL(fileURLWithPath: summaryPath),
                with: companion
            ) ? .success : .failure("Refresh failed: summary write")
        } catch {
            return .failure("Refresh failed: \(error)")
        }
    }

    nonisolated private static func runRemoteHistorySync(summaryURL: URL) -> FullScanResult {
        guard let binary = tokensBinaryURL() else {
            return .failure("History sync failed: tokens command unavailable")
        }
        let started = Date()
        let status = runCapturing(
            executableURL: binary,
            arguments: ["status", "--json"],
            timeout: 15
        )
        guard status.ok, let statusData = status.data,
            let statusJSON = (try? JSONSerialization.jsonObject(with: statusData))
                as? [String: Any],
            let apiURLString = statusJSON["apiUrl"] as? String,
            let auth = statusJSON["auth"] as? [String: Any],
            let username = auth["username"] as? String,
            !username.isEmpty,
            var profileURL = URL(string: apiURLString),
            profileURL.scheme == "https" || profileURL.scheme == "http"
        else {
            return .failure("History sync unavailable: tokens.ci login not found")
        }
        if !profileURL.path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
            .hasSuffix("api")
        {
            profileURL.appendPathComponent("api")
        }
        profileURL.appendPathComponent("users")
        profileURL.appendPathComponent(username)
        guard var profileComponents = URLComponents(
            url: profileURL,
            resolvingAgainstBaseURL: false
        ) else {
            return .failure("History sync failed: invalid profile URL")
        }
        var queryItems = profileComponents.queryItems ?? []
        queryItems.removeAll { $0.name == "history" }
        queryItems.append(URLQueryItem(name: "history", value: "all"))
        profileComponents.queryItems = queryItems
        guard let fullHistoryURL = profileComponents.url else {
            return .failure("History sync failed: invalid profile URL")
        }

        let fetch = fetch(fullHistoryURL, timeout: 30)
        guard fetch.ok, let profileData = fetch.data, !profileData.isEmpty else {
            return .failure("History sync failed; kept local cache")
        }
        let elapsedMs = Int(Date().timeIntervalSince(started) * 1000)
        let syncedAt = ISO8601DateFormatter().string(from: Date())
        let wrote = summaryMutations.mutate(summaryURL: summaryURL) { latest in
            GraphCompanionAdapter.patchRemoteProfile(
                companionData: latest,
                profileData: profileData,
                todayDate: localDateString(),
                syncedAt: syncedAt,
                lastScanDurationMs: elapsedMs
            )
        }
        guard wrote else {
            return .failure("Remote history rejected; kept corrected local cache")
        }
        return .success
    }

    /// Cheap today-only refresh: rescan just today's files (`graph --today-only`,
    /// ~2s) and patch today's spend + work time over the existing summary, leaving
    /// the slow-to-scan history untouched. Stays on the local binary (today's data
    /// is all local), so it skips the merged-home wrapper the full scan uses.
    nonisolated private static func runTodayScan(summaryURL: URL) -> FullScanResult {
        guard let binary = tokensBinaryURL() else {
            return .failure("Refresh failed: tokens command unavailable")
        }
        let graph = runCapturing(
            executableURL: binary,
            arguments: ["graph", "--today-only", "--work-time", "--no-spinner"],
            timeout: 60
        )
        guard graph.ok, let graphData = graph.data, !graphData.isEmpty else {
            return .failure("Today refresh failed: \(graph.message ?? "graph scan")")
        }
        let wrote = summaryMutations.mutate(summaryURL: summaryURL) { latest in
            GraphCompanionAdapter.patchTodayData(
                companionData: latest,
                todayGraphData: graphData,
                todayDate: localDateString()
            )
        }
        return wrote ? .success : .failure("Today refresh failed: patch or write")
    }

    /// Quota-only refresh: run `tokens usage` and patch the existing summary. Keeps
    /// the previous quota when the fetch returns nothing, so a transient failure
    /// doesn't blank the badge to "No live".
    nonisolated private static func runQuotaRefresh(summaryURL: URL) -> String {
        guard let binary = tokensBinaryURL() else {
            return "Refresh failed: tokens command unavailable"
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
        let wrote = summaryMutations.mutate(summaryURL: summaryURL) { latest in
            GraphCompanionAdapter.patchedQuota(
                companionData: latest,
                usageData: usageData,
                quotaRefreshedAt: nowISO
            )
        }
        return wrote ? "Refresh finished." : "Quota unavailable (kept previous)."
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
    nonisolated private static func graphCommands(binary: URL) -> [GraphCommand] {
        let home = FileManager.default.homeDirectoryForCurrentUser
        let wrapper =
            home
            .appendingPathComponent(".local/bin/tokens-graph-merged", isDirectory: false)
        let mergedHome = home.appendingPathComponent(".cache/tokscale-merged", isDirectory: true)
        var mergedHomeIsDirectory: ObjCBool = false
        let canUseMergedWrapper =
            FileManager.default.isExecutableFile(atPath: wrapper.path)
            && FileManager.default.fileExists(
                atPath: mergedHome.path,
                isDirectory: &mergedHomeIsDirectory
            )
            && mergedHomeIsDirectory.boolValue
        var commands: [GraphCommand] = []
        if canUseMergedWrapper {
            commands.append(GraphCommand(executableURL: wrapper, arguments: []))
        }
        commands.append(
            GraphCommand(
                executableURL: binary,
                arguments: ["graph", "--subagents", "--work-time", "--no-spinner"]
            )
        )
        return commands
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

    nonisolated private static func fetch(
        _ url: URL,
        timeout: TimeInterval
    ) -> (data: Data?, ok: Bool) {
        var request = URLRequest(url: url)
        request.timeoutInterval = timeout
        request.cachePolicy = .reloadRevalidatingCacheData
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        let finished = DispatchSemaphore(value: 0)
        let box = NetworkBox()
        let task = URLSession.shared.dataTask(with: request) { data, response, error in
            box.data = data
            box.statusCode = (response as? HTTPURLResponse)?.statusCode
            box.failed = error != nil
            finished.signal()
        }
        task.resume()
        if finished.wait(timeout: .now() + timeout + 1) == .timedOut {
            task.cancel()
            return (nil, false)
        }
        return (box.data, !box.failed && box.statusCode == 200)
    }
}

/// Reference box so the background read closure can hand the captured bytes back
/// without a captured `var` data race.
private final class DataBox: @unchecked Sendable {
    var data = Data()
}

private final class NetworkBox: @unchecked Sendable {
    var data: Data?
    var statusCode: Int?
    var failed = false
}

private struct GraphCommand: Sendable {
    let executableURL: URL
    let arguments: [String]
}

private struct FullScanResult: Sendable {
    let message: String
    let succeeded: Bool

    static let success = FullScanResult(message: "Refresh finished.", succeeded: true)

    static func failure(_ message: String) -> FullScanResult {
        FullScanResult(message: message, succeeded: false)
    }
}
