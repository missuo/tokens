import Foundation

public struct UsageService {
    /// Resolve the tokens binary path.
    public static func resolveBinaryPath(override: String? = nil) -> String? {
        if let override, !override.isEmpty, FileManager.default.isExecutableFile(atPath: override) {
            return override
        }

        let candidates = [
            "/opt/homebrew/bin/tokens",
            "/usr/local/bin/tokens",
            NSHomeDirectory() + "/.local/bin/tokens",
        ]
        for path in candidates where FileManager.default.isExecutableFile(atPath: path) {
            return path
        }

        // PATH lookup
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/which")
        process.arguments = ["tokens"]
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = Pipe()
        do {
            try process.run()
            process.waitUntilExit()
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            if let path = String(data: data, encoding: .utf8)?
                .trimmingCharacters(in: .whitespacesAndNewlines),
               !path.isEmpty,
               FileManager.default.isExecutableFile(atPath: path)
            {
                return path
            }
        } catch {
            return nil
        }
        return nil
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
        let extras = ["/opt/homebrew/bin", "/usr/local/bin", NSHomeDirectory() + "/.local/bin"]
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

        let message = [outText, errText]
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .first { !$0.isEmpty } ?? "tokens usage failed"
        throw UsageServiceError.commandFailed(
            code: process.terminationStatus,
            message: message
        )
    }
}
