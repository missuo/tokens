import Foundation

/// Builds the companion-summary JSON the menu bar already decodes, from the
/// stock `tokens graph --subagents` and `tokens usage` outputs. This lets the app
/// ride on upstream `main` instead of the fork-only `companion-summary`
/// subcommand: the same `TokscaleSummary` shape comes out the other end, so the
/// store, decoder, and all views are unchanged.
///
/// `graph` is camelCase except its opt-in `subagents` block, which is snake_case
/// (it serializes `aggregator::SubagentSummary` directly). `usage` is snake_case.
public enum GraphCompanionAdapter {
    public enum AdapterError: Error {
        case invalidGraph(String)
    }

    // MARK: - graph payload (camelCase; subagents block is snake_case)

    private struct Graph: Decodable {
        let meta: Meta
        let summary: Summary
        let contributions: [Contribution]
        let subagents: Subagents?

        struct Meta: Decodable {
            let generatedAt: String
        }

        struct Summary: Decodable {
            let totalTokens: Int64
            let totalCost: Double
            let activeDays: Int
            let clients: [String]
            let models: [String]
        }

        struct Contribution: Decodable {
            let date: String
            let intensity: Int
            let totals: Totals
            let clients: [Client]

            struct Totals: Decodable {
                let tokens: Int64
                let cost: Double
                let messages: Int
            }

            struct Client: Decodable {
                let client: String
                let modelId: String
                let cost: Double
                let messages: Int
                let tokens: Tokens

                struct Tokens: Decodable {
                    let input: Int64
                    let output: Int64
                    let cacheRead: Int64
                    let cacheWrite: Int64
                    let reasoning: Int64

                    var total: Int64 {
                        input + output + cacheRead + cacheWrite + reasoning
                    }
                }
            }
        }

        struct Subagents: Decodable {
            let sessionCount: Int
            let invocationCount: Int
            let totalTokens: Int64
            let totalMessages: Int
            let agents: [Agent]

            enum CodingKeys: String, CodingKey {
                case sessionCount = "session_count"
                case invocationCount = "invocation_count"
                case totalTokens = "total_tokens"
                case totalMessages = "total_messages"
                case agents
            }

            struct Agent: Decodable {
                let name: String
                let tokens: Int64
                let messages: Int
                let sessions: Int
                let invocations: Int
            }
        }
    }

    // MARK: - usage payload (snake_case)

    private struct UsageProvider: Decodable {
        let provider: String
        let plan: String?
        let metrics: [Metric]

        struct Metric: Decodable {
            let label: String
            let usedPercent: Double
            let remainingPercent: Double
            let remainingLabel: String?
            let resetsAt: String?

            enum CodingKeys: String, CodingKey {
                case label
                case usedPercent = "used_percent"
                case remainingPercent = "remaining_percent"
                case remainingLabel = "remaining_label"
                case resetsAt = "resets_at"
            }
        }
    }

