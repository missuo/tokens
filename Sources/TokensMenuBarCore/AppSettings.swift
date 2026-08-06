import Foundation
import Combine

@MainActor
public final class AppSettings: ObservableObject {
    private enum Keys {
        static let displayMode = "displayMode"
        static let scanInterval = "scanInterval"
        static let scanIntervalCustomMinutes = "scanIntervalCustomMinutes"
        static let binaryOverride = "binaryOverride"
    }

    private let defaults: UserDefaults

    @Published public var displayMode: MenuBarDisplayMode {
        didSet { defaults.set(displayMode.rawValue, forKey: Keys.displayMode) }
    }

    @Published public var scanInterval: ScanIntervalOption {
        didSet {
            defaults.set(scanInterval.storageKey, forKey: Keys.scanInterval)
            // Remember last custom minutes so re-selecting CUSTOM restores it.
            if case .custom(let minutes) = scanInterval {
                defaults.set(minutes, forKey: Keys.scanIntervalCustomMinutes)
            }
        }
    }

    @Published public var binaryOverride: String {
        didSet { defaults.set(binaryOverride, forKey: Keys.binaryOverride) }
    }

    /// Last Custom scan duration (minutes). Dashboard date selection is never persisted.
    public var lastCustomMinutes: Int {
        let stored = defaults.object(forKey: Keys.scanIntervalCustomMinutes) as? Int
        return ScanIntervalOption.clampMinutes(stored ?? 30)
    }

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        displayMode = MenuBarDisplayMode(
            rawValue: defaults.string(forKey: Keys.displayMode) ?? ""
        ) ?? .tokens
        scanInterval = ScanIntervalOption.fromStorage(defaults.string(forKey: Keys.scanInterval))
        binaryOverride = defaults.string(forKey: Keys.binaryOverride) ?? ""
    }
}
