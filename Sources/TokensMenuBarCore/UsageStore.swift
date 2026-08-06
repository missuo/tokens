import Foundation
import Combine
import AppKit

@MainActor
public final class UsageStore: ObservableObject {
    public typealias BinaryResolver = @Sendable (String?) -> String?
    public typealias ReportFetcher = @Sendable (
        UsageSelection,
        UsageRefreshPolicy,
        String
    ) throws -> UsageReport

    @Published public private(set) var report: UsageReport?
    /// Trailing-30d report powering the Advanced page heatmap. Weekday × hour
    /// patterns are meaningless over the dashboard's default Today range, so
    /// the Advanced page always aggregates its own fixed 30-day window.
    @Published public private(set) var advancedReport: UsageReport?
    @Published public private(set) var isLoading = false
    @Published public private(set) var lastError: String?
    @Published public private(set) var binaryPath: String?
    @Published public private(set) var binaryMissing = false
    @Published public private(set) var selection: UsageSelection = .preset(.today)
    @Published public var showSettings = false

    public var isShowingStaleReport: Bool {
        guard let report else { return false }
        return report.selection != selection
    }

    public let settings: AppSettings
    private let binaryResolver: BinaryResolver
    private let reportFetcher: ReportFetcher
    private var timer: Timer?
    private var statusItem: NSStatusItem?
    /// Monotonic token so only the latest refresh may clear loading / write report.
    private var refreshGeneration = 0
    private var refreshTask: Task<Void, Never>?
    /// Monotonic token so only the latest Advanced fetch may write `advancedReport`.
    private var advancedGeneration = 0

    public init(
        settings: AppSettings? = nil,
        binaryResolver: @escaping BinaryResolver = { override in
            UsageService.resolveBinaryPath(override: override)
        },
        reportFetcher: @escaping ReportFetcher = { selection, refreshPolicy, binaryPath in
            try UsageService().fetch(
                selection: selection,
                refreshPolicy: refreshPolicy,
                binaryPath: binaryPath
            )
        }
    ) {
        self.settings = settings ?? AppSettings()
        self.binaryResolver = binaryResolver
        self.reportFetcher = reportFetcher
        // Deliberately do not restore the legacy persisted period. Every process
        // launch begins on Today; Custom and preset selection are session state.
        self.selection = .preset(.today)
        resolveBinary()
    }

    public func attachStatusItem(_ item: NSStatusItem) {
        statusItem = item
        updateStatusTitle()
    }

    public func bootstrap() {
        resolveBinary()
        startRefresh(policy: .refresh, showSpinner: true)
        restartTimer()
    }

    public func resolveBinary() {
        let path = binaryResolver(
            settings.binaryOverride.isEmpty ? nil : settings.binaryOverride
        )
        binaryPath = path
        binaryMissing = path == nil
        updateStatusTitle()
    }

    public func setPeriod(_ period: UsagePeriod) {
        setSelection(.preset(period))
    }

    public func setCustomRange(_ range: DateSelectionRange) {
        guard range.isOrdered else {
            lastError = "Custom range start must be on or before its end."
            updateStatusTitle()
            return
        }
        setSelection(.custom(range))
    }

    public func setSelection(_ newSelection: UsageSelection) {
        if newSelection == selection, report?.selection == newSelection {
            return
        }
        selection = newSelection
        // Range switches reuse the same-day facts snapshot. Keep the previous
        // report visible while the replacement is generated (stale-while-revalidate).
        startRefresh(policy: .snapshot, showSpinner: report == nil)
    }

    public func manualRefresh() {
        startRefresh(policy: .refresh, showSpinner: true)
    }

    /// Fixed trailing-30d time range the Advanced page heatmap always charts.
    public static let advancedSelection: UsageSelection = .preset(.days30)

    /// Load (or reload) the Advanced page's trailing-30d report. Reuses the
    /// same-day facts snapshot, so this is cheap; failures keep the previous
    /// report and the page falls back to the dashboard report.
    public func loadAdvancedReport() {
        advancedGeneration += 1
        let generation = advancedGeneration
        resolveBinary()
        guard let binaryPath = binaryPath, !binaryMissing else { return }
        let fetcher = reportFetcher
        let selection = UsageStore.advancedSelection
        Task.detached(priority: .userInitiated) { [weak self] in
            guard let report = try? fetcher(selection, .snapshot, binaryPath),
                  report.selection == selection
            else { return }
            await MainActor.run { [weak self] in
                guard let self, self.advancedGeneration == generation else { return }
                self.advancedReport = report
            }
        }
    }

    public func fullRescan() {
        startRefresh(policy: .forceRescan, showSpinner: true)
    }

    public func restartTimer() {
        timer?.invalidate()
        guard let interval = settings.scanInterval.timeInterval else {
            timer = nil
            return
        }
        timer = Timer.scheduledTimer(withTimeInterval: interval, repeats: true) { [weak self] _ in
            Task { @MainActor in
                self?.startRefresh(policy: .refresh, showSpinner: false)
            }
        }
    }

    public func updateStatusTitle() {
        let title = Formatting.menuBarTitle(
            report: report,
            mode: settings.displayMode,
            missingBinary: binaryMissing,
            hasError: lastError != nil
        )
        statusItem?.button?.title = " \(title)"
    }

    private func startRefresh(policy: UsageRefreshPolicy, showSpinner: Bool) {
        refreshTask?.cancel()
        refreshGeneration += 1
        let generation = refreshGeneration

        if showSpinner {
            isLoading = true
        }

        let requestedSelection = selection
        let fetcher = reportFetcher

        refreshTask = Task { [weak self] in
            guard let self else { return }
            defer {
                Task { @MainActor in
                    if self.refreshGeneration == generation {
                        self.isLoading = false
                    }
                }
            }

            self.resolveBinary()
            guard let binaryPath = self.binaryPath, !self.binaryMissing else {
                guard self.refreshGeneration == generation else { return }
                self.lastError = UsageServiceError.binaryNotFound.localizedDescription
                self.updateStatusTitle()
                return
            }

            if Task.isCancelled { return }

            do {
                let report = try await Task.detached(priority: .userInitiated) {
                    try fetcher(requestedSelection, policy, binaryPath)
                }.value
                guard report.selection == requestedSelection else {
                    throw UsageServiceError.invalidJSON(
                        "report selection did not match the requested selection"
                    )
                }

                if Task.isCancelled { return }
                guard self.refreshGeneration == generation else { return }
                self.report = report
                self.lastError = nil
                self.updateStatusTitle()
            } catch is CancellationError {
                return
            } catch {
                if Task.isCancelled { return }
                guard self.refreshGeneration == generation else { return }
                self.lastError =
                    (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
                self.updateStatusTitle()
            }
        }
    }

    public func openTokensSite() {
        if let url = URL(string: "https://tokens.ci") {
            NSWorkspace.shared.open(url)
        }
    }

    public func quit() {
        refreshTask?.cancel()
        NSApp.terminate(nil)
    }
}
