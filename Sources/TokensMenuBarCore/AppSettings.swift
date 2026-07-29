import Foundation
import Combine

@MainActor
public final class AppSettings: ObservableObject {
    private enum Keys {
        static let displayMode = "displayMode"
        static let scanInterval = "scanInterval"
        static let scanIntervalCustomMinutes = "scanIntervalCustomMinutes"
        static let binaryOverride = "binaryOverride"
        static let lastPeriod = "lastPeriod"
    }

    @Published public var displayMode: MenuBarDisplayMode {
        didSet { UserDefaults.standard.set(displayMode.rawValue, forKey: Keys.displayMode) }
    }

    @Published public var scanInterval: ScanIntervalOption {
        didSet {
            UserDefaults.standard.set(scanInterval.storageKey, forKey: Keys.scanInterval)
            // Remember last custom minutes so re-selecting CUSTOM restores it.
            if case .custom(let minutes) = scanInterval {
                UserDefaults.standard.set(minutes, forKey: Keys.scanIntervalCustomMinutes)
            }
        }
    }

    @Published public var binaryOverride: String {
        didSet { UserDefaults.standard.set(binaryOverride, forKey: Keys.binaryOverride) }
    }

    @Published public var lastPeriod: UsagePeriod {
        didSet { UserDefaults.standard.set(lastPeriod.rawValue, forKey: Keys.lastPeriod) }
    }

    /// Last Custom duration (minutes). Used when switching chip → CUSTOM.
    public var lastCustomMinutes: Int {
        let stored = UserDefaults.standard.object(forKey: Keys.scanIntervalCustomMinutes) as? Int
        return ScanIntervalOption.clampMinutes(stored ?? 30)
    }

    public init() {
        let defaults = UserDefaults.standard
        displayMode = MenuBarDisplayMode(rawValue: defaults.string(forKey: Keys.displayMode) ?? "") ?? .tokens
        scanInterval = ScanIntervalOption.fromStorage(defaults.string(forKey: Keys.scanInterval))
        binaryOverride = defaults.string(forKey: Keys.binaryOverride) ?? ""
        lastPeriod = UsagePeriod(rawValue: defaults.string(forKey: Keys.lastPeriod) ?? "") ?? .today
    }
}
