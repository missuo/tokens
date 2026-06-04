import XCTest
@testable import TokscaleMenuBarCore

final class TokscaleSummaryTests: XCTestCase {
    func testDecodesCompanionSummaryAndKeepsCollapsedTitleShort() throws {
        let summary = try TokscaleSummary.decode(sampleSummaryData())

        XCTAssertEqual(summary.statusTitle, "$399")
        XCTAssertEqual(summary.menuBarTitle, "AI $399")
        XCTAssertEqual(summary.collapsed.state, "normal")
        XCTAssertFalse(summary.stale)
    }

    func testBuildsCompactMenuLinesForPopover() throws {
        let summary = try TokscaleSummary.decode(sampleSummaryData())

        XCTAssertEqual(
            summary.menuLines,
            [
                "Today: $398.56 - 522.6M tokens - 2501 messages",
                "Total: $24.0K - 35.4B tokens - 120 active days",
                "Top: codex - gpt-5.5",
                "Accuracy: medium - local-scan",
                "Last scan: 5m 1s"
            ]
        )
    }

    func testDefaultSummaryURLUsesTokensCache() {
        let home = URL(fileURLWithPath: "/Users/example", isDirectory: true)

        let url = TokscaleSummary.defaultSummaryURL(homeDirectory: home)

        XCTAssertEqual(
            url.path,
            "/Users/example/.config/tokens/cache/companion-summary.json"
        )
    }

    func testMissingOptionalTopFieldsStillRenders() throws {
        let data = sampleSummaryJSON(
            topJSON: "",
            accuracyJSON: #""confidence":"low","sourceKinds":[],"warnings":["unpriced"]"#
        ).data(using: .utf8)!

        let summary = try TokscaleSummary.decode(data)

        XCTAssertEqual(summary.menuLines[2], "Top: none")
        XCTAssertEqual(summary.menuLines[3], "Accuracy: low - warning")
    }

    func testDecodesLegacySummaryWithoutProviderBreakdown() throws {
        let data = """
        {
          "version": 1,
          "generatedAt": "2026-06-04T02:25:56.459117+00:00",
          "stale": false,
          "collapsed": {"metric": "todayCost", "label": "$399", "state": "normal"},
          "today": {"date": "2026-06-04", "costUsd": 398.56, "tokens": 522596373, "messages": 2501},
          "totals": {
            "costUsd": 24045.19,
            "tokens": 35380336692,
            "activeDays": 120,
            "clients": ["claude", "codex"],
            "models": 24
          },
          "top": {"client": "codex", "model": "gpt-5.5"},
          "health": {"summaryPath": "/tmp/summary.json", "lastScanDurationMs": 300943, "warnings": []},
          "accuracy": {"confidence": "medium", "sourceKinds": ["local-scan"], "warnings": []}
        }
        """.data(using: .utf8)!

        let summary = try TokscaleSummary.decode(data)
        let dashboard = TokscaleDashboardModel(summary: summary)

        XCTAssertTrue(summary.providers.isEmpty)
        XCTAssertEqual(dashboard.providers.map(\.label), ["Claude", "Codex"])
    }

    func testStoreReturnsNilWhenSummaryFileIsMissing() throws {
        let directory = URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true)
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let store = TokscaleSummaryStore(summaryURL: directory.appendingPathComponent("missing.json"))

        let summary = try store.load()

