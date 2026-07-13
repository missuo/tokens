import XCTest
@testable import TokscaleMenuBarCore

final class TokscaleSummaryTests: XCTestCase {
    func testDecodesCompanionSummaryAndKeepsCollapsedTitleShort() throws {
        let summary = try TokscaleSummary.decode(sampleSummaryData())

        XCTAssertEqual(summary.statusTitle, "$399")
        XCTAssertEqual(summary.menuBarTitle, "Tokens")
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

    func testFreshTodayDoesNotMaskStaleHistory() throws {
        let data = sampleSummaryJSON(
            generatedAt: "2026-06-04T02:59:00Z",
            topJSON: #""client":"codex","model":"gpt-5.5""#,
            accuracyJSON: #""confidence":"medium","sourceKinds":["local-scan"],"warnings":[]"#,
            healthExtraJSON: #", "historyGeneratedAt":"2026-06-02T23:00:00Z""#
        ).data(using: .utf8)!
        var summary = try TokscaleSummary.decode(data)
        let now = try isoDate("2026-06-04T03:00:00Z")

        XCTAssertTrue(summary.needsScanOnOpen(now: now, minimumInterval: 60))
        summary.refreshFreshness(now: now)

        XCTAssertTrue(summary.stale)
        XCTAssertEqual(summary.staleReason, "history-older-than-26h")
    }

    func testSummaryRequestsOpenRefreshWhenStaleOrOlderThanMinimumInterval() throws {
        var staleSummary = try TokscaleSummary.decode(sampleSummaryData())
        staleSummary.refreshFreshness(now: try isoDate("2026-06-04T04:26:00Z"))

        XCTAssertTrue(staleSummary.needsRefreshOnOpen(now: try isoDate("2026-06-04T04:26:00Z")))

        let oldSummary = try TokscaleSummary.decode(sampleSummaryData())
        XCTAssertTrue(
            oldSummary.needsRefreshOnOpen(
                now: try isoDate("2026-06-04T02:27:01Z"),
                minimumInterval: 60
            )
        )

        XCTAssertFalse(
            oldSummary.needsRefreshOnOpen(
                now: try isoDate("2026-06-04T02:26:30Z"),
                minimumInterval: 60
            )
        )
    }

    func testSummaryRequestsOpenRefreshWhenGeneratedAtCannotBeParsed() throws {
        let data = sampleSummaryJSON(
            generatedAt: "not-a-date",
            topJSON: #""client":"codex","model":"gpt-5.5""#,
            accuracyJSON: #""confidence":"medium","sourceKinds":["local-scan"],"warnings":[]"#
        ).data(using: .utf8)!
        let summary = try TokscaleSummary.decode(data)

        XCTAssertTrue(summary.needsRefreshOnOpen(now: try isoDate("2026-06-04T02:26:30Z")))
    }

    func testSummaryDoesNotRequestOpenRefreshSoonAfterQuotaRefresh() throws {
        let data = sampleSummaryJSON(
            generatedAt: "2026-06-04T02:25:56Z",
            topJSON: #""client":"codex","model":"gpt-5.5""#,
            accuracyJSON: #""confidence":"medium","sourceKinds":["local-scan"],"warnings":[]"#,
            healthExtraJSON: #","quotaRefreshedAt":"2026-06-04T02:30:00Z""#
        ).data(using: .utf8)!
        let summary = try TokscaleSummary.decode(data)

        XCTAssertFalse(
            summary.needsRefreshOnOpen(
                now: try isoDate("2026-06-04T02:30:30Z"),
                minimumInterval: 60
            )
        )

        XCTAssertTrue(
            summary.needsRefreshOnOpen(
                now: try isoDate("2026-06-04T02:31:30Z"),
                minimumInterval: 60
            )
        )
    }

    func testSummaryRequestsScanOnOpenWhenUsageStaleDespiteRecentQuotaRefresh() throws {
        // Usage (generatedAt) is 4m+ stale, but quota was just refreshed 30s ago.
        // needsRefreshOnOpen is fooled by the recent quota refresh and returns false;
        // needsScanOnOpen must still ask for a full re-scan, because quota-only
        // refreshes never update local usage (tokens / contribution / subagents).
        let data = sampleSummaryJSON(
            generatedAt: "2026-06-04T02:25:56Z",
            topJSON: #""client":"codex","model":"gpt-5.5""#,
            accuracyJSON: #""confidence":"medium","sourceKinds":["local-scan"],"warnings":[]"#,
            healthExtraJSON: #","quotaRefreshedAt":"2026-06-04T02:30:00Z""#
        ).data(using: .utf8)!
        let summary = try TokscaleSummary.decode(data)

        XCTAssertFalse(
            summary.needsRefreshOnOpen(
                now: try isoDate("2026-06-04T02:30:30Z"),
                minimumInterval: 60
            )
        )
        XCTAssertTrue(
            summary.needsScanOnOpen(
                now: try isoDate("2026-06-04T02:30:30Z"),
                minimumInterval: 60
            )
        )
    }

    func testSummaryDoesNotRequestScanOnOpenWhenUsageFresh() throws {
        let data = sampleSummaryJSON(
            generatedAt: "2026-06-04T02:25:56Z",
            topJSON: #""client":"codex","model":"gpt-5.5""#,
            accuracyJSON: #""confidence":"medium","sourceKinds":["local-scan"],"warnings":[]"#
        ).data(using: .utf8)!
        let summary = try TokscaleSummary.decode(data)

        XCTAssertFalse(
            summary.needsScanOnOpen(
                now: try isoDate("2026-06-04T02:26:30Z"),
                minimumInterval: 60
            )
        )
        XCTAssertTrue(
            summary.needsScanOnOpen(
                now: try isoDate("2026-06-04T02:27:01Z"),
                minimumInterval: 60
            )
        )
    }

    func testDashboardModelBuildsMultiClientDashboardSections() throws {
        let summary = try TokscaleSummary.decode(sampleSummaryData())

        let dashboard = TokscaleDashboardModel(summary: summary)

        XCTAssertEqual(dashboard.hero.title, "$399")
        XCTAssertEqual(dashboard.hero.subtitle, "4 AI clients - local cache")
        XCTAssertEqual(dashboard.hero.progressLabel, "199% of account avg")
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
        XCTAssertEqual(dashboard.providerFocus(for: "codex").modelCostDetail, "gpt-5.5 $280.00 / composer $70.00")
        XCTAssertEqual(dashboard.providerDetails(for: "claude").title, "Claude")
    }

    func testDashboardModelBuildsSelectedProviderFocus() throws {
        let summary = try TokscaleSummary.decode(sampleSummaryData())

        let dashboard = TokscaleDashboardModel(summary: summary)

        let claude = dashboard.providerFocus(for: "claude")
        XCTAssertEqual(claude.id, "claude")
        XCTAssertEqual(claude.title, "Claude")
        XCTAssertEqual(claude.topModel, "claude-sonnet")
        XCTAssertEqual(claude.modelCostDetail, "claude-fable-5")
        XCTAssertEqual(claude.today, "$30.00 · 20M")
        XCTAssertEqual(claude.quotaWindows.map(\.title), ["5h", "Week"])
        XCTAssertEqual(claude.primaryQuota?.title, "5h")
        XCTAssertEqual(claude.weeklyQuota?.title, "Week")
        XCTAssertEqual(claude.quotaStatus, "Live")
        XCTAssertEqual(claude.focusedModelTime, "Sonnet-only unavailable")

        let gemini = dashboard.providerFocus(for: "gemini")
        XCTAssertEqual(gemini.id, "gemini")
        XCTAssertEqual(gemini.title, "Gemini")
        XCTAssertNil(gemini.primaryQuota)
        XCTAssertTrue(gemini.quotaWindows.isEmpty)
        XCTAssertEqual(gemini.quotaStatus, "No live quota")
    }

    func testDashboardModelBuildsQuotaBoardForPrimaryProviders() throws {
        let summary = try TokscaleSummary.decode(sampleSummaryData())

        let dashboard = TokscaleDashboardModel(summary: summary)

        XCTAssertEqual(dashboard.quotaBoardProviders.map(\.id), ["claude", "codex", "gemini"])
        XCTAssertEqual(dashboard.quotaBoardProviders[0].quotaStatus, "Live")
        XCTAssertEqual(dashboard.quotaBoardProviders[1].quotaStatus, "No live quota")
        XCTAssertEqual(dashboard.quotaBoardProviders[2].quotaStatus, "No live quota")

        let claude = dashboard.quotaBoardProviders[0]
        let primaryQuota = try XCTUnwrap(claude.primaryQuota)
        XCTAssertEqual(primaryQuota.title, "5h")
        XCTAssertEqual(primaryQuota.value(for: .remaining), "28% left")
        XCTAssertEqual(primaryQuota.detail(for: .remaining), "72% used")
        XCTAssertEqual(primaryQuota.progress(for: .remaining), 0.28, accuracy: 0.01)
        XCTAssertEqual(primaryQuota.value(for: .used), "72% used")
        XCTAssertEqual(primaryQuota.detail(for: .used), "28% left")
        XCTAssertEqual(primaryQuota.progress(for: .used), 0.72, accuracy: 0.01)
        XCTAssertEqual(claude.weeklyQuota?.title, "Week")
    }

    func testDashboardModelIncludesGrokQuotaOnBoard() throws {
        let data = sampleSummaryJSON(
            topJSON: #""client":"codex","model":"gpt-5.5""#,
            accuracyJSON: #""confidence":"medium","sourceKinds":["local-scan"],"warnings":[]"#,
            quotaJSON: """
            [
              {
                "provider": "Grok",
                "plan": "SuperGrok",
                "windows": [
                  {
                    "label": "Credits",
                    "usedPercent": 15.0,
                    "remainingPercent": 85.0,
                    "remainingLabel": "85.0% left",
                    "resetsAt": "2026-07-01T00:00:00+00:00"
                  }
                ]
              }
            ]
            """
        ).data(using: .utf8)!
        let summary = try TokscaleSummary.decode(data)

        let grok = try XCTUnwrap(TokscaleDashboardModel(summary: summary).quotaBoardProviders.first { $0.id == "grok" })

        XCTAssertEqual(grok.title, "Grok")
        XCTAssertEqual(grok.primaryQuota?.title, "Credits")
        XCTAssertEqual(grok.primaryQuota?.value(for: .remaining), "85.0% left")
        XCTAssertEqual(grok.primaryQuota?.detail(for: .remaining), "15% used")
    }

    func testDashboardModelUsesSingleSharedGrokCreditQuota() throws {
        let data = """
        {
          "version": 1,
          "generatedAt": "2026-06-16T02:00:00Z",
          "stale": false,
          "collapsed": {"metric": "todayCost", "label": "$4.00", "state": "normal"},
          "today": {"date": "2026-06-16", "costUsd": 4.0, "tokens": 4000, "messages": 12},
          "totals": {
            "costUsd": 12.0,
            "tokens": 12000,
            "activeDays": 1,
            "clients": ["grok"],
            "models": 2
          },
          "providers": [
            {
              "client": "grok",
              "costUsd": 12.0,
              "tokens": 12000,
              "messages": 36,
              "todayCostUsd": 4.0,
              "todayTokens": 4000,
              "todayMessages": 12,
              "topModel": "grok-build",
              "models": [
                {
                  "model": "grok-build",
                  "costUsd": 8.0,
                  "tokens": 8000,
                  "messages": 24,
                  "todayCostUsd": 3.0,
                  "todayTokens": 3000,
                  "todayMessages": 9
                },
                {
                  "model": "grok-composer-2.5-fast",
                  "costUsd": 4.0,
                  "tokens": 4000,
                  "messages": 12,
                  "todayCostUsd": 1.0,
                  "todayTokens": 1000,
                  "todayMessages": 3
                }
              ]
            }
          ],
          "quota": [
            {
              "provider": "Grok",
              "plan": "SuperGrok",
              "windows": [
                {
                  "label": "Credits",
                  "usedPercent": 20.0,
                  "remainingPercent": 80.0,
                  "remainingLabel": "80.0% left",
                  "resetsAt": "2026-07-01T00:00:00+00:00"
                }
              ]
            }
          ],
          "history": [],
          "top": {"client": "grok", "model": "grok-build"},
          "health": {
            "summaryPath": "/tmp/summary.json",
            "lastScanDurationMs": 100,
            "quotaRefreshedAt": "2026-06-16T02:00:00Z",
            "warnings": []
          },
          "accuracy": {"confidence": "high", "sourceKinds": ["local-scan"], "warnings": []}
        }
        """.data(using: .utf8)!
        let summary = try TokscaleSummary.decode(data)

        let grok = try XCTUnwrap(TokscaleDashboardModel(summary: summary).quotaBoardProviders.first { $0.id == "grok" })

        XCTAssertEqual(grok.quotaWindows.count, 1)
        XCTAssertEqual(grok.primaryQuota?.title, "Credits")
        XCTAssertEqual(grok.primaryQuota?.value(for: .remaining), "80.0% left")
        XCTAssertEqual(grok.modelCostDetail, "grok $3.00 / composer $1.00")
    }

    func testDashboardTreatsQuotaRefreshAsLiveWhenHistoryIsStale() throws {
        let data = sampleSummaryJSON(
            generatedAt: "2026-06-04T02:25:56.459117+00:00",
            topJSON: #""client":"codex","model":"gpt-5.5""#,
            accuracyJSON: #""confidence":"medium","sourceKinds":["local-scan"],"warnings":[]"#,
            healthExtraJSON: #","quotaRefreshedAt":"2026-06-04T04:25:56Z""#
        ).data(using: .utf8)!
        var summary = try TokscaleSummary.decode(data)
        summary.refreshFreshness(now: try isoDate("2026-06-04T04:26:00Z"))

        let dashboard = TokscaleDashboardModel(
            summary: summary,
            now: try isoDate("2026-06-04T04:26:00Z")
        )

        XCTAssertTrue(summary.stale)
        XCTAssertEqual(dashboard.quotaBoardProviders[0].quotaStatus, "Live")
    }

    func testDashboardModelPreservesOneDecimalForQuotaPercentages() throws {
        let data = sampleSummaryJSON(
            topJSON: #""client":"claude","model":"claude-sonnet""#,
            accuracyJSON: #""confidence":"medium","sourceKinds":["local-scan"],"warnings":[]"#,
            quotaJSON: """
            [
              {
                "provider": "Claude",
                "plan": "Max 5x",
                "windows": [
                  {
                    "label": "Session",
                    "usedPercent": 72.4,
                    "remainingPercent": 27.6,
                    "resetsAt": "2026-06-04T10:00:00Z"
                  }
                ]
              }
            ]
            """
        ).data(using: .utf8)!
        let summary = try TokscaleSummary.decode(data)

        let quota = try XCTUnwrap(TokscaleDashboardModel(summary: summary).quotaBoardProviders[0].primaryQuota)

        XCTAssertEqual(quota.value(for: .remaining), "27.6% left")
        XCTAssertEqual(quota.detail(for: .remaining), "72.4% used")
        XCTAssertEqual(quota.value(for: .used), "72.4% used")
        XCTAssertEqual(quota.detail(for: .used), "27.6% left")
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
        XCTAssertEqual(summary.history.count, 14)
        XCTAssertEqual(summary.history[0].date, "2026-05-22")
        XCTAssertEqual(summary.history[13].date, "2026-06-04")
        XCTAssertEqual(summary.history[13].costUsd, 398.56475810000006)
    }

    func testDashboardModelBuildsQuotaAndHistorySections() throws {
        let summary = try TokscaleSummary.decode(sampleSummaryData())

        let dashboard = TokscaleDashboardModel(summary: summary)

        XCTAssertEqual(dashboard.quotaWindows.count, 2)
        XCTAssertEqual(dashboard.quotaWindows[0].provider, "Claude")
        XCTAssertEqual(dashboard.quotaWindows[0].title, "5h")
        XCTAssertEqual(dashboard.quotaWindows[0].value, "28% left")
        XCTAssertEqual(dashboard.quotaWindows[0].detail, "72% used")
        XCTAssertEqual(dashboard.quotaWindows[0].progress, 0.72, accuracy: 0.01)
        XCTAssertEqual(dashboard.quotaWindows[1].title, "Week")
        XCTAssertEqual(dashboard.historyTrend.count, 14)
        XCTAssertEqual(dashboard.historyTrend[0].value, "$2.00")
        XCTAssertEqual(dashboard.historyTrend[13].value, "$398.56")
        XCTAssertEqual(dashboard.historyPeak?.date, "2026-06-04")
        XCTAssertEqual(dashboard.previousWeekTrend.count, 7)
        XCTAssertEqual(dashboard.currentWeekTrend.count, 7)
        XCTAssertEqual(dashboard.spendHighlights[0].title, "Today")
        XCTAssertEqual(dashboard.spendHighlights[0].value, "$398.56")
        XCTAssertEqual(dashboard.spendHighlights[1].title, "All-time")
        XCTAssertEqual(dashboard.spendHighlights[1].value, "$24.0K")
        XCTAssertEqual(dashboard.spendHighlights[2].title, "7d spend")
        XCTAssertEqual(dashboard.spendHighlights[2].value, "$588.56")
        XCTAssertEqual(dashboard.spendHighlights[2].detail, "+320% vs prior 7d")
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
        accuracyJSON: String,
        quotaJSON: String? = nil,
        healthExtraJSON: String = ""
    ) -> String {
        let quotaJSON = quotaJSON ?? """
            [
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
            ]
            """
        return """
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
              "topModel": "gpt-5.5",
              "models": [
                {
                  "model": "gpt-5.5",
                  "costUsd": 9000.0,
                  "tokens": 12000000000,
                  "messages": 40000,
                  "todayCostUsd": 280.0,
                  "todayTokens": 400000000,
                  "todayMessages": 1900
                },
                {
                  "model": "codex-composer",
                  "costUsd": 3000.0,
                  "tokens": 4000000000,
                  "messages": 12000,
                  "todayCostUsd": 70.0,
                  "todayTokens": 100000000,
                  "todayMessages": 500
                }
              ]
            },
            {
              "client": "claude",
              "costUsd": 6000.0,
              "tokens": 9000000000,
              "messages": 28000,
              "todayCostUsd": 30.0,
              "todayTokens": 20000000,
              "todayMessages": 90,
              "topModel": "claude-sonnet",
              "models": [
                {
                  "model": "claude-sonnet",
                  "costUsd": 5900.0,
                  "tokens": 8500000000,
                  "messages": 27000,
                  "todayCostUsd": 0.0,
                  "todayTokens": 0,
                  "todayMessages": 0
                },
                {
                  "model": "claude-fable-5",
                  "costUsd": 100.0,
                  "tokens": 500000000,
                  "messages": 1000,
                  "todayCostUsd": 0.0,
                  "todayTokens": 20000000,
                  "todayMessages": 90
                }
              ]
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
          "quota": \(quotaJSON),
          "history": [
            {"date": "2026-05-22", "costUsd": 2.0, "tokens": 200000, "messages": 2},
            {"date": "2026-05-23", "costUsd": 8.0, "tokens": 800000, "messages": 8},
            {"date": "2026-05-24", "costUsd": 12.0, "tokens": 1200000, "messages": 12},
            {"date": "2026-05-25", "costUsd": 18.0, "tokens": 1800000, "messages": 18},
            {"date": "2026-05-26", "costUsd": 24.0, "tokens": 2400000, "messages": 24},
            {"date": "2026-05-27", "costUsd": 32.0, "tokens": 3200000, "messages": 32},
            {"date": "2026-05-28", "costUsd": 44.0, "tokens": 4400000, "messages": 44},
            {"date": "2026-05-29", "costUsd": 10.0, "tokens": 1000000, "messages": 10},
            {"date": "2026-05-30", "costUsd": 20.0, "tokens": 2000000, "messages": 20},
            {"date": "2026-05-31", "costUsd": 30.0, "tokens": 3000000, "messages": 30},
            {"date": "2026-06-01", "costUsd": 40.0, "tokens": 4000000, "messages": 40},
            {"date": "2026-06-02", "costUsd": 50.0, "tokens": 5000000, "messages": 50},
            {"date": "2026-06-03", "costUsd": 40.0, "tokens": 4000000, "messages": 40},
            {"date": "2026-06-04", "costUsd": 398.56475810000006, "tokens": 522596373, "messages": 2501}
          ],
          "top": {
            \(topJSON)
          },
          "health": {
            "summaryPath": "/Users/example/.config/tokens/cache/companion-summary.json",
            "lastScanDurationMs": 300943,
            "warnings": []
            \(healthExtraJSON)
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