    /// Convert raw `graph`/`usage` stdout into the companion-summary JSON the menu
    /// bar's `TokscaleSummary` decodes. `usageData` may be nil/garbage (quota fetch
    /// failed) — quota is then empty and the caller can fall back to a prior value.
    public static func companionJSON(
        graphData: Data,
        usageData: Data?,
        todayDate: String,
        summaryPath: String,
        lastScanDurationMs: Int,
        quotaRefreshedAt: String?
    ) throws -> Data {
        let graph: Graph
        do {
            graph = try JSONDecoder().decode(Graph.self, from: graphData)
        } catch {
            throw AdapterError.invalidGraph("\(error)")
        }
        let usage: [UsageProvider] =
            usageData.flatMap { try? JSONDecoder().decode([UsageProvider].self, from: $0) } ?? []

        let today = graph.contributions.first { $0.date == todayDate }
        let todayCost = today?.totals.cost ?? 0
        let todayTokens = today?.totals.tokens ?? 0
        let todayMessages = today?.totals.messages ?? 0

        var summary: [String: Any] = [
            "version": 1,
            "generatedAt": graph.meta.generatedAt,
            "stale": false,
            "collapsed": [
                "metric": "todayCost",
                "label": formatCompactCost(todayCost),
                "state": todayTokens > 0 ? "normal" : "idle",
            ],
            "today": [
                "date": todayDate,
                "costUsd": todayCost,
                "tokens": todayTokens,
                "messages": todayMessages,
            ],
            "totals": [
                "costUsd": graph.summary.totalCost,
                "tokens": graph.summary.totalTokens,
                "activeDays": graph.summary.activeDays,
                "clients": graph.summary.clients,
                "models": graph.summary.models.count,
            ],
            "providers": providerBreakdown(graph, todayDate: todayDate),
            "quota": quotaBreakdown(usage),
            "history": historyBreakdown(graph),
            "contribution": contributionBreakdown(graph),
            "top": topBreakdown(graph),
            "health": [
                "summaryPath": summaryPath,
                "lastScanDurationMs": lastScanDurationMs,
                "warnings": [String](),
            ] as [String: Any],
            // accuracy_report_for_graph is Rust-internal and not in graph JSON, so
            // the menu bar shows a static local-scan attribution here.
            "accuracy": [
                "confidence": "high",
                "sourceKinds": ["local-scan"],
                "warnings": [String](),
            ] as [String: Any],
        ]

        if var health = summary["health"] as? [String: Any], let quotaRefreshedAt {
            health["quotaRefreshedAt"] = quotaRefreshedAt
            summary["health"] = health
        }

        if let subagents = subagentBreakdown(graph) {
            summary["subagents"] = subagents
        }

        return try JSONSerialization.data(withJSONObject: summary, options: [])
    }

    /// Refresh only the quota windows of an existing companion summary from a new
    /// `usage` output, leaving the (slow-to-scan) usage data untouched. Returns nil
    /// when the new quota is empty — the caller keeps the previous quota instead of
    /// blanking it to "No live", so a transient fetch failure doesn't wipe the badge.
    public static func patchedQuota(
        companionData: Data,
        usageData: Data,
        quotaRefreshedAt: String
    ) -> Data? {
        guard var dict = (try? JSONSerialization.jsonObject(with: companionData)) as? [String: Any],
            let usage = try? JSONDecoder().decode([UsageProvider].self, from: usageData)
        else {
            return nil
        }
        let quota = quotaBreakdown(usage)
        guard !quota.isEmpty else { return nil }
        dict["quota"] = quota
        if var health = dict["health"] as? [String: Any] {
            health["quotaRefreshedAt"] = quotaRefreshedAt
            dict["health"] = health
        }
        return try? JSONSerialization.data(withJSONObject: dict, options: [])
    }

    // MARK: - breakdowns (mirror companion_summary.rs from_graph)

    private static func providerBreakdown(_ graph: Graph, todayDate: String) -> [[String: Any]] {
        struct Acc {
            var cost = 0.0
            var tokens: Int64 = 0
            var messages = 0
            var todayCost = 0.0
            var todayTokens: Int64 = 0
            var todayMessages = 0
            var modelCosts: [String: Double] = [:]
        }
        var providers: [String: Acc] = [:]
        for day in graph.contributions {
            let isToday = day.date == todayDate
            for client in day.clients {
                var acc = providers[client.client] ?? Acc()
                let tokenCount = client.tokens.total
                acc.cost += client.cost
                acc.tokens += tokenCount
                acc.messages += client.messages
                acc.modelCosts[client.modelId, default: 0] += client.cost
                if isToday {
                    acc.todayCost += client.cost
                    acc.todayTokens += tokenCount
                    acc.todayMessages += client.messages
                }
                providers[client.client] = acc
            }
        }
        return providers
            .map { client, acc -> [String: Any] in
                let topModel = acc.modelCosts.max { $0.value < $1.value }?.key
                var row: [String: Any] = [
                    "client": client,
                    "costUsd": acc.cost,
                    "tokens": acc.tokens,
                    "messages": acc.messages,
                    "todayCostUsd": acc.todayCost,
                    "todayTokens": acc.todayTokens,
                    "todayMessages": acc.todayMessages,
                ]
                if let topModel { row["topModel"] = topModel }
                return row
            }
            .sorted { lhs, rhs in
                let lc = lhs["costUsd"] as? Double ?? 0
                let rc = rhs["costUsd"] as? Double ?? 0
                if lc == rc {
                    return (lhs["client"] as? String ?? "") < (rhs["client"] as? String ?? "")
                }
                return lc > rc
            }
    }

