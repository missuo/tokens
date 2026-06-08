import XCTest

@testable import TokscaleMenuBarCore

final class GraphCompanionAdapterTests: XCTestCase {
    // graph payload: camelCase body, snake_case subagents block (matches the real
    // `tokens graph --subagents` output). Two days, today = 2026-06-07.
    private let graphJSON = """
    {
      "meta": { "generatedAt": "2026-06-07T03:00:00+00:00", "version": "3.0.3",
                "dateRange": { "start": "2026-06-06", "end": "2026-06-07" } },
      "summary": {
        "totalTokens": 3500, "totalCost": 35.0, "activeDays": 2,
        "averagePerDay": 17.5, "maxCostInSingleDay": 20.0,
        "clients": ["claude", "codex"],
        "models": ["claude-opus-4-8", "gpt-5"]
      },
      "years": [],
      "contributions": [
        {
          "date": "2026-06-06", "intensity": 2,
          "totals": { "tokens": 1500, "cost": 15.0, "messages": 8 },
          "tokenBreakdown": { "input": 1500, "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 },
          "clients": [
            { "client": "claude", "modelId": "claude-opus-4-8", "providerId": "anthropic",
              "cost": 10.0, "messages": 5,
              "tokens": { "input": 1000, "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 } },
            { "client": "codex", "modelId": "gpt-5", "providerId": "openai",
              "cost": 5.0, "messages": 3,
              "tokens": { "input": 500, "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 } }
          ]
        },
        {
          "date": "2026-06-07", "intensity": 4,
          "totals": { "tokens": 2000, "cost": 20.0, "messages": 10 },
          "tokenBreakdown": { "input": 2000, "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 },
          "clients": [
            { "client": "claude", "modelId": "claude-opus-4-8", "providerId": "anthropic",
              "cost": 20.0, "messages": 10,
              "tokens": { "input": 2000, "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 } }
          ]
        }
      ],
      "subagents": {
        "session_count": 2, "invocation_count": 5,
        "total_tokens": 700, "total_messages": 20,
        "agents": [
          { "name": "Explore", "tokens": 700, "messages": 20, "sessions": 2, "invocations": 5 }
        ]
      }
    }
    """

    private let usageJSON = """
    [
      { "provider": "Claude", "plan": "Max", "metrics": [
          { "label": "Session", "used_percent": 12.0, "remaining_percent": 88.0,
            "remaining_label": "88% left", "resets_at": "2026-06-07T08:00:00+00:00" },
          { "label": "Weekly", "used_percent": 51.0, "remaining_percent": 49.0,
            "remaining_label": null, "resets_at": null }
      ] },
      { "provider": "Copilot", "plan": "Individual", "metrics": [] }
    ]
    """

    private func makeSummary(usage: Data?) throws -> TokscaleSummary {
        let companion = try GraphCompanionAdapter.companionJSON(
            graphData: Data(graphJSON.utf8),
            usageData: usage,
            todayDate: "2026-06-07",
            summaryPath: "/tmp/companion-summary.json",
            lastScanDurationMs: 123,
            quotaRefreshedAt: usage != nil ? "2026-06-07T03:01:00+00:00" : nil
        )
        return try TokscaleSummary.decode(companion)
    }

    func testTotalsPassThroughFromGraphSummary() throws {
        let summary = try makeSummary(usage: Data(usageJSON.utf8))
        XCTAssertEqual(summary.totals.tokens, 3500)
        XCTAssertEqual(summary.totals.costUsd, 35.0, accuracy: 0.001)
        XCTAssertEqual(summary.totals.activeDays, 2)
        XCTAssertEqual(summary.totals.models, 2)
        XCTAssertEqual(summary.totals.clients, ["claude", "codex"])
    }

    func testTodayPicksTheRightDay() throws {
        let summary = try makeSummary(usage: Data(usageJSON.utf8))
        XCTAssertEqual(summary.today.date, "2026-06-07")
        XCTAssertEqual(summary.today.costUsd, 20.0, accuracy: 0.001)
        XCTAssertEqual(summary.today.tokens, 2000)
        XCTAssertEqual(summary.today.messages, 10)
    }