        XCTAssertNil(summary)
    }

    func testStoreLoadsSummaryFile() throws {
        let directory = URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true)
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let summaryURL = directory.appendingPathComponent("companion-summary.json")
        try sampleSummaryData().write(to: summaryURL)
        let store = TokscaleSummaryStore(summaryURL: summaryURL)

        let summary = try store.load()

        XCTAssertEqual(summary?.statusTitle, "$399")
    }

    func testDashboardModelBuildsMultiClientDashboardSections() throws {
        let summary = try TokscaleSummary.decode(sampleSummaryData())

        let dashboard = TokscaleDashboardModel(summary: summary)

        XCTAssertEqual(dashboard.hero.title, "$399")
        XCTAssertEqual(dashboard.hero.subtitle, "4 AI clients - local cache")
        XCTAssertEqual(dashboard.hero.progressLabel, "199% of daily average")
        XCTAssertEqual(dashboard.hero.progress, 0.99, accuracy: 0.01)
        XCTAssertEqual(dashboard.clientLabels, ["Codex", "Claude", "Gemini", "OpenClaw"])
        XCTAssertEqual(dashboard.metrics[0], .init(title: "Today", value: "$398.56", detail: "522.6M tokens - 2501 messages"))
        XCTAssertEqual(dashboard.metrics[1], .init(title: "Total", value: "$24.0K", detail: "35.4B tokens - 120 active days"))
        XCTAssertEqual(dashboard.insights[0], .init(title: "Top driver", value: "codex", detail: "gpt-5.5"))
        XCTAssertEqual(dashboard.insights[1], .init(title: "Accuracy", value: "medium", detail: "local-scan"))
        XCTAssertEqual(dashboard.health.title, "Fresh")
        XCTAssertEqual(dashboard.health.detail, "Last scan 5m 1s")
    }

    func testDashboardModelBuildsSelectableProviderBreakdown() throws {
        let summary = try TokscaleSummary.decode(sampleSummaryData())

        let dashboard = TokscaleDashboardModel(summary: summary)

        XCTAssertEqual(dashboard.providers.count, 4)
        XCTAssertEqual(dashboard.providers[0].id, "codex")
        XCTAssertEqual(dashboard.providers[0].label, "Codex")
        XCTAssertEqual(dashboard.providers[0].value, "$12.0K")
        XCTAssertEqual(dashboard.providers[0].detail, "16B tokens - 50%")
        XCTAssertEqual(dashboard.providers[0].share, 0.50, accuracy: 0.01)
        XCTAssertEqual(dashboard.providerDetails(for: "codex").title, "Codex")
        XCTAssertEqual(dashboard.providerDetails(for: "codex").model, "gpt-5.5")
        XCTAssertEqual(dashboard.providerDetails(for: "claude").title, "Claude")
    }

    private func sampleSummaryData() -> Data {
        sampleSummaryJSON(
            topJSON: #""client":"codex","model":"gpt-5.5""#,
            accuracyJSON: #""confidence":"medium","sourceKinds":["local-scan"],"warnings":[]"#
        ).data(using: .utf8)!
    }

    private func sampleSummaryJSON(topJSON: String, accuracyJSON: String) -> String {
        """
        {
          "version": 1,
          "generatedAt": "2026-06-04T02:25:56.459117+00:00",
          "stale": false,
          "collapsed": {
            "metric": "todayCost",
            "label": "$399",
            "state": "normal"
          },
          "today": {
            "date": "2026-06-04",
            "costUsd": 398.56475810000006,
            "tokens": 522596373,
            "messages": 2501
          },
          "totals": {
            "costUsd": 24045.195710949993,
            "tokens": 35380336692,
            "activeDays": 120,
            "clients": ["claude", "codex", "gemini", "openclaw"],
            "models": 24
          },
          "providers": [
            {
              "client": "codex",
              "costUsd": 12000.0,
              "tokens": 16000000000,
              "messages": 52000,
              "todayCostUsd": 350.0,
              "todayTokens": 500000000,
              "todayMessages": 2400,
              "topModel": "gpt-5.5"
            },
            {
              "client": "claude",
              "costUsd": 6000.0,
              "tokens": 9000000000,
              "messages": 28000,
              "todayCostUsd": 30.0,
              "todayTokens": 20000000,
              "todayMessages": 90,
              "topModel": "claude-sonnet"
            },
            {
              "client": "gemini",
              "costUsd": 4000.0,
              "tokens": 7000000000,
              "messages": 16000,
              "todayCostUsd": 12.0,
              "todayTokens": 2000000,
              "todayMessages": 8,
              "topModel": "gemini-pro"
            },
            {
              "client": "openclaw",
              "costUsd": 2045.195710949993,
              "tokens": 3380336692,
              "messages": 9000,
              "todayCostUsd": 6.56475810000006,
              "todayTokens": 596373,
              "todayMessages": 3,
              "topModel": "openclaw"
            }
          ],
          "top": {
            \(topJSON)
          },
          "health": {
            "summaryPath": "/Users/example/.config/tokens/cache/companion-summary.json",
            "lastScanDurationMs": 300943,
            "warnings": []
          },
          "accuracy": {
            \(accuracyJSON)
          }
        }
        """
    }
}
