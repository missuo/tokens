import Foundation
import Combine

@MainActor
public final class AppSettings: ObservableObject {
    private enum Keys {
        static let displayMode = "displayMode"
        static let scanInterval = "scanInterval"
        static let binaryOverride = "binaryOverride"
        static let lastPeriod = "lastPeriod"
    }

    @Published public var displayMode: MenuBarDisplayMode {
        didSet { UserDefaults.standard.set(displayMode.rawValue, forKey: Keys.displayMode) }
    }

    @Published public var scanInterval: ScanIntervalOption {
        didSet { UserDefaults.standard.set(scanInterval.rawValue, forKey: Keys.scanInterval) }
    }

    @Published public var binaryOverride: String {
        didSet { UserDefaults.standard.set(binaryOverride, forKey: Keys.binaryOverride) }
    }

    @Published public var lastPeriod: UsagePeriod {
        didSet { UserDefaults.standard.set(lastPeriod.rawValue, forKey: Keys.lastPeriod) }
    }

    public init() {
        let defaults = UserDefaults.standard
        displayMode = MenuBarDisplayMode(rawValue: defaults.string(forKey: Keys.displayMode) ?? "") ?? .tokens
        scanInterval = ScanIntervalOption(rawValue: defaults.string(forKey: Keys.scanInterval) ?? "") ?? .twelveHours
        binaryOverride = defaults.string(forKey: Keys.binaryOverride) ?? ""
        lastPeriod = UsagePeriod(rawValue: defaults.string(forKey: Keys.lastPeriod) ?? "") ?? .today
    }
}