    func testProvidersAggregateAcrossDaysSortedByCost() throws {
        let summary = try makeSummary(usage: Data(usageJSON.utf8))
        XCTAssertEqual(summary.providers.count, 2)
        let claude = summary.providers[0]
        XCTAssertEqual(claude.client, "claude")  // 30 > 5, sorted first
        XCTAssertEqual(claude.costUsd, 30.0, accuracy: 0.001)
        XCTAssertEqual(claude.tokens, 3000)
        XCTAssertEqual(claude.messages, 15)
        XCTAssertEqual(claude.todayCostUsd, 20.0, accuracy: 0.001)
        XCTAssertEqual(claude.todayTokens, 2000)
        XCTAssertEqual(claude.topModel, "claude-opus-4-8")
        XCTAssertEqual(summary.providers[1].client, "codex")
        XCTAssertEqual(summary.providers[1].costUsd, 5.0, accuracy: 0.001)
    }

    func testQuotaFromUsageDropsEmptyProviders() throws {
        let summary = try makeSummary(usage: Data(usageJSON.utf8))
        // Copilot has no metrics → dropped. Only Claude remains.
        XCTAssertEqual(summary.quota.count, 1)
        let claude = summary.quota[0]
        XCTAssertEqual(claude.provider, "Claude")
        XCTAssertEqual(claude.plan, "Max")
        XCTAssertEqual(claude.windows.count, 2)
        XCTAssertEqual(claude.windows[0].label, "Session")
        XCTAssertEqual(claude.windows[0].remainingPercent, 88.0, accuracy: 0.001)
    }

    func testSubagentsConvertSnakeToCamelAndShare() throws {
        let summary = try makeSummary(usage: Data(usageJSON.utf8))
        let subagents = try XCTUnwrap(summary.subagents)
        XCTAssertEqual(subagents.sessions, 2)
        XCTAssertEqual(subagents.invocations, 5)
        XCTAssertEqual(subagents.tokens, 700)
        XCTAssertEqual(subagents.messages, 20)
        XCTAssertEqual(subagents.share, 700.0 / 3500.0, accuracy: 0.0001)
        XCTAssertEqual(subagents.top.first?.name, "Explore")
        XCTAssertEqual(subagents.top.first?.invocations, 5)
    }

    func testTopClientByCost() throws {
        let summary = try makeSummary(usage: Data(usageJSON.utf8))
        XCTAssertEqual(summary.top.client, "claude")
        XCTAssertEqual(summary.top.model, "claude-opus-4-8")
    }

    func testNoUsageLeavesQuotaEmpty() throws {
        let summary = try makeSummary(usage: nil)
        XCTAssertTrue(summary.quota.isEmpty)
    }

    func testPatchedQuotaKeepsPreviousWhenUsageEmpty() throws {
        let base = try GraphCompanionAdapter.companionJSON(
            graphData: Data(graphJSON.utf8),
            usageData: Data(usageJSON.utf8),
            todayDate: "2026-06-07",
            summaryPath: "/tmp/companion-summary.json",
            lastScanDurationMs: 1,
            quotaRefreshedAt: "2026-06-07T03:01:00+00:00"
        )
        // Empty usage array → patchedQuota returns nil → caller keeps previous quota.
        let patched = GraphCompanionAdapter.patchedQuota(
            companionData: base,
            usageData: Data("[]".utf8),
            quotaRefreshedAt: "2026-06-07T04:00:00+00:00"
        )
        XCTAssertNil(patched)
    }

    func testPatchedQuotaReplacesWindowsWhenUsagePresent() throws {
        let base = try GraphCompanionAdapter.companionJSON(
            graphData: Data(graphJSON.utf8),
            usageData: nil,
            todayDate: "2026-06-07",
            summaryPath: "/tmp/companion-summary.json",
            lastScanDurationMs: 1,
            quotaRefreshedAt: nil
        )
        let patched = try XCTUnwrap(
            GraphCompanionAdapter.patchedQuota(
                companionData: base,
                usageData: Data(usageJSON.utf8),
                quotaRefreshedAt: "2026-06-07T04:00:00+00:00"
            )
        )
        let summary = try TokscaleSummary.decode(patched)
        XCTAssertEqual(summary.quota.count, 1)
        XCTAssertEqual(summary.quota[0].provider, "Claude")
        XCTAssertEqual(summary.health.quotaRefreshedAt, "2026-06-07T04:00:00+00:00")
    }
}
