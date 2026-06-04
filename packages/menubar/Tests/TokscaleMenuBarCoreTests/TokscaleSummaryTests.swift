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

        let summary = try store.load(now: try isoDate("2026-06-04T03:00:00Z"))

        XCTAssertEqual(summary?.statusTitle, "$399")
    }

    func testStoreMarksOldSummaryStaleAtLoadTime() throws {
        let directory = URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true)
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let summaryURL = directory.appendingPathComponent("companion-summary.json")
        try sampleSummaryData().write(to: summaryURL)
        let store = TokscaleSummaryStore(summaryURL: summaryURL)

        let summary = try XCTUnwrap(store.load(now: try isoDate("2026-06-04T04:26:00Z")))

        XCTAssertTrue(summary.stale)
        XCTAssertEqual(summary.staleReason, "summary-older-than-2h")
        XCTAssertEqual(summary.collapsed.state, "stale")
        XCTAssertEqual(summary.statusTitle, "$399!")
    }

    func testStoreMarksPreviousLocalDaySummaryStaleAtLoadTime() throws {
        let directory = URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true)
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let summaryURL = directory.appendingPathComponent("companion-summary.json")
        let data = sampleSummaryJSON(
            generatedAt: "2026-06-04T15:30:00Z",
            topJSON: #""client":"codex","model":"gpt-5.5""#,
            accuracyJSON: #""confidence":"medium","sourceKinds":["local-scan"],"warnings":[]"#
        ).data(using: .utf8)!
        try data.write(to: summaryURL)
        let store = TokscaleSummaryStore(summaryURL: summaryURL)
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(secondsFromGMT: 9 * 60 * 60)!

        let summary = try XCTUnwrap(
            store.load(
                now: try isoDate("2026-06-04T15:31:00Z"),
                calendar: calendar
            )
        )

        XCTAssertTrue(summary.stale)
        XCTAssertEqual(summary.staleReason, "summary-date-mismatch")
        XCTAssertEqual(summary.collapsed.state, "stale")
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

    func testDashboardModelBuildsSelectedProviderFocus() throws {
        let summary = try TokscaleSummary.decode(sampleSummaryData())

        let dashboard = TokscaleDashboardModel(summary: summary)

        let claude = dashboard.providerFocus(for: "claude")
        XCTAssertEqual(claude.id, "claude")
        XCTAssertEqual(claude.title, "Claude")
        XCTAssertEqual(claude.topModel, "claude-sonnet")
        XCTAssertEqual(claude.today, "$30.00 today")
        XCTAssertEqual(claude.quotaWindows.map(\.title), ["Session", "Weekly"])
        XCTAssertEqual(claude.primaryQuota?.title, "Session")
        XCTAssertEqual(claude.weeklyQuota?.title, "Weekly")
        XCTAssertEqual(claude.quotaStatus, "Quota fresh")
        XCTAssertEqual(claude.focusedModelTime, "Sonnet-only unavailable")

        let gemini = dashboard.providerFocus(for: "gemini")
        XCTAssertEqual(gemini.id, "gemini")
        XCTAssertEqual(gemini.title, "Gemini")
        XCTAssertNil(gemini.primaryQuota)
        XCTAssertTrue(gemini.quotaWindows.isEmpty)
        XCTAssertEqual(gemini.quotaStatus, "No official quota")
    }

    func testDecodesQuotaAndHistoryModules() throws {
        let summary = try TokscaleSummary.decode(sampleSummaryData())

        XCTAssertEqual(summary.quota.count, 1)
        XCTAssertEqual(summary.quota[0].provider, "Claude")
        XCTAssertEqual(summary.quota[0].plan, "Pro 5x")
        XCTAssertEqual(summary.quota[0].windows.count, 2)
        XCTAssertEqual(summary.quota[0].windows[0].label, "Session")
        XCTAssertEqual(summary.quota[0].windows[0].usedPercent, 72.0)
        XCTAssertEqual(summary.quota[0].windows[0].resetsAt, "2026-06-04T10:00:00Z")
        XCTAssertEqual(summary.history.count, 7)
        XCTAssertEqual(summary.history[0].date, "2026-05-29")
        XCTAssertEqual(summary.history[6].date, "2026-06-04")
        XCTAssertEqual(summary.history[6].costUsd, 398.56475810000006)
    }

    func testDashboardModelBuildsQuotaAndHistorySections() throws {
        let summary = try TokscaleSummary.decode(sampleSummaryData())

        let dashboard = TokscaleDashboardModel(summary: summary)

        XCTAssertEqual(dashboard.quotaWindows.count, 2)
        XCTAssertEqual(dashboard.quotaWindows[0].provider, "Claude")
        XCTAssertEqual(dashboard.quotaWindows[0].title, "Session")
        XCTAssertEqual(dashboard.quotaWindows[0].value, "72% used")
        XCTAssertEqual(dashboard.quotaWindows[0].detail, "28% left")
        XCTAssertEqual(dashboard.quotaWindows[0].progress, 0.72, accuracy: 0.01)
        XCTAssertEqual(dashboard.quotaWindows[1].title, "Weekly")
        XCTAssertEqual(dashboard.historyTrend.count, 7)
        XCTAssertEqual(dashboard.historyTrend[0].value, "$10.00")
        XCTAssertEqual(dashboard.historyTrend[6].value, "$398.56")
        XCTAssertEqual(dashboard.historyPeak?.date, "2026-06-04")
    }

    private func sampleSummaryData() -> Data {
        sampleSummaryJSON(
            topJSON: #""client":"codex","model":"gpt-5.5""#,
            accuracyJSON: #""confidence":"medium","sourceKinds":["local-scan"],"warnings":[]"#
        ).data(using: .utf8)!
    }

    private func sampleSummaryJSON(
        generatedAt: String = "2026-06-04T02:25:56.459117+00:00",
        topJSON: String,
        accuracyJSON: String
    ) -> String {
        """
        {
          "version": 1,
          "generatedAt": "\(generatedAt)",
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
          "quota": [
            {
              "provider": "Claude",
              "plan": "Pro 5x",
              "windows": [
                {
                  "label": "Session",
                  "usedPercent": 72.0,
                  "remainingPercent": 28.0,
                  "resetsAt": "2026-06-04T10:00:00Z"
                },
                {
                  "label": "Weekly",
                  "usedPercent": 41.0,
                  "remainingPercent": 59.0,
                  "resetsAt": "2026-06-08T00:00:00Z"
                }
              ]
            }
          ],
          "history": [
            {"date": "2026-05-29", "costUsd": 10.0, "tokens": 1000000, "messages": 10},
            {"date": "2026-05-30", "costUsd": 20.0, "tokens": 2000000, "messages": 20},
            {"date": "2026-05-31", "costUsd": 30.0, "tokens": 3000000, "messages": 30},
            {"date": "2026-06-01", "costUsd": 40.0, "tokens": 4000000, "messages": 40},
            {"date": "2026-06-02", "costUsd": 50.0, "tokens": 5000000, "messages": 50},
            {"date": "2026-06-03", "costUsd": 60.0, "tokens": 6000000, "messages": 60},
            {"date": "2026-06-04", "costUsd": 398.56475810000006, "tokens": 522596373, "messages": 2501}
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

    private func isoDate(_ value: String) throws -> Date {
        try XCTUnwrap(ISO8601DateFormatter().date(from: value))
    }
}
