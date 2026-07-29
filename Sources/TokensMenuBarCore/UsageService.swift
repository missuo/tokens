import Foundation

public struct UsageService {
    private static let probeLock = NSLock()
    private static var probeCache: [String: Bool] = [:]

    /// Resolve a tokens binary that supports `tokens usage --period`.
    ///
    /// Homebrew still ships an older `tokens usage` (provider quota TUI) that only
    /// accepts `--json`. Prefer user-local / monorepo builds that implement the
    /// Menu Bar report contract.
    public static func resolveBinaryPath(override: String? = nil) -> String? {
        if let override, !override.isEmpty, FileManager.default.isExecutableFile(atPath: override) {
            if supportsMenuBarUsage(at: override) || overrideOverrideBypass(override) {
                return override
            }
        }

        // Prefer an explicit env override from make/scripts (`TOKENS_CLI=...`).
        if let env = ProcessInfo.processInfo.environment["TOKENS_CLI"],
           !env.isEmpty,
           FileManager.default.isExecutableFile(atPath: env),
           supportsMenuBarUsage(at: env) {
            return (env as NSString).standardizingPath
        }

        var candidates: [String] = []
        candidates.append(contentsOf: repoLocalCLICandidates())
        candidates.append(contentsOf: [
            NSHomeDirectory() + "/.local/bin/tokens",
            // Common monorepo release layout when developing from this checkout.
            NSHomeDirectory() + "/Documents/Codebase/tokens/cli/target/release/tokens",
            "/opt/homebrew/bin/tokens",
            "/usr/local/bin/tokens",
        ])

        // PATH lookup via a login-like path order (user local first).
        if let pathEnv = ProcessInfo.processInfo.environment["PATH"] {
            for dir in pathEnv.split(separator: ":") {
                candidates.append("\(dir)/tokens")
            }
        }
        candidates.append(contentsOf: whichAllTokens())

        var seen = Set<String>()
        for path in candidates {
            let resolved = (path as NSString).standardizingPath
            guard seen.insert(resolved).inserted else { continue }
            guard FileManager.default.isExecutableFile(atPath: resolved) else { continue }
            if supportsMenuBarUsage(at: resolved) {
                return resolved
            }
        }

        // Last resort: any tokens binary (will fail with a clearer CLI error).
        for path in candidates {
            let resolved = (path as NSString).standardizingPath
            if FileManager.default.isExecutableFile(atPath: resolved) {
                return resolved
            }
        }
        return nil
    }

    /// Explicit override always wins even if probe fails (advanced users).
    private static func overrideOverrideBypass(_ path: String) -> Bool {
        // If the user set an override we still prefer it only when it works;
        // this helper is reserved if we later want force-use. Keep false.
        _ = path
        return false
    }

    /// Discover `cli/target/release/tokens` relative to the running app / cwd.
    private static func repoLocalCLICandidates() -> [String] {
        var roots: [String] = []
        // Current working directory (swift run / make from repo root).
        roots.append(FileManager.default.currentDirectoryPath)
        // Walk up from the executable path (`.build/debug/TokensMenuBar`).
        if let exe = Bundle.main.executablePath {
            roots.append((exe as NSString).deletingLastPathComponent)
        }
        var out: [String] = []
        var seen = Set<String>()
        for root in roots {
            var dir = URL(fileURLWithPath: root, isDirectory: true)
            for _ in 0..<8 {
                let candidate = dir.appendingPathComponent("cli/target/release/tokens").path
                let normalized = (candidate as NSString).standardizingPath
                if seen.insert(normalized).inserted {
                    out.append(normalized)
                }
                let parent = dir.deletingLastPathComponent()
                if parent.path == dir.path { break }
                dir = parent
            }
        }
        return out
    }

