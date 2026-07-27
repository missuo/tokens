import Foundation

public struct UsageService {
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

        var candidates: [String] = [
            NSHomeDirectory() + "/.local/bin/tokens",
            // Common monorepo release layout when developing from this checkout.
            NSHomeDirectory() + "/Documents/Codebase/tokens/cli/target/release/tokens",
            "/opt/homebrew/bin/tokens",
            "/usr/local/bin/tokens",
        ]

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

    /// True when `tokens usage --help` advertises `--period` (Menu Bar schema).
    public static func supportsMenuBarUsage(at path: String) -> Bool {
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
        return help.contains("--period")
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
                (missing `usage --period`). Build this repo's CLI and put it first on PATH, e.g.:
                  cargo build --release --manifest-path cli/Cargo.toml -p tokens-cli
                  ln -sfn "$(pwd)/cli/target/release/tokens" ~/.local/bin/tokens
                """
            )
        }

        var args = ["usage", "--json", "--period", period.cliValue]
        if forceRescan {
            args.append("--force-rescan")
        } else if refresh {
            args.append("--refresh")
        }

        let process = Process()
        process.executableURL = URL(fileURLWithPath: binary)
        process.arguments = args
        // Inherit a sane PATH for nested tools some scanners may call.
        var env = ProcessInfo.processInfo.environment
        let extras = [
            NSHomeDirectory() + "/.local/bin",
            "/opt/homebrew/bin",
            "/usr/local/bin",
        ]
        let path = env["PATH"] ?? ""
        env["PATH"] = (extras + [path]).joined(separator: ":")
        process.environment = env

        let stdout = Pipe()
        let stderr = Pipe()
        process.standardOutput = stdout
        process.standardError = stderr

        try process.run()

        let group = DispatchGroup()
        group.enter()
        DispatchQueue.global(qos: .userInitiated).async {
            process.waitUntilExit()
            group.leave()
        }
        let waitResult = group.wait(timeout: .now() + timeoutSeconds)
        if waitResult == .timedOut {
            process.terminate()
            throw UsageServiceError.timeout
        }

        let outData = stdout.fileHandleForReading.readDataToEndOfFile()
        let errData = stderr.fileHandleForReading.readDataToEndOfFile()
        let outText = String(data: outData, encoding: .utf8) ?? ""
        let errText = String(data: errData, encoding: .utf8) ?? ""

        let decoder = JSONDecoder()
        if process.terminationStatus == 0 {
            do {
                return try decoder.decode(UsageReport.self, from: outData)
            } catch {
                throw UsageServiceError.invalidJSON(error.localizedDescription)
            }
        }

        if let errReport = try? decoder.decode(UsageErrorReport.self, from: outData) {
            throw UsageServiceError.commandFailed(
                code: process.terminationStatus,
                message: errReport.error.message
            )
        }

        // Surface clap-style stderr ("unexpected argument '--period'") clearly.
        let message = [errText, outText]
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .first { !$0.isEmpty } ?? "tokens usage failed"
        throw UsageServiceError.commandFailed(
            code: process.terminationStatus,
            message: message
        )
    }
}
