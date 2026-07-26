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
    private var inFlight = false
    private var statusItem: NSStatusItem?

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
        Task { await refresh(forceRescan: false, useSnapshot: false) }
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
        period = newPeriod
        settings.lastPeriod = newPeriod
        Task { await refresh(forceRescan: false, useSnapshot: true) }
    }

    public func manualRefresh() {
        Task { await refresh(forceRescan: false, useSnapshot: false) }
    }

    public func fullRescan() {
        Task { await refresh(forceRescan: true, useSnapshot: false) }
    }

    public func restartTimer() {
        timer?.invalidate()
        guard let interval = settings.scanInterval.timeInterval else {
            timer = nil
            return
        }
        timer = Timer.scheduledTimer(withTimeInterval: interval, repeats: true) { [weak self] _ in
            Task { @MainActor in
                await self?.refresh(forceRescan: false, useSnapshot: false)
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

    private func refresh(forceRescan: Bool, useSnapshot: Bool) async {
        if inFlight { return }
        inFlight = true
        isLoading = true
        defer {
            isLoading = false
            inFlight = false
        }

        resolveBinary()
        guard let binaryPath, !binaryMissing else {
            lastError = UsageServiceError.binaryNotFound.localizedDescription
            updateStatusTitle()
            return
        }

        let period = self.period
        let refreshFlag = !useSnapshot && !forceRescan
        do {
            let report = try await Task.detached(priority: .userInitiated) {
                try UsageService().fetch(
                    period: period,
                    refresh: refreshFlag,
                    forceRescan: forceRescan,
                    binaryPath: binaryPath
                )
            }.value
            self.report = report
            self.lastError = nil
            updateStatusTitle()
        } catch {
            self.lastError = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
            updateStatusTitle()
        }
    }

    public func openTokensSite() {
        if let url = URL(string: "https://tokens.ci") {
            NSWorkspace.shared.open(url)
        }
    }

    public func quit() {
        NSApp.terminate(nil)
    }
}