    /// True when `tokens usage --help` advertises `--period` (Menu Bar schema).
    public static func supportsMenuBarUsage(at path: String) -> Bool {
        probeLock.lock()
        if let cached = probeCache[path] {
            probeLock.unlock()
            return cached
        }
        probeLock.unlock()

        let process = Process()
        process.executableURL = URL(fileURLWithPath: path)
        process.arguments = ["usage", "--help"]
        let stdout = Pipe()
        let stderr = Pipe()
        process.standardOutput = stdout
        process.standardError = stderr
        do {
            try process.run()
            process.waitUntilExit()
        } catch {
            return false
        }
        let out = String(data: stdout.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        let err = String(data: stderr.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        let help = out + err
        let ok = help.contains("--period")
        probeLock.lock()
        probeCache[path] = ok
        probeLock.unlock()
        return ok
    }

    private static func whichAllTokens() -> [String] {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/which")
        process.arguments = ["-a", "tokens"]
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = Pipe()
        // Give which a PATH that includes common install locations.
        var env = ProcessInfo.processInfo.environment
        let extras = [
            NSHomeDirectory() + "/.local/bin",
            "/opt/homebrew/bin",
            "/usr/local/bin",
        ]
        env["PATH"] = (extras + [env["PATH"] ?? ""]).joined(separator: ":")
        process.environment = env
        do {
            try process.run()
            process.waitUntilExit()
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            let text = String(data: data, encoding: .utf8) ?? ""
            return text
                .split(whereSeparator: \.isNewline)
                .map { String($0).trimmingCharacters(in: .whitespacesAndNewlines) }
                .filter { !$0.isEmpty }
        } catch {
            return []
        }
    }

    public func fetch(
        period: UsagePeriod,
        refresh: Bool = false,
        forceRescan: Bool = false,
        binaryPath: String? = nil,
        timeoutSeconds: TimeInterval = 180
    ) throws -> UsageReport {
        guard let binary = binaryPath ?? Self.resolveBinaryPath() else {
            throw UsageServiceError.binaryNotFound
        }

        if !Self.supportsMenuBarUsage(at: binary) {
            throw UsageServiceError.commandFailed(
                code: 2,
                message: """
                Found tokens at \(binary), but it is too old for the Menu Bar report \
                (missing `usage --period`). Build this repo's CLI + app together, e.g.:
                  make build
                  make restart
                """
            )
        }

        var args = ["usage", "--json", "--period", period.cliValue]
        if forceRescan {
            args.append("--force-rescan")
        } else if refresh {
            args.append("--refresh")
        }

        // Inherit a sane PATH for nested tools some scanners may call.
        var env = ProcessInfo.processInfo.environment
        let extras = [
            NSHomeDirectory() + "/.local/bin",
            "/opt/homebrew/bin",
            "/usr/local/bin",
        ]
        let path = env["PATH"] ?? ""
        env["PATH"] = (extras + [path]).joined(separator: ":")

        let result = try Self.runProcess(
            executable: binary,
            arguments: args,
            environment: env,
            timeoutSeconds: timeoutSeconds
        )

        let outData = result.stdout
        let errData = result.stderr
        let outText = String(data: outData, encoding: .utf8) ?? ""
        let errText = String(data: errData, encoding: .utf8) ?? ""

        let decoder = JSONDecoder()
        if result.status == 0 {
            do {
                return try decoder.decode(UsageReport.self, from: outData)
            } catch {
                throw UsageServiceError.invalidJSON(error.localizedDescription)
            }
        }

        if let errReport = try? decoder.decode(UsageErrorReport.self, from: outData) {
            throw UsageServiceError.commandFailed(
                code: result.status,
                message: errReport.error.message
            )
        }

        // Surface clap-style stderr ("unexpected argument '--period'") clearly.
        let message = [errText, outText]
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .first { !$0.isEmpty } ?? "tokens usage failed"
        throw UsageServiceError.commandFailed(
            code: result.status,
            message: message
        )
    }

    /// Run a process while **concurrently** draining stdout/stderr.
    ///
    /// Waiting for exit before reading pipes deadlocks once the OS pipe buffer
    /// (~64KB) fills — `tokens usage --json` pretty output already exceeds that
    /// for period `all`, which surfaced as `UsageServiceError.timeout`.
    static func runProcess(
        executable: String,
        arguments: [String],
        environment: [String: String]? = nil,
        timeoutSeconds: TimeInterval
    ) throws -> (status: Int32, stdout: Data, stderr: Data) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = arguments
        if let environment {
            process.environment = environment
        }

        let stdout = Pipe()
        let stderr = Pipe()
        process.standardOutput = stdout
        process.standardError = stderr

        try process.run()

        let box = ProcessOutputBox()
        let group = DispatchGroup()

        group.enter()
        DispatchQueue.global(qos: .userInitiated).async {
            box.stdout = stdout.fileHandleForReading.readDataToEndOfFile()
            group.leave()
        }
        group.enter()
        DispatchQueue.global(qos: .userInitiated).async {
            box.stderr = stderr.fileHandleForReading.readDataToEndOfFile()
            group.leave()
        }
        group.enter()
        DispatchQueue.global(qos: .userInitiated).async {
            process.waitUntilExit()
            group.leave()
        }

        let waitResult = group.wait(timeout: .now() + timeoutSeconds)
        if waitResult == .timedOut {
            process.terminate()
            // Give readers a moment to unblock after terminate closes pipes.
            _ = group.wait(timeout: .now() + 1)
            throw UsageServiceError.timeout
        }

        return (process.terminationStatus, box.stdout, box.stderr)
    }
}

/// Thread-safe bag for concurrent pipe drains.
private final class ProcessOutputBox: @unchecked Sendable {
    private let lock = NSLock()
    private var _stdout = Data()
    private var _stderr = Data()

    var stdout: Data {
        get { lock.lock(); defer { lock.unlock() }; return _stdout }
        set { lock.lock(); _stdout = newValue; lock.unlock() }
    }

    var stderr: Data {
        get { lock.lock(); defer { lock.unlock() }; return _stderr }
        set { lock.lock(); _stderr = newValue; lock.unlock() }
    }
}