    private static func quotaBreakdown(_ usage: [UsageProvider]) -> [[String: Any]] {
        usage
            .compactMap { output -> [String: Any]? in
                let windows: [[String: Any]] = output.metrics.map { metric in
                    var window: [String: Any] = [
                        "label": metric.label,
                        "usedPercent": metric.usedPercent.clamped(0, 100),
                        "remainingPercent": metric.remainingPercent.clamped(0, 100),
                    ]
                    if let remainingLabel = metric.remainingLabel {
                        window["remainingLabel"] = remainingLabel
                    }
                    if let resetsAt = metric.resetsAt {
                        window["resetsAt"] = resetsAt
                    }
                    return window
                }
                if windows.isEmpty { return nil }
                var provider: [String: Any] = [
                    "provider": output.provider,
                    "windows": windows,
                ]
                if let plan = output.plan { provider["plan"] = plan }
                return provider
            }
            .sorted { ($0["provider"] as? String ?? "") < ($1["provider"] as? String ?? "") }
    }

    private static func historyBreakdown(_ graph: Graph) -> [[String: Any]] {
        graph.contributions
            .sorted { $0.date < $1.date }
            .map { day in
                [
                    "date": day.date,
                    "costUsd": day.totals.cost,
                    "tokens": day.totals.tokens,
                    "messages": day.totals.messages,
                ]
            }
    }

    private static func contributionBreakdown(_ graph: Graph) -> [[String: Any]] {
        graph.contributions
            .sorted { $0.date < $1.date }
            .map { day in
                [
                    "date": day.date,
                    "costUsd": day.totals.cost,
                    "intensity": day.intensity,
                ]
            }
    }

    private static func subagentBreakdown(_ graph: Graph) -> [String: Any]? {
        guard let s = graph.subagents else { return nil }
        if s.totalTokens == 0 && s.agents.isEmpty { return nil }
        let total = max(graph.summary.totalTokens, 1)
        let top = s.agents.prefix(6).map { agent in
            [
                "name": agent.name,
                "tokens": agent.tokens,
                "sessions": agent.sessions,
                "invocations": agent.invocations,
            ] as [String: Any]
        }
        return [
            "sessions": s.sessionCount,
            "invocations": s.invocationCount,
            "tokens": s.totalTokens,
            "messages": s.totalMessages,
            "share": Double(s.totalTokens) / Double(total),
            "top": top,
        ]
    }

    private static func topBreakdown(_ graph: Graph) -> [String: Any] {
        var clientCosts: [String: Double] = [:]
        var modelCosts: [String: Double] = [:]
        for day in graph.contributions {
            for client in day.clients {
                clientCosts[client.client, default: 0] += client.cost
                modelCosts[client.modelId, default: 0] += client.cost
            }
        }
        var top: [String: Any] = [:]
        if let client = clientCosts.max(by: { $0.value < $1.value })?.key {
            top["client"] = client
        }
        if let model = modelCosts.max(by: { $0.value < $1.value })?.key {
            top["model"] = model
        }
        return top
    }

    private static func formatCompactCost(_ cost: Double) -> String {
        if cost >= 1000 {
            return String(format: "$%.1fK", cost / 1000)
        } else if cost >= 100 {
            return String(format: "$%.0f", cost)
        }
        return String(format: "$%.2f", cost)
    }
}

private extension Double {
    func clamped(_ lo: Double, _ hi: Double) -> Double {
        Swift.min(Swift.max(self, lo), hi)
    }
}
