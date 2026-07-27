import Foundation
import Combine
import AppKit

@MainActor
public final class UsageStore: ObservableObject {
    @Published public private(set) var report: UsageReport?
    @Published public private(set) var isLoading = false
    @Published public private(set) var lastError: String?
    @Published public private(set) var binaryPath: String?
    @Published public private(set) var binaryMissing = false
    @Published public var period: UsagePeriod
    @Published public var showSettings = false

    public let settings: AppSettings
    private var timer: Timer?
    private var statusItem: NSStatusItem?
    /// Monotonic token so only the latest refresh may clear loading / write report.
    private var refreshGeneration = 0
    private var refreshTask: Task<Void, Never>?

    public init(settings: AppSettings? = nil) {
        let settings = settings ?? AppSettings()
        self.settings = settings
        self.period = settings.lastPeriod
        resolveBinary()
    }

    public func attachStatusItem(_ item: NSStatusItem) {
        statusItem = item
        updateStatusTitle()
    }

    public func bootstrap() {
        resolveBinary()
        // Warm snapshot path first if possible is still a real scan on first launch.
        startRefresh(forceRescan: false, useSnapshot: false, showSpinner: true)
        restartTimer()
    }

    public func resolveBinary() {
        let path = UsageService.resolveBinaryPath(
            override: settings.binaryOverride.isEmpty ? nil : settings.binaryOverride
        )
        binaryPath = path
        binaryMissing = path == nil
        updateStatusTitle()
    }

    public func setPeriod(_ newPeriod: UsagePeriod) {
        guard newPeriod != period || report?.period != newPeriod.cliValue else { return }
        period = newPeriod
        settings.lastPeriod = newPeriod
        // Period switches should hit Layer B snapshot (ms). Always allow replacing
        // an in-flight scan so the spinner cannot stick on a superseded request.
        startRefresh(forceRescan: false, useSnapshot: true, showSpinner: report == nil)
    }

    public func manualRefresh() {
        startRefresh(forceRescan: false, useSnapshot: false, showSpinner: true)
    }

    public func fullRescan() {
        startRefresh(forceRescan: true, useSnapshot: false, showSpinner: true)
    }

    public func restartTimer() {
        timer?.invalidate()
        guard let interval = settings.scanInterval.timeInterval else {
            timer = nil
            return
        }
        timer = Timer.scheduledTimer(withTimeInterval: interval, repeats: true) { [weak self] _ in
            Task { @MainActor in
                self?.startRefresh(forceRescan: false, useSnapshot: false, showSpinner: false)
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

    private func startRefresh(forceRescan: Bool, useSnapshot: Bool, showSpinner: Bool) {
        refreshTask?.cancel()
        refreshGeneration += 1
        let generation = refreshGeneration

        if showSpinner {
            isLoading = true
        }

        let period = self.period
        let refreshFlag = !useSnapshot && !forceRescan

        refreshTask = Task { [weak self] in
            guard let self else { return }
            defer {
                Task { @MainActor in
                    // Only the latest generation clears the spinner.
                    if self.refreshGeneration == generation {
                        self.isLoading = false
                    }
                }
            }

            await MainActor.run { self.resolveBinary() }

            let binaryPath = await MainActor.run { self.binaryPath }
            let missing = await MainActor.run { self.binaryMissing }

            guard let binaryPath, !missing else {
                await MainActor.run {
                    guard self.refreshGeneration == generation else { return }
                    self.lastError = UsageServiceError.binaryNotFound.localizedDescription
                    self.updateStatusTitle()
                }
                return
            }

            if Task.isCancelled { return }

            do {
                let report = try await Task.detached(priority: .userInitiated) {
                    try UsageService().fetch(
                        period: period,
                        refresh: refreshFlag,
                        forceRescan: forceRescan,
                        binaryPath: binaryPath
                    )
                }.value

                if Task.isCancelled { return }
                await MainActor.run {
                    guard self.refreshGeneration == generation else { return }
                    self.report = report
                    self.lastError = nil
                    self.updateStatusTitle()
                }
            } catch is CancellationError {
                return
            } catch {
                if Task.isCancelled { return }
                await MainActor.run {
                    guard self.refreshGeneration == generation else { return }
                    self.lastError =
                        (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
                    self.updateStatusTitle()
                }
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
