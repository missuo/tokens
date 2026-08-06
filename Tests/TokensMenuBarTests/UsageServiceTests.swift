import XCTest
@testable import TokensMenuBarCore

final class UsageServiceTests: XCTestCase {
    func testPresetArgumentsExplicitlyRequestV3() {
        XCTAssertEqual(
            UsageService.arguments(for: .preset(.today)),
            ["usage", "--json", "--contract", "v3", "--period", "today"]
        )
        XCTAssertEqual(
            UsageService.arguments(for: .preset(.days7)),
            ["usage", "--json", "--contract", "v3", "--period", "7d"]
        )
        XCTAssertEqual(
            UsageService.arguments(for: .preset(.days30)),
            ["usage", "--json", "--contract", "v3", "--period", "30d"]
        )
        XCTAssertEqual(
            UsageService.arguments(for: .preset(.all)),
            ["usage", "--json", "--contract", "v3", "--period", "all"]
        )
    }

    func testCustomArgumentsUseInclusiveCivilDatesWithoutPeriod() {
        let selection = UsageSelection.custom(
            DateSelectionRange(startDate: "2026-03-08", endDate: "2026-03-08")
        )
        XCTAssertEqual(
            UsageService.arguments(for: selection),
            [
                "usage", "--json", "--contract", "v3",
                "--since", "2026-03-08", "--until", "2026-03-08",
            ]
        )
    }

    func testRefreshAndForceRescanArgumentsRemainDistinct() {
        XCTAssertEqual(
            UsageService.arguments(
                for: .preset(.today),
                refreshPolicy: .refresh
            ).suffix(1),
            ["--refresh"]
        )
        XCTAssertEqual(
            UsageService.arguments(
                for: .preset(.today),
                refreshPolicy: .forceRescan
            ).suffix(1),
            ["--force-rescan"]
        )
    }

    func testFetchRejectsMismatchedEchoedSelection() throws {
        let testsDirectory = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
        let fixture = testsDirectory
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("docs/wayfinder/time-range-cost-chart/prototypes/report-cache-contract/fixtures/report-v3-30d.json")
        let script = FileManager.default.temporaryDirectory
            .appendingPathComponent("tokens-selection-mismatch-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: script) }
        try """
        #!/bin/sh
        if [ "$1" = "usage" ] && [ "$2" = "--help" ]; then
          printf '%s\\n' '--contract --period --since --until'
          exit 0
        fi
        exec /bin/cat '\(fixture.path)'
        """.write(to: script, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o755],
            ofItemAtPath: script.path
        )

        XCTAssertThrowsError(
            try UsageService().fetch(
                selection: .preset(.today),
                binaryPath: script.path,
                timeoutSeconds: 5
            )
        ) { error in
            guard case UsageServiceError.invalidJSON(let detail) = error else {
                return XCTFail("unexpected error: \(error)")
            }
            XCTAssertTrue(detail.contains("selection"))
        }
    }

    /// Regression: waiting for process exit *before* draining stdout deadlocks once
    /// the OS pipe buffer fills (~64KB).
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
        guard let binary = UsageService.resolveBinaryPath(),
              UsageService.supportsMenuBarUsage(at: binary) else {
            throw XCTSkip("tokens binary with usage contract v3 not found")
        }
        let report = try UsageService().fetch(
            selection: .preset(.all),
            binaryPath: binary,
            // A first v3 request may rebuild the full-history facts snapshot.
            timeoutSeconds: 180
        )
        XCTAssertEqual(report.selection, .preset(.all))
        XCTAssertEqual(report.meta.reportContract, "v3")
    }
}
