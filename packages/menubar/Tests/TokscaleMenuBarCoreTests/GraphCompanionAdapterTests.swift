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

    private func remoteProfile(dayOneTokens: Int64, dayTwoTokens: Int64) -> Data {
        let total = dayOneTokens + dayTwoTokens
        let dayOneCost = Double(dayOneTokens) / 100
        let dayTwoCost = Double(dayTwoTokens) / 100
        let totalCost = dayOneCost + dayTwoCost
        return Data(
            """
            {
              "stats": { "totalTokens": \(total), "totalCost": \(totalCost), "activeDays": 2 },
              "dateRange": { "start": "2026-06-06", "end": "2026-06-07" },
              "clients": ["claude"], "models": ["remote-model"],
              "contributions": [
                {
                  "date": "2026-06-06", "intensity": 2,
                  "totals": { "tokens": \(dayOneTokens), "cost": \(dayOneCost), "messages": 0 },
                  "clients": [{
                    "client": "claude", "modelId": "", "cost": \(dayOneCost), "messages": 4,
                    "tokens": { "input": \(dayOneTokens), "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 },
                    "models": { "remote-model": { "cost": \(dayOneCost), "input": \(dayOneTokens), "output": 0, "messages": 4, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 } }
                  }]
                },
                {
                  "date": "2026-06-07", "intensity": 4,
                  "totals": { "tokens": \(dayTwoTokens), "cost": \(dayTwoCost), "messages": 0 },
                  "clients": [{
                    "client": "claude", "modelId": "", "cost": \(dayTwoCost), "messages": 5,
                    "tokens": { "input": \(dayTwoTokens), "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 },
                    "models": { "remote-model": { "cost": \(dayTwoCost), "input": \(dayTwoTokens), "output": 0, "messages": 5, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 } }
                  }]
                }
              ]
            }
            """.utf8
        )
    }

    private func remoteProfileData(
        days: [(date: String, tokens: Int64)],
        allTime: (tokens: Int64, cost: Double, activeDays: Int)? = nil,
        allTimeStart: String? = nil
    ) throws -> Data {
        let contributions: [[String: Any]] = days.map { day in
            let cost = Double(day.tokens) / 100
            return [
                "date": day.date,
                "intensity": day.tokens > 0 ? 2 : 0,
                "totals": ["tokens": day.tokens, "cost": cost, "messages": 0],
                "clients": [[
                    "client": "claude",
                    "modelId": "",
                    "cost": cost,
                    "messages": day.tokens > 0 ? 1 : 0,
                    "tokens": [
                        "input": day.tokens, "output": 0, "cacheRead": 0,
                        "cacheWrite": 0, "reasoning": 0,
                    ],
                    "models": [
                        "remote-model": [
                            "cost": cost, "input": day.tokens, "output": 0,
                            "messages": day.tokens > 0 ? 1 : 0, "cacheRead": 0,
                            "cacheWrite": 0, "reasoning": 0,
                        ]
                    ],
                ]],
            ]
        }
        let rollingTokens = days.reduce(Int64(0)) { $0 + $1.tokens }
        let rollingCost = days.reduce(0.0) { $0 + Double($1.tokens) / 100 }
        let rollingActiveDays = days.filter { $0.tokens > 0 }.count
        let windowStart = try XCTUnwrap(allTimeStart ?? days.first?.date)
        let windowEnd = try XCTUnwrap(days.last?.date)
        return try JSONSerialization.data(withJSONObject: [
            "stats": [
                "totalTokens": allTime?.tokens ?? rollingTokens,
                "totalCost": allTime?.cost ?? rollingCost,
                "activeDays": allTime?.activeDays ?? rollingActiveDays,
            ],
            "dateRange": [
                "start": windowStart,
                "end": windowEnd,
            ],
            "clients": ["claude"],
            "models": ["remote-model"],
            "contributions": contributions,
        ])
    }

    private func replacingLocalDailyTokens(
        in companion: Data,
        with values: [String: Int64]
    ) throws -> Data {
        var dict = try XCTUnwrap(
            JSONSerialization.jsonObject(with: companion) as? [String: Any]
        )
        var localDaily = try XCTUnwrap(dict["localDaily"] as? [[String: Any]])
        for index in localDaily.indices {
            guard let date = localDaily[index]["date"] as? String,
                let tokens = values[date],
                var totals = localDaily[index]["totals"] as? [String: Any]
            else { continue }
            totals["tokens"] = tokens
            localDaily[index]["totals"] = totals
        }
        dict["localDaily"] = localDaily
        return try JSONSerialization.data(withJSONObject: dict)
    }

    private func addingLocalBaseline(
        to companion: Data,
        dates: [String],
        tokens: Int64
    ) throws -> Data {
        var dict = try XCTUnwrap(
            JSONSerialization.jsonObject(with: companion) as? [String: Any]
        )
        var localDaily = try XCTUnwrap(dict["localDaily"] as? [[String: Any]])
        let template = try XCTUnwrap(localDaily.first)
        for date in dates {
            var row = template
            row["date"] = date
            var totals = try XCTUnwrap(row["totals"] as? [String: Any])
            totals["tokens"] = tokens
            row["totals"] = totals
            localDaily.append(row)
        }
        dict["localDaily"] = localDaily
        return try JSONSerialization.data(withJSONObject: dict)
    }

    func testTotalsPassThroughFromGraphSummary() throws {
        let summary = try makeSummary(usage: Data(usageJSON.utf8))
        XCTAssertEqual(summary.totals.tokens, 3500)
        XCTAssertEqual(summary.totals.costUsd, 35.0, accuracy: 0.001)
        XCTAssertEqual(summary.totals.activeDays, 2)
        XCTAssertEqual(summary.totals.models, 2)
        XCTAssertEqual(summary.totals.clients, ["claude", "codex"])
        XCTAssertEqual(summary.health.historyGeneratedAt, "2026-06-07T03:00:00+00:00")
    }

    func testGraphValidationRejectsMalformedWrapperOutput() {
        XCTAssertTrue(GraphCompanionAdapter.isValidGraphData(Data(graphJSON.utf8)))
        XCTAssertFalse(GraphCompanionAdapter.isValidGraphData(Data(#"{"status":"ok"}"#.utf8)))
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

    func testProvidersExposePerModelBreakdown() throws {
        let graphJSON = """
        {
          "meta": { "generatedAt": "2026-06-07T03:00:00+00:00", "version": "3.0.3",
                    "dateRange": { "start": "2026-06-06", "end": "2026-06-07" } },
          "summary": {
            "totalTokens": 2500, "totalCost": 2.5, "activeDays": 2,
            "averagePerDay": 1.25, "maxCostInSingleDay": 1.75,
            "clients": ["grok"],
            "models": ["grok-build", "grok-composer-2.5-fast"]
          },
          "years": [],
          "contributions": [
            {
              "date": "2026-06-06", "intensity": 2,
              "totals": { "tokens": 1000, "cost": 0.75, "messages": 4 },
              "clients": [
                { "client": "grok", "modelId": "grok-build", "providerId": "xai",
                  "cost": 0.75, "messages": 4,
                  "tokens": { "input": 1000, "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 } }
              ]
            },
            {
              "date": "2026-06-07", "intensity": 4,
              "totals": { "tokens": 1500, "cost": 1.75, "messages": 9 },
              "clients": [
                { "client": "grok", "modelId": "grok-build", "providerId": "xai",
                  "cost": 1.25, "messages": 6,
                  "tokens": { "input": 1000, "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 } },
                { "client": "grok", "modelId": "grok-composer-2.5-fast", "providerId": "xai",
                  "cost": 0.5, "messages": 3,
                  "tokens": { "input": 500, "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 } }
              ]
            }
          ]
        }
        """
        let companion = try GraphCompanionAdapter.companionJSON(
            graphData: Data(graphJSON.utf8),
            usageData: nil,
            todayDate: "2026-06-07",
            summaryPath: "/tmp/companion-summary.json",
            lastScanDurationMs: 123,
            quotaRefreshedAt: nil
        )

        let summary = try TokscaleSummary.decode(companion)
        let grok = try XCTUnwrap(summary.providers.first)

        XCTAssertEqual(grok.client, "grok")
        XCTAssertEqual(grok.topModel, "grok-build")
        XCTAssertEqual(grok.models.map(\.model), ["grok-build", "grok-composer-2.5-fast"])
        XCTAssertEqual(grok.models[0].costUsd, 2.0, accuracy: 0.001)
        XCTAssertEqual(grok.models[0].todayCostUsd, 1.25, accuracy: 0.001)
        XCTAssertEqual(grok.models[1].costUsd, 0.5, accuracy: 0.001)
        XCTAssertEqual(grok.models[1].todayCostUsd, 0.5, accuracy: 0.001)
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

    func testTopModelBelongsToTopClient() throws {
        let graph = """
            {
              "meta": { "generatedAt": "2026-06-07T03:00:00+00:00", "version": "3.0.3",
                        "dateRange": { "start": "2026-06-07", "end": "2026-06-07" } },
              "summary": {
                "totalTokens": 2200, "totalCost": 220.0, "activeDays": 1,
                "averagePerDay": 220.0, "maxCostInSingleDay": 220.0,
                "clients": ["claude", "codex"],
                "models": ["claude-opus-4-8", "claude-fable-5", "gpt-5.5"]
              },
              "years": [],
              "contributions": [
                {
                  "date": "2026-06-07", "intensity": 4,
                  "totals": { "tokens": 2200, "cost": 220.0, "messages": 22 },
                  "clients": [
                    { "client": "claude", "modelId": "claude-opus-4-8", "providerId": "anthropic",
                      "cost": 70.0, "messages": 7,
                      "tokens": { "input": 700, "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 } },
                    { "client": "claude", "modelId": "claude-fable-5", "providerId": "anthropic",
                      "cost": 50.0, "messages": 5,
                      "tokens": { "input": 500, "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 } },
                    { "client": "codex", "modelId": "gpt-5.5", "providerId": "openai",
                      "cost": 100.0, "messages": 10,
                      "tokens": { "input": 1000, "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 } }
                  ]
                }
              ]
            }
            """
        let companion = try GraphCompanionAdapter.companionJSON(
            graphData: Data(graph.utf8),
            usageData: nil,
            todayDate: "2026-06-07",
            summaryPath: "/tmp/companion-summary.json",
            lastScanDurationMs: 1,
            quotaRefreshedAt: nil
        )
        let summary = try TokscaleSummary.decode(companion)

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

    func testRemoteProfileKeepsAccountAggregatesWhileLocalTodayWinsPresentation() throws {
        let full = try GraphCompanionAdapter.companionJSON(
            graphData: Data(graphJSON.utf8),
            usageData: Data(usageJSON.utf8),
            todayDate: "2026-06-07",
            summaryPath: "/tmp/companion-summary.json",
            lastScanDurationMs: 123,
            quotaRefreshedAt: "2026-06-07T03:01:00+00:00"
        )
        let patched = try XCTUnwrap(
            GraphCompanionAdapter.patchRemoteProfile(
                companionData: full,
                profileData: remoteProfile(dayOneTokens: 1600, dayTwoTokens: 2200),
                todayDate: "2026-06-07",
                syncedAt: "2026-06-07T04:00:00Z",
                lastScanDurationMs: 20
            )
        )
        let summary = try TokscaleSummary.decode(patched)

        XCTAssertEqual(summary.totals.tokens, 3800)
        XCTAssertEqual(summary.totals.costUsd, 38, accuracy: 0.001)
        XCTAssertEqual(summary.history.map(\.tokens), [1600, 2200])
        XCTAssertEqual(summary.today.tokens, 2000)
        XCTAssertEqual(summary.providers.first?.tokens, 3800)
        XCTAssertEqual(summary.providers.first?.todayTokens, 2000)
        XCTAssertEqual(summary.quota.count, 1)
        XCTAssertEqual(summary.health.historyGeneratedAt, "2026-06-07T04:00:00Z")
        XCTAssertEqual(summary.accuracy.sourceKinds, ["tokens-ci", "local-today"])
        XCTAssertNil(summary.subagents)
    }

    func testRepeatedRemoteSyncDoesNotPromoteRemoteTodayToLocalPresentation() throws {
        let full = try GraphCompanionAdapter.companionJSON(
            graphData: Data(graphJSON.utf8),
            usageData: nil,
            todayDate: "2026-06-07",
            summaryPath: "/tmp/companion-summary.json",
            lastScanDurationMs: 1,
            quotaRefreshedAt: nil
        )
        let first = try XCTUnwrap(
            GraphCompanionAdapter.patchRemoteProfile(
                companionData: full,
                profileData: try remoteProfileData(days: [
                    ("2026-06-06", 1600), ("2026-06-08", 2200),
                ]),
                todayDate: "2026-06-08",
                syncedAt: "2026-06-08T01:00:00Z",
                lastScanDurationMs: 20
            )
        )
        let second = try XCTUnwrap(
            GraphCompanionAdapter.patchRemoteProfile(
                companionData: first,
                profileData: try remoteProfileData(days: [
                    ("2026-06-06", 1600), ("2026-06-08", 3300),
                ]),
                todayDate: "2026-06-08",
                syncedAt: "2026-06-08T02:00:00Z",
                lastScanDurationMs: 20
            )
        )
        let summary = try TokscaleSummary.decode(second)
        let json = try XCTUnwrap(
            JSONSerialization.jsonObject(with: second) as? [String: Any]
        )
        let provenance = try XCTUnwrap(json["historyProvenance"] as? [String: Any])

        XCTAssertEqual(summary.today.tokens, 3300)
        XCTAssertEqual(summary.totals.tokens, 4900)
        XCTAssertEqual(summary.history.map(\.tokens), [1600, 3300])
        XCTAssertEqual(summary.accuracy.sourceKinds, ["tokens-ci"])
        XCTAssertEqual(provenance["localTodayDate"] as? String, "2026-06-07")
    }

    func testRemoteProfileAcceptsAccountTotalsFarAboveLocalDevice() throws {
        let full = try GraphCompanionAdapter.companionJSON(
            graphData: Data(graphJSON.utf8),
            usageData: nil,
            todayDate: "2026-06-07",
            summaryPath: "/tmp/companion-summary.json",
            lastScanDurationMs: 123,
            quotaRefreshedAt: nil
        )
        let adjustedTotals = try replacingLocalDailyTokens(
            in: full,
            with: ["2026-06-06": 800_000_000, "2026-06-07": 987_000_000]
        )
        let adjusted = try addingLocalBaseline(
            to: adjustedTotals,
            dates: ["2026-06-01", "2026-06-02", "2026-06-03", "2026-06-04", "2026-06-05"],
            tokens: 1_000_000_000
        )

        XCTAssertNotNil(
            GraphCompanionAdapter.patchRemoteProfile(
                companionData: adjusted,
                profileData: try remoteProfileData(days: [
                    ("2026-06-01", 1_000_000_000), ("2026-06-02", 1_000_000_000),
                    ("2026-06-03", 1_000_000_000), ("2026-06-04", 1_000_000_000),
                    ("2026-06-05", 1_000_000_000), ("2026-06-06", 5_700_000_000),
                    ("2026-06-07", 14_500_000_000),
                ]),
                todayDate: "2026-06-07",
                syncedAt: "2026-06-07T04:00:00Z",
                lastScanDurationMs: 20
            )
        )
    }

    func testRemoteProfileAllowsPlausibleMultiDeviceOverlap() throws {
        let full = try GraphCompanionAdapter.companionJSON(
            graphData: Data(graphJSON.utf8),
            usageData: nil,
            todayDate: "2026-06-07",
            summaryPath: "/tmp/companion-summary.json",
            lastScanDurationMs: 1,
            quotaRefreshedAt: nil
        )
        let adjusted = try replacingLocalDailyTokens(
            in: full,
            with: ["2026-06-06": 100_000_000, "2026-06-07": 100_000_000]
        )

        XCTAssertNotNil(
            GraphCompanionAdapter.patchRemoteProfile(
                companionData: adjusted,
                profileData: try remoteProfileData(days: [
                    ("2026-06-06", 700_000_000), ("2026-06-07", 700_000_000),
                ]),
                todayDate: "2026-06-07",
                syncedAt: "2026-06-07T04:00:00Z",
                lastScanDurationMs: 20
            )
        )
    }

    func testRemoteProfileReplacesWindowAndDropsAgedOutDay() throws {
        let full = try GraphCompanionAdapter.companionJSON(
            graphData: Data(graphJSON.utf8),
            usageData: nil,
            todayDate: "2026-06-07",
            summaryPath: "/tmp/companion-summary.json",
            lastScanDurationMs: 1,
            quotaRefreshedAt: nil
        )
        let first = try XCTUnwrap(
            GraphCompanionAdapter.patchRemoteProfile(
                companionData: full,
                profileData: try remoteProfileData(days: [
                    ("2026-06-05", 500), ("2026-06-06", 1500), ("2026-06-07", 2000),
                ]),
                todayDate: "2026-06-07",
                syncedAt: "2026-06-07T04:00:00Z",
                lastScanDurationMs: 20
            )
        )
        let second = try XCTUnwrap(
            GraphCompanionAdapter.patchRemoteProfile(
                companionData: first,
                profileData: try remoteProfileData(days: [
                    ("2026-06-06", 1500), ("2026-06-07", 2000),
                ], allTime: (tokens: 4000, cost: 40, activeDays: 3),
                    allTimeStart: "2026-06-05"),
                todayDate: "2026-06-07",
                syncedAt: "2026-06-08T04:00:00Z",
                lastScanDurationMs: 20
            )
        )
        let summary = try TokscaleSummary.decode(second)

        XCTAssertEqual(summary.history.map(\.date), ["2026-06-06", "2026-06-07"])
        XCTAssertEqual(summary.history.map(\.tokens), [1500, 2000])
        XCTAssertEqual(summary.totals.tokens, 4000)
    }

    func testRemoteProfileReplacementDoesNotKeepDeletedMiddleDay() throws {
        let full = try GraphCompanionAdapter.companionJSON(
            graphData: Data(graphJSON.utf8),
            usageData: nil,
            todayDate: "2026-06-07",
            summaryPath: "/tmp/companion-summary.json",
            lastScanDurationMs: 1,
            quotaRefreshedAt: nil
        )
        let first = try XCTUnwrap(
            GraphCompanionAdapter.patchRemoteProfile(
                companionData: full,
                profileData: try remoteProfileData(days: [
                    ("2026-06-05", 500), ("2026-06-06", 1500), ("2026-06-07", 2000),
                ]),
                todayDate: "2026-06-07",
                syncedAt: "2026-06-07T04:00:00Z",
                lastScanDurationMs: 20
            )
        )
        let second = try XCTUnwrap(
            GraphCompanionAdapter.patchRemoteProfile(
                companionData: first,
                profileData: try remoteProfileData(
                    days: [("2026-06-05", 500), ("2026-06-07", 2000)]
                ),
                todayDate: "2026-06-07",
                syncedAt: "2026-06-08T04:00:00Z",
                lastScanDurationMs: 20
            )
        )
        let summary = try TokscaleSummary.decode(second)
        let json = try XCTUnwrap(
            JSONSerialization.jsonObject(with: second) as? [String: Any]
        )
        let remoteDaily = try XCTUnwrap(json["remoteDaily"] as? [[String: Any]])

        XCTAssertEqual(summary.history.map(\.date), ["2026-06-05", "2026-06-07"])
        XCTAssertEqual(remoteDaily.compactMap { $0["date"] as? String }, [
            "2026-06-05", "2026-06-07",
        ])
    }

    func testRemoteProfileDoesNotMixUnsubmittedLocalYesterdayIntoAccountHistory() throws {
        let full = try GraphCompanionAdapter.companionJSON(
            graphData: Data(graphJSON.utf8),
            usageData: nil,
            todayDate: "2026-06-07",
            summaryPath: "/tmp/companion-summary.json",
            lastScanDurationMs: 1,
            quotaRefreshedAt: nil
        )
        let patched = try XCTUnwrap(
            GraphCompanionAdapter.patchRemoteProfile(
                companionData: full,
                profileData: try remoteProfileData(days: [("2026-06-06", 1600)]),
                todayDate: "2026-06-08",
                syncedAt: "2026-06-08T00:05:00Z",
                lastScanDurationMs: 20
            )
        )
        let summary = try TokscaleSummary.decode(patched)

        XCTAssertEqual(summary.history.map(\.date), ["2026-06-06"])
        XCTAssertEqual(summary.history.map(\.tokens), [1600])
        XCTAssertEqual(summary.today.date, "2026-06-08")
        XCTAssertEqual(summary.today.tokens, 0)
        XCTAssertEqual(summary.totals.tokens, 1600)
        XCTAssertEqual(summary.totals.activeDays, 1)
        XCTAssertEqual(summary.accuracy.sourceKinds, ["tokens-ci"])
    }

    func testRemoteProfileDoesNotCompareIndividualDaysAgainstLocalDevice() throws {
        let full = try GraphCompanionAdapter.companionJSON(
            graphData: Data(graphJSON.utf8),
            usageData: nil,
            todayDate: "2026-06-07",
            summaryPath: "/tmp/companion-summary.json",
            lastScanDurationMs: 1,
            quotaRefreshedAt: nil
        )
        let adjustedTotals = try replacingLocalDailyTokens(
            in: full,
            with: ["2026-06-06": 800_000_000, "2026-06-07": 1_000_000_000]
        )
        let adjusted = try addingLocalBaseline(
            to: adjustedTotals,
            dates: ["2026-06-01", "2026-06-02", "2026-06-03", "2026-06-04", "2026-06-05"],
            tokens: 1_000_000_000
        )

        XCTAssertNotNil(
            GraphCompanionAdapter.patchRemoteProfile(
                companionData: adjusted,
                profileData: try remoteProfileData(days: [
                    ("2026-06-01", 1_000_000_000), ("2026-06-02", 1_000_000_000),
                    ("2026-06-03", 1_000_000_000), ("2026-06-04", 1_000_000_000),
                    ("2026-06-05", 1_000_000_000), ("2026-06-06", 5_700_000_000),
                    ("2026-06-07", 0),
                ]),
                todayDate: "2026-06-07",
                syncedAt: "2026-06-07T04:00:00Z",
                lastScanDurationMs: 20
            )
        )
    }

    func testRemoteProfileUsesAllTimeStatsWithRollingContributions() throws {
        let full = try GraphCompanionAdapter.companionJSON(
            graphData: Data(graphJSON.utf8),
            usageData: nil,
            todayDate: "2026-06-07",
            summaryPath: "/tmp/companion-summary.json",
            lastScanDurationMs: 1,
            quotaRefreshedAt: nil
        )
        let patched = try XCTUnwrap(
            GraphCompanionAdapter.patchRemoteProfile(
                companionData: full,
                profileData: try remoteProfileData(
                    days: [("2026-06-06", 1600), ("2026-06-07", 2200)],
                    allTime: (tokens: 10_000, cost: 100, activeDays: 2),
                    allTimeStart: "2025-01-01"
                ),
                todayDate: "2026-06-07",
                syncedAt: "2026-06-07T04:00:00Z",
                lastScanDurationMs: 20
            )
        )
        let summary = try TokscaleSummary.decode(patched)
        let dashboard = TokscaleDashboardModel(summary: summary)

        XCTAssertEqual(summary.totals.tokens, 10_000)
        XCTAssertEqual(summary.totals.costUsd, 100, accuracy: 0.001)
        XCTAssertEqual(summary.totals.activeDays, 2)
        XCTAssertEqual(summary.history.map(\.tokens), [1600, 2200])
        XCTAssertTrue(summary.accuracy.sourceKinds.contains("tokens-ci-rolling-breakdown"))
        XCTAssertTrue(dashboard.providerFocus(for: "claude").total.contains("last year"))
        XCTAssertEqual(dashboard.allTimeDays, "2 active days in the last year")
        XCTAssertEqual(dashboard.dailyAverage.value, "$19.00")
        XCTAssertEqual(dashboard.dailyAverage.detail, "last-year account · 2 active days")
        XCTAssertEqual(dashboard.hero.progressLabel, "105% of last-year account avg")
    }

    func testRemoteProfileRejectsRollingTotalsAboveAllTimeStats() throws {
        let full = try GraphCompanionAdapter.companionJSON(
            graphData: Data(graphJSON.utf8),
            usageData: nil,
            todayDate: "2026-06-07",
            summaryPath: "/tmp/companion-summary.json",
            lastScanDurationMs: 1,
            quotaRefreshedAt: nil
        )

        XCTAssertNil(
            GraphCompanionAdapter.patchRemoteProfile(
                companionData: full,
                profileData: try remoteProfileData(
                    days: [("2026-06-06", 1500)],
                    allTime: (tokens: 1499, cost: 14.99, activeDays: 2)
                ),
                todayDate: "2026-06-07",
                syncedAt: "2026-06-07T04:00:00Z",
                lastScanDurationMs: 20
            )
        )
    }

    func testRemoteProfileAcceptsLegacyCacheWithoutLocalSnapshotMarker() throws {
        let full = try GraphCompanionAdapter.companionJSON(
            graphData: Data(graphJSON.utf8),
            usageData: nil,
            todayDate: "2026-06-07",
            summaryPath: "/tmp/companion-summary.json",
            lastScanDurationMs: 1,
            quotaRefreshedAt: nil
        )
        var dict = try XCTUnwrap(
            JSONSerialization.jsonObject(with: full) as? [String: Any]
        )
        var provenance = try XCTUnwrap(dict["historyProvenance"] as? [String: Any])
        provenance.removeValue(forKey: "localSnapshotComplete")
        dict["historyProvenance"] = provenance

        XCTAssertNotNil(
            GraphCompanionAdapter.patchRemoteProfile(
                companionData: try JSONSerialization.data(withJSONObject: dict),
                profileData: remoteProfile(dayOneTokens: 1600, dayTwoTokens: 2200),
                todayDate: "2026-06-07",
                syncedAt: "2026-06-07T04:00:00Z",
                lastScanDurationMs: 20
            )
        )
    }

    func testRemoteProfileRejectsStatsContributionMismatch() throws {
        let full = try GraphCompanionAdapter.companionJSON(
            graphData: Data(graphJSON.utf8),
            usageData: nil,
            todayDate: "2026-06-07",
            summaryPath: "/tmp/companion-summary.json",
            lastScanDurationMs: 1,
            quotaRefreshedAt: nil
        )
        var profile = try XCTUnwrap(
            JSONSerialization.jsonObject(
                with: try remoteProfileData(days: [("2026-06-06", 1500)])
            ) as? [String: Any]
        )
        var stats = try XCTUnwrap(profile["stats"] as? [String: Any])
        stats["totalTokens"] = 1501
        profile["stats"] = stats

        XCTAssertNil(
            GraphCompanionAdapter.patchRemoteProfile(
                companionData: full,
                profileData: try JSONSerialization.data(withJSONObject: profile),
                todayDate: "2026-06-07",
                syncedAt: "2026-06-07T04:00:00Z",
                lastScanDurationMs: 20
            )
        )
    }

    func testSummaryMutationCoordinatorPreservesConcurrentPatches() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(
            UUID().uuidString,
            isDirectory: true
        )
        defer { try? FileManager.default.removeItem(at: directory) }
        let summaryURL = directory.appendingPathComponent("summary.json")
        let coordinator = SummaryMutationCoordinator(label: "test.summary-mutations")
        XCTAssertTrue(
            coordinator.replace(
                summaryURL: summaryURL,
                with: Data(#"{"today":0,"quota":0}"#.utf8)
            )
        )
        let group = DispatchGroup()
        group.enter()
        DispatchQueue.global().async {
            _ = coordinator.mutate(summaryURL: summaryURL) { latest in
                guard var dict = try? JSONSerialization.jsonObject(with: latest) as? [String: Any]
                else { return nil }
                dict["today"] = 42
                return try? JSONSerialization.data(withJSONObject: dict)
            }
            group.leave()
        }
        group.enter()
        DispatchQueue.global().async {
            _ = coordinator.mutate(summaryURL: summaryURL) { latest in
                guard var dict = try? JSONSerialization.jsonObject(with: latest) as? [String: Any]
                else { return nil }
                dict["quota"] = 88
                return try? JSONSerialization.data(withJSONObject: dict)
            }
            group.leave()
        }
        XCTAssertEqual(group.wait(timeout: .now() + 5), .success)
        let final = try XCTUnwrap(
            JSONSerialization.jsonObject(with: Data(contentsOf: summaryURL)) as? [String: Any]
        )

        XCTAssertEqual(final["today"] as? Int, 42)
        XCTAssertEqual(final["quota"] as? Int, 88)
    }

    func testTodayPatchUpdatesPresentationWithoutChangingRemoteAggregates() throws {
        let full = try GraphCompanionAdapter.companionJSON(
            graphData: Data(graphJSON.utf8),
            usageData: nil,
            todayDate: "2026-06-07",
            summaryPath: "/tmp/companion-summary.json",
            lastScanDurationMs: 123,
            quotaRefreshedAt: nil
        )
        let remote = try XCTUnwrap(
            GraphCompanionAdapter.patchRemoteProfile(
                companionData: full,
                profileData: remoteProfile(dayOneTokens: 1600, dayTwoTokens: 2200),
                todayDate: "2026-06-07",
                syncedAt: "2026-06-07T04:00:00Z",
                lastScanDurationMs: 20
            )
        )
        let todayGraph = Data(
            """
            {
              "meta": { "generatedAt": "2026-06-07T09:00:00Z" },
              "summary": { "totalTokens": 5000, "totalCost": 50, "activeDays": 1, "clients": ["claude"], "models": ["remote-model"] },
              "contributions": [{
                "date": "2026-06-07", "intensity": 4,
                "totals": { "tokens": 5000, "cost": 50, "messages": 10 },
                "clients": [{ "client": "claude", "modelId": "remote-model", "cost": 50, "messages": 10, "tokens": { "input": 5000, "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 } }]
              }]
            }
            """.utf8
        )
        _ = try GraphCompanionAdapter.companionJSON(
            graphData: todayGraph,
            usageData: nil,
            todayDate: "2026-06-07",
            summaryPath: "",
            lastScanDurationMs: 0,
            quotaRefreshedAt: nil
        )
        let patched = try XCTUnwrap(
            GraphCompanionAdapter.patchTodayData(
                companionData: remote,
                todayGraphData: todayGraph,
                todayDate: "2026-06-07"
            )
        )
        let summary = try TokscaleSummary.decode(patched)
        let patchedJSON = try XCTUnwrap(
            JSONSerialization.jsonObject(with: patched) as? [String: Any]
        )
        let localDaily = try XCTUnwrap(patchedJSON["localDaily"] as? [[String: Any]])
        let localToday = try XCTUnwrap(
            localDaily.first { ($0["date"] as? String) == "2026-06-07" }
        )
        let localTodayTotals = try XCTUnwrap(localToday["totals"] as? [String: Any])

        XCTAssertEqual(summary.today.tokens, 5000)
        XCTAssertEqual(summary.totals.tokens, 3800)
        XCTAssertEqual(summary.history.map(\.tokens), [1600, 2200])
        XCTAssertEqual(summary.providers.first?.tokens, 3800)
        XCTAssertEqual(summary.providers.first?.todayTokens, 5000)
        XCTAssertEqual((localTodayTotals["tokens"] as? NSNumber)?.int64Value, 5000)
    }

    func testTodayPatchAcrossMidnightKeepsRemoteAggregatesAndAddsLocalPresentation() throws {
        let full = try GraphCompanionAdapter.companionJSON(
            graphData: Data(graphJSON.utf8),
            usageData: nil,
            todayDate: "2026-06-07",
            summaryPath: "/tmp/companion-summary.json",
            lastScanDurationMs: 123,
            quotaRefreshedAt: nil
        )
        let remote = try XCTUnwrap(
            GraphCompanionAdapter.patchRemoteProfile(
                companionData: full,
                profileData: remoteProfile(dayOneTokens: 1600, dayTwoTokens: 2200),
                todayDate: "2026-06-07",
                syncedAt: "2026-06-07T23:59:00Z",
                lastScanDurationMs: 20
            )
        )
        let nextDayGraph = Data(
            """
            {
              "meta": { "generatedAt": "2026-06-08T00:10:00Z" },
              "summary": { "totalTokens": 5000, "totalCost": 50, "activeDays": 1, "clients": ["codex"], "models": ["gpt-local"] },
              "contributions": [{
                "date": "2026-06-08", "intensity": 4,
                "totals": { "tokens": 5000, "cost": 50, "messages": 10 },
                "clients": [{ "client": "codex", "modelId": "gpt-local", "cost": 50, "messages": 10, "tokens": { "input": 5000, "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 } }]
              }],
              "todayWorkTime": { "codex": 3600000 }
            }
            """.utf8
        )
        let patched = try XCTUnwrap(
            GraphCompanionAdapter.patchTodayData(
                companionData: remote,
                todayGraphData: nextDayGraph,
                todayDate: "2026-06-08"
            )
        )
        let summary = try TokscaleSummary.decode(patched)
        let remoteProvider = try XCTUnwrap(
            summary.providers.first { $0.client == "claude" }
        )
        let localProvider = try XCTUnwrap(
            summary.providers.first { $0.client == "codex" }
        )
        let localModel = try XCTUnwrap(
            localProvider.models.first { $0.model == "gpt-local" }
        )

        XCTAssertEqual(summary.today.date, "2026-06-08")
        XCTAssertEqual(summary.today.tokens, 5000)
        XCTAssertEqual(summary.totals.tokens, 3800)
        XCTAssertEqual(summary.totals.costUsd, 38, accuracy: 0.001)
        XCTAssertEqual(summary.totals.activeDays, 2)
        XCTAssertEqual(summary.history.map(\.date), ["2026-06-06", "2026-06-07"])
        XCTAssertEqual(summary.history.map(\.tokens), [1600, 2200])
        XCTAssertEqual(remoteProvider.tokens, 3800)
        XCTAssertEqual(remoteProvider.todayTokens, 0)
        XCTAssertEqual(localProvider.tokens, 0)
        XCTAssertEqual(localProvider.todayTokens, 5000)
        XCTAssertEqual(localProvider.workTimeMs, 3_600_000)
        XCTAssertEqual(localModel.tokens, 0)
        XCTAssertEqual(localModel.todayTokens, 5000)
    }

    func testTodayPatchCanClearSameDayWithoutLeavingLocalGhost() throws {
        let full = try GraphCompanionAdapter.companionJSON(
            graphData: Data(graphJSON.utf8),
            usageData: nil,
            todayDate: "2026-06-07",
            summaryPath: "/tmp/companion-summary.json",
            lastScanDurationMs: 1,
            quotaRefreshedAt: nil
        )
        let emptyToday = Data(
            """
            {
              "meta": { "generatedAt": "2026-06-07T10:00:00Z" },
              "summary": { "totalTokens": 0, "totalCost": 0, "activeDays": 0, "clients": [], "models": [] },
              "contributions": []
            }
            """.utf8
        )
        let patched = try XCTUnwrap(
            GraphCompanionAdapter.patchTodayData(
                companionData: full,
                todayGraphData: emptyToday,
                todayDate: "2026-06-07"
            )
        )
        let summary = try TokscaleSummary.decode(patched)
        let json = try XCTUnwrap(
            JSONSerialization.jsonObject(with: patched) as? [String: Any]
        )
        let localDaily = try XCTUnwrap(json["localDaily"] as? [[String: Any]])

        XCTAssertEqual(summary.today.tokens, 0)
        XCTAssertEqual(summary.totals.tokens, 1500)
        XCTAssertEqual(summary.totals.activeDays, 1)
        XCTAssertEqual(summary.history.map(\.date), ["2026-06-06"])
        XCTAssertFalse(localDaily.contains { ($0["date"] as? String) == "2026-06-07" })
    }

    func testEmptyTodayPatchRecordsLocalProvenanceOnNewDay() throws {
        let full = try GraphCompanionAdapter.companionJSON(
            graphData: Data(graphJSON.utf8),
            usageData: nil,
            todayDate: "2026-06-07",
            summaryPath: "/tmp/companion-summary.json",
            lastScanDurationMs: 1,
            quotaRefreshedAt: nil
        )
        let emptyToday = Data(
            """
            {
              "meta": { "generatedAt": "2026-06-08T00:10:00Z" },
              "summary": { "totalTokens": 0, "totalCost": 0, "activeDays": 0, "clients": [], "models": [] },
              "contributions": []
            }
            """.utf8
        )
        let patched = try XCTUnwrap(
            GraphCompanionAdapter.patchTodayData(
                companionData: full,
                todayGraphData: emptyToday,
                todayDate: "2026-06-08"
            )
        )
        let summary = try TokscaleSummary.decode(patched)
        let json = try XCTUnwrap(
            JSONSerialization.jsonObject(with: patched) as? [String: Any]
        )
        let provenance = try XCTUnwrap(json["historyProvenance"] as? [String: Any])

        XCTAssertEqual(summary.today.date, "2026-06-08")
        XCTAssertEqual(summary.today.tokens, 0)
        XCTAssertEqual(provenance["localTodayDate"] as? String, "2026-06-08")
    }

    // Claude pattern: almost every token is cache (reads + first-time writes), fresh
    // input ~0. A cache-hit rate must count cacheWrite as a miss — otherwise the
    // denominator collapses to cacheRead and the rate pins to 100%.
    // 900 read / 100 write / 0 input → 90%, not 100%.
    func testCacheHitRateCountsCacheWriteAsMiss() throws {
        let graph = """
        {
          "meta": { "generatedAt": "2026-06-07T03:00:00+00:00", "version": "3.0.3",
                    "dateRange": { "start": "2026-06-07", "end": "2026-06-07" } },
          "summary": {
            "totalTokens": 1050, "totalCost": 10.0, "activeDays": 1,
            "averagePerDay": 10.0, "maxCostInSingleDay": 10.0,
            "clients": ["claude"], "models": ["claude-opus-4-8"]
          },
          "years": [],
          "contributions": [
            {
              "date": "2026-06-07", "intensity": 4,
              "totals": { "tokens": 1050, "cost": 10.0, "messages": 4 },
              "tokenBreakdown": { "input": 0, "output": 50, "cacheRead": 900, "cacheWrite": 100, "reasoning": 0 },
              "clients": [
                { "client": "claude", "modelId": "claude-opus-4-8", "providerId": "anthropic",
                  "cost": 10.0, "messages": 4,
                  "tokens": { "input": 0, "output": 50, "cacheRead": 900, "cacheWrite": 100, "reasoning": 0 } }
              ]
            }
          ]
        }
        """
        let companion = try GraphCompanionAdapter.companionJSON(
            graphData: Data(graph.utf8),
            usageData: nil,
            todayDate: "2026-06-07",
            summaryPath: "/tmp/companion-summary.json",
            lastScanDurationMs: 1,
            quotaRefreshedAt: nil
        )
        let summary = try TokscaleSummary.decode(companion)
        let claude = try XCTUnwrap(summary.providers.first { $0.client == "claude" })
        XCTAssertEqual(claude.cacheHitPercent, 90.0, accuracy: 0.01)
    }

    // `graph --work-time` adds a top-level todayWorkTime map (client → active ms).
    // The adapter routes each into its provider, and the dashboard formats it for
    // the cards: 1h → "1h 0m", 45 min → "45m".
    func testWorkTimeFromGraphPopulatesProviders() throws {
        let graph = """
        {
          "meta": { "generatedAt": "2026-06-07T03:00:00+00:00", "version": "3.0.3",
                    "dateRange": { "start": "2026-06-07", "end": "2026-06-07" } },
          "summary": {
            "totalTokens": 3000, "totalCost": 30.0, "activeDays": 1,
            "averagePerDay": 30.0, "maxCostInSingleDay": 30.0,
            "clients": ["claude", "codex"], "models": ["claude-opus-4-8", "gpt-5"]
          },
          "years": [],
          "contributions": [
            {
              "date": "2026-06-07", "intensity": 4,
              "totals": { "tokens": 3000, "cost": 30.0, "messages": 12 },
              "tokenBreakdown": { "input": 3000, "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 },
              "clients": [
                { "client": "claude", "modelId": "claude-opus-4-8", "providerId": "anthropic",
                  "cost": 20.0, "messages": 8,
                  "tokens": { "input": 2000, "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 } },
                { "client": "codex", "modelId": "gpt-5", "providerId": "openai",
                  "cost": 10.0, "messages": 4,
                  "tokens": { "input": 1000, "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 } }
              ]
            }
          ],
          "todayWorkTime": { "claude": 3600000, "codex": 2700000 }
        }
        """
        let companion = try GraphCompanionAdapter.companionJSON(
            graphData: Data(graph.utf8),
            usageData: nil,
            todayDate: "2026-06-07",
            summaryPath: "/tmp/companion-summary.json",
            lastScanDurationMs: 1,
            quotaRefreshedAt: nil
        )
        let summary = try TokscaleSummary.decode(companion)
        let claude = try XCTUnwrap(summary.providers.first { $0.client == "claude" })
        XCTAssertEqual(claude.workTimeMs, 3_600_000)
        let codex = try XCTUnwrap(summary.providers.first { $0.client == "codex" })
        XCTAssertEqual(codex.workTimeMs, 2_700_000)

        let dashboard = TokscaleDashboardModel(summary: summary)
        XCTAssertEqual(dashboard.providerFocus(for: "claude").workTime, "1h 0m")
        XCTAssertEqual(dashboard.providerFocus(for: "codex").workTime, "45m")
    }

    // The menu bar's cheap "today" refresh replaces the same calendar day in every
    // aggregate while preserving older history and quota.
    func testPatchTodayDataRefreshesTodayAndKeepsHistoryConsistent() throws {
        let full = try GraphCompanionAdapter.companionJSON(
            graphData: Data(graphJSON.utf8),
            usageData: Data(usageJSON.utf8),
            todayDate: "2026-06-07",
            summaryPath: "/tmp/companion-summary.json",
            lastScanDurationMs: 1,
            quotaRefreshedAt: "2026-06-07T03:01:00+00:00"
        )
        // Today-only scan: only 2026-06-07, claude spent more + logged 1h work time.
        let todayGraph = """
        {
          "meta": { "generatedAt": "2026-06-07T09:00:00+00:00", "version": "3.0.3",
                    "dateRange": { "start": "2026-06-07", "end": "2026-06-07" } },
          "summary": { "totalTokens": 5000, "totalCost": 50.0, "activeDays": 1,
                       "averagePerDay": 50.0, "maxCostInSingleDay": 50.0,
                       "clients": ["claude"], "models": ["claude-opus-4-8"] },
          "years": [],
          "contributions": [
            { "date": "2026-06-07", "intensity": 4,
              "totals": { "tokens": 5000, "cost": 50.0, "messages": 25 },
              "tokenBreakdown": { "input": 5000, "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 },
              "clients": [
                { "client": "claude", "modelId": "claude-opus-4-8", "providerId": "anthropic",
                  "cost": 50.0, "messages": 25,
                  "tokens": { "input": 5000, "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 } }
              ] }
          ],
          "todayWorkTime": { "claude": 3600000 }
        }
        """
        let patched = try XCTUnwrap(
            GraphCompanionAdapter.patchTodayData(
                companionData: full,
                todayGraphData: Data(todayGraph.utf8),
                todayDate: "2026-06-07"
            )
        )
        let summary = try TokscaleSummary.decode(patched)
        // Today panel refreshed to the today-only numbers.
        XCTAssertEqual(summary.generatedAt, "2026-06-07T09:00:00+00:00")
        XCTAssertEqual(summary.health.historyGeneratedAt, "2026-06-07T03:00:00+00:00")
        XCTAssertEqual(summary.today.costUsd, 50.0, accuracy: 0.001)
        XCTAssertEqual(summary.today.tokens, 5000)
        XCTAssertEqual(summary.totals.tokens, 6500)
        XCTAssertEqual(summary.totals.costUsd, 65.0, accuracy: 0.001)
        XCTAssertEqual(summary.history.map(\.tokens), [1500, 5000])
        let claude = try XCTUnwrap(summary.providers.first { $0.client == "claude" })
        XCTAssertEqual(claude.todayCostUsd, 50.0, accuracy: 0.001)
        XCTAssertEqual(claude.workTimeMs, 3_600_000)
        XCTAssertEqual(claude.costUsd, 60.0, accuracy: 0.001)
        let claudeModel = try XCTUnwrap(claude.models.first)
        XCTAssertEqual(claudeModel.costUsd, 60.0, accuracy: 0.001)
        XCTAssertEqual(claudeModel.todayCostUsd, 50.0, accuracy: 0.001)
        let codex = try XCTUnwrap(summary.providers.first { $0.client == "codex" })
        XCTAssertEqual(codex.todayCostUsd, 0, accuracy: 0.001)
        let codexModel = try XCTUnwrap(codex.models.first)
        XCTAssertEqual(codexModel.todayCostUsd, 0, accuracy: 0.001)
        // Quota preserved from the full scan.
        XCTAssertEqual(summary.quota.count, 1)
    }
}
