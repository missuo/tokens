import XCTest
@testable import TokensMenuBarCore

final class UsageServiceTests: XCTestCase {
    /// Regression: waiting for process exit *before* draining stdout deadlocks once
    /// the OS pipe buffer fills (~64KB). `tokens usage --json --period all` is
    /// already past that size with pretty JSON.
    func testRunProcessDrainsLargeStdoutWithoutDeadlock() throws {
        let payloadSize = 200_000
        let script = "import sys; sys.stdout.buffer.write(b'x' * \(payloadSize)); sys.stdout.buffer.flush()"
        let result = try UsageService.runProcess(
            executable: "/usr/bin/python3",
            arguments: ["-c", script],
            timeoutSeconds: 5
        )
        XCTAssertEqual(result.status, 0)
        XCTAssertEqual(result.stdout.count, payloadSize)
        XCTAssertTrue(result.stderr.isEmpty)
    }

    func testRunProcessCapturesStderrAndNonZeroStatus() throws {
        let script = "import sys; sys.stderr.write('boom'); sys.exit(7)"
        let result = try UsageService.runProcess(
            executable: "/usr/bin/python3",
            arguments: ["-c", script],
            timeoutSeconds: 5
        )
        XCTAssertEqual(result.status, 7)
        XCTAssertEqual(String(data: result.stderr, encoding: .utf8), "boom")
    }

    func testFetchAllPeriodDoesNotTimeout() throws {
        guard let binary = UsageService.resolveBinaryPath() else {
            throw XCTSkip("tokens binary with usage --period not found")
        }
        // Snapshot path should be ms; even refresh is ~1s. Old deadlock hit 180s.
        let report = try UsageService().fetch(
            period: .all,
            binaryPath: binary,
            timeoutSeconds: 15
        )
        XCTAssertEqual(report.period, "all")
        XCTAssertFalse(report.byDay.isEmpty)
    }
}
