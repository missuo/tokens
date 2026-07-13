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
        /// Opt-in per-client active work time for today (ms), from `graph --work-time`.
        let todayWorkTime: [String: Int64]?

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

        struct Contribution: Codable {
            let date: String
            let intensity: Int
            let totals: Totals
            let clients: [Client]

            struct Totals: Codable {
                let tokens: Int64
                let cost: Double
                let messages: Int
            }

            struct Client: Codable {
                let client: String
                let modelId: String
                let cost: Double
                let messages: Int
                let tokens: Tokens

                struct Tokens: Codable {
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

    private struct RemoteProfile: Decodable {
        let stats: Stats
        let clients: [String]
        let models: [String]
        let contributions: [Contribution]
        let dateRange: DateRange?

        struct Stats: Decodable {
            let totalTokens: Int64
            let totalCost: Double
            let activeDays: Int
        }

        struct DateRange: Decodable {
            let start: String?
            let end: String?
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
                let modelId: String?
                let models: [String: Model]?
                let tokens: Tokens
                let cost: Double
                let messages: Int

                struct Model: Decodable {
                    let cost: Double
                    let input: Int64
                    let output: Int64
                    let messages: Int
                    let cacheRead: Int64
                    let reasoning: Int64
                    let cacheWrite: Int64
                }

                struct Tokens: Decodable {
                    let input: Int64
                    let output: Int64
                    let cacheRead: Int64
                    let cacheWrite: Int64
                    let reasoning: Int64
                }
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

        return try companionJSON(
            graph: graph,
            usage: usage,
            todayDate: todayDate,
            summaryPath: summaryPath,
            lastScanDurationMs: lastScanDurationMs,
            quotaRefreshedAt: quotaRefreshedAt
        )
    }

    public static func isValidGraphData(_ data: Data) -> Bool {
        (try? JSONDecoder().decode(Graph.self, from: data)) != nil
    }

    private static func companionJSON(
        graph: Graph,
        usage: [UsageProvider],
        todayDate: String,
        summaryPath: String,
        lastScanDurationMs: Int,
        quotaRefreshedAt: String?
    ) throws -> Data {
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
                "historyGeneratedAt": graph.meta.generatedAt,
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
        summary["localDaily"] = encodedContributions(graph.contributions)
        summary["historyProvenance"] = [
            "localCapturedAt": graph.meta.generatedAt,
            "localTodayDate": todayDate,
            "mergeRule": "local-full-scan",
            "localSnapshotComplete": true,
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

    /// Replace the expensive local history scan with the compact daily aggregates
    /// already stored by tokens.ci. The existing summary remains the offline cache,
    /// while its local today snapshot only replaces the device-local presentation.
    public static func patchRemoteProfile(
        companionData: Data,
        profileData: Data,
        todayDate: String,
        syncedAt: String,
        lastScanDurationMs: Int
    ) -> Data? {
        guard
            let profile = try? JSONDecoder().decode(RemoteProfile.self, from: profileData),
            var dict = (try? JSONSerialization.jsonObject(with: companionData)) as? [String: Any]
        else { return nil }
        guard profileTotalsAreConsistent(profile) else { return nil }

        let localPresentation = dict
        let localProvenance =
            localPresentation["historyProvenance"] as? [String: Any] ?? [:]
        let localTodayDate = localProvenance["localTodayDate"] as? String
        let graph = graph(from: profile, generatedAt: syncedAt)
        guard
            let remoteData = try? companionJSON(
                graph: graph,
                usage: [],
                todayDate: todayDate,
                summaryPath: "",
                lastScanDurationMs: lastScanDurationMs,
                quotaRefreshedAt: nil
            ),
            let remote = (try? JSONSerialization.jsonObject(with: remoteData)) as? [String: Any]
        else { return nil }

        for key in ["today", "collapsed", "totals", "providers", "history", "contribution", "top"] {
            dict[key] = remote[key]
        }
        let cachedTodayDate =
            (localPresentation["today"] as? [String: Any])?["date"] as? String
        let hasLocalToday = localTodayDate == todayDate && cachedTodayDate == todayDate
        if hasLocalToday {
            patchTodayPresentation(
                in: &dict,
                replacement: localPresentation,
                updateGeneratedAt: false
            )
        }
        dict["remoteDaily"] = encodedContributions(graph.contributions)
        var historyProvenance: [String: Any] = [
            "localCapturedAt":
                localProvenance["localCapturedAt"]
                ?? dict["generatedAt"]
                ?? syncedAt,
            "remoteSyncedAt": syncedAt,
            "mergeRule": "remote-aggregates-local-today-presentation",
        ]
        if let localTodayDate {
            historyProvenance["localTodayDate"] = localTodayDate
        }
        dict["historyProvenance"] = historyProvenance
        if var health = dict["health"] as? [String: Any] {
            health["historyGeneratedAt"] = syncedAt
            health["lastScanDurationMs"] = lastScanDurationMs
            health["historySource"] = "remote-profile"
            dict["health"] = health
        }
        var sourceKinds = ["tokens-ci"]
        if !profileWindowCoversAllTime(profile) {
            sourceKinds.append("tokens-ci-rolling-breakdown")
        }
        if hasLocalToday { sourceKinds.append("local-today") }
        dict["accuracy"] = [
            "confidence": "high",
            "sourceKinds": sourceKinds,
            "warnings": [String](),
        ] as [String: Any]
        dict.removeValue(forKey: "subagents")

        return try? JSONSerialization.data(withJSONObject: dict, options: [])
    }

    private static func profileTotalsAreConsistent(_ profile: RemoteProfile) -> Bool {
        let tokens = profile.contributions.reduce(Int64(0)) { $0 + $1.totals.tokens }
        let cost = profile.contributions.reduce(0.0) { $0 + $1.totals.cost }
        let activeDays = profile.contributions.filter { $0.totals.tokens > 0 }.count
        let costTolerance = max(0.01, abs(profile.stats.totalCost) * 0.000001)
        guard profile.stats.totalTokens >= 0, profile.stats.totalCost.isFinite,
            profile.stats.totalCost >= 0, profile.stats.activeDays >= 0,
            profile.contributions.allSatisfy({
                $0.totals.tokens >= 0 && $0.totals.cost.isFinite && $0.totals.cost >= 0
            }),
            Set(profile.contributions.map(\.date)).count == profile.contributions.count,
            tokens <= profile.stats.totalTokens,
            cost <= profile.stats.totalCost + costTolerance,
            activeDays <= profile.stats.activeDays
        else { return false }

        if profileWindowCoversAllTime(profile) {
            return tokens == profile.stats.totalTokens
                && abs(cost - profile.stats.totalCost) <= costTolerance
                && activeDays == profile.stats.activeDays
        }
        return true
    }

    private static func profileWindowCoversAllTime(_ profile: RemoteProfile) -> Bool {
        if profile.stats.totalTokens == 0, profile.contributions.isEmpty {
            return true
        }
        guard let allTimeStart = profile.dateRange?.start,
            let firstContribution = profile.contributions.map(\.date).min()
        else { return false }
        return allTimeStart == firstContribution
    }

    /// Patch today's figures from a fast today-only graph scan. Remote account
    /// aggregates remain untouched; a local-only cache reconciles its aggregates.
    public static func patchTodayData(
        companionData: Data,
        todayGraphData: Data,
        todayDate: String
    ) -> Data? {
        guard var dict = (try? JSONSerialization.jsonObject(with: companionData)) as? [String: Any]
        else { return nil }
        guard
            let todayData = try? companionJSON(
                graphData: todayGraphData,
                usageData: nil,
                todayDate: todayDate,
                summaryPath: "",
                lastScanDurationMs: 0,
                quotaRefreshedAt: nil
            ),
            let todayDict = (try? JSONSerialization.jsonObject(with: todayData)) as? [String: Any]
        else { return nil }

        var localByDate = Dictionary(
            uniqueKeysWithValues: decodedContributions(dict["localDaily"]).map { ($0.date, $0) }
        )
        localByDate.removeValue(forKey: todayDate)
        for contribution in decodedContributions(todayDict["localDaily"])
        where contribution.date == todayDate {
            localByDate[contribution.date] = contribution
        }
        dict["localDaily"] = encodedContributions(Array(localByDate.values))
        var provenance = dict["historyProvenance"] as? [String: Any] ?? [:]
        provenance["localCapturedAt"] = todayDict["generatedAt"]
        provenance["localTodayDate"] = todayDate
        let usesRemoteHistory =
            (dict["health"] as? [String: Any])?["historySource"] as? String
            == "remote-profile"
        provenance["mergeRule"] =
            usesRemoteHistory
            ? "remote-aggregates-local-today-presentation"
            : "local-full-scan"
        dict["historyProvenance"] = provenance

        if usesRemoteHistory {
            patchTodayPresentation(
                in: &dict,
                replacement: todayDict,
                updateGeneratedAt: true
            )
        } else {
            let previousToday = dict["today"] as? [String: Any]
            reconcileToday(
                in: &dict,
                previousToday: previousToday,
                replacement: todayDict,
                todayDate: todayDate,
                updateGeneratedAt: true
            )
        }

        return try? JSONSerialization.data(withJSONObject: dict, options: [])
    }

    private static func graph(from profile: RemoteProfile, generatedAt: String) -> Graph {
        let contributions = profile.contributions.map { day in
            let clients = day.clients.flatMap { client -> [Graph.Contribution.Client] in
                if let models = client.models, !models.isEmpty {
                    return models.map { modelId, model in
                        Graph.Contribution.Client(
                            client: client.client,
                            modelId: modelId,
                            cost: model.cost,
                            messages: model.messages,
                            tokens: Graph.Contribution.Client.Tokens(
                                input: model.input,
                                output: model.output,
                                cacheRead: model.cacheRead,
                                cacheWrite: model.cacheWrite,
                                reasoning: model.reasoning
                            )
                        )
                    }
                }
                return [
                    Graph.Contribution.Client(
                        client: client.client,
                        modelId: client.modelId.flatMap { $0.isEmpty ? nil : $0 } ?? "unknown",
                        cost: client.cost,
                        messages: client.messages,
                        tokens: Graph.Contribution.Client.Tokens(
                            input: client.tokens.input,
                            output: client.tokens.output,
                            cacheRead: client.tokens.cacheRead,
                            cacheWrite: client.tokens.cacheWrite,
                            reasoning: client.tokens.reasoning
                        )
                    )
                ]
            }
            return Graph.Contribution(
                date: day.date,
                intensity: day.intensity,
                totals: Graph.Contribution.Totals(
                    tokens: day.totals.tokens,
                    cost: day.totals.cost,
                    messages: day.totals.messages
                ),
                clients: clients
            )
        }
        return Graph(
            meta: Graph.Meta(generatedAt: generatedAt),
            summary: Graph.Summary(
                totalTokens: profile.stats.totalTokens,
                totalCost: profile.stats.totalCost,
                activeDays: profile.stats.activeDays,
                clients: profile.clients,
                models: profile.models
            ),
            contributions: contributions,
            subagents: nil,
            todayWorkTime: nil
        )
    }

    private static func encodedContributions(
        _ contributions: [Graph.Contribution]
    ) -> [[String: Any]] {
        guard let data = try? JSONEncoder().encode(contributions),
            let rows = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]]
        else { return [] }
        return rows
    }

    private static func decodedContributions(_ value: Any?) -> [Graph.Contribution] {
        guard let value, JSONSerialization.isValidJSONObject(value),
            let data = try? JSONSerialization.data(withJSONObject: value),
            let contributions = try? JSONDecoder().decode([Graph.Contribution].self, from: data)
        else { return [] }
        return contributions
    }

    private static func reconcileToday(
        in dict: inout [String: Any],
        previousToday: [String: Any]?,
        replacement: [String: Any],
        todayDate: String,
        updateGeneratedAt: Bool
    ) {
        guard let nextToday = replacement["today"] as? [String: Any] else { return }
        let prior = previousToday ?? [:]
        let replacesSameDay = prior["date"] as? String == todayDate

        if var totals = dict["totals"] as? [String: Any] {
            totals["costUsd"] = max(
                0,
                doubleValue(totals["costUsd"])
                    - (replacesSameDay ? doubleValue(prior["costUsd"]) : 0)
                    + doubleValue(nextToday["costUsd"])
            )
            totals["tokens"] = max(
                0,
                int64Value(totals["tokens"])
                    - (replacesSameDay ? int64Value(prior["tokens"]) : 0)
                    + int64Value(nextToday["tokens"])
            )
            var activeDays = intValue(totals["activeDays"])
            if replacesSameDay, int64Value(prior["tokens"]) > 0 { activeDays -= 1 }
            if int64Value(nextToday["tokens"]) > 0 { activeDays += 1 }
            totals["activeDays"] = max(0, activeDays)
            dict["totals"] = totals
        }

        var history = (dict["history"] as? [[String: Any]] ?? [])
            .filter { ($0["date"] as? String) != todayDate }
        if int64Value(nextToday["tokens"]) > 0 || doubleValue(nextToday["costUsd"]) > 0 {
            history.append([
                "date": todayDate,
                "costUsd": doubleValue(nextToday["costUsd"]),
                "tokens": int64Value(nextToday["tokens"]),
                "messages": intValue(nextToday["messages"]),
            ])
        }
        dict["history"] = history.sorted {
            ($0["date"] as? String ?? "") < ($1["date"] as? String ?? "")
        }

        var contributions = (dict["contribution"] as? [[String: Any]] ?? [])
            .filter { ($0["date"] as? String) != todayDate }
        if int64Value(nextToday["tokens"]) > 0 || doubleValue(nextToday["costUsd"]) > 0 {
            contributions.append([
                "date": todayDate,
                "costUsd": doubleValue(nextToday["costUsd"]),
                "intensity": 0,
            ])
        }
        let maxCost = contributions.map { doubleValue($0["costUsd"]) }.max() ?? 0
        dict["contribution"] = contributions.map { day -> [String: Any] in
            var day = day
            let cost = doubleValue(day["costUsd"])
            day["intensity"] = contributionIntensity(cost: cost, maxCost: maxCost)
            return day
        }.sorted {
            ($0["date"] as? String ?? "") < ($1["date"] as? String ?? "")
        }

        if let today = replacement["today"] { dict["today"] = today }
        if let collapsed = replacement["collapsed"] { dict["collapsed"] = collapsed }
        if updateGeneratedAt, let generatedAt = replacement["generatedAt"] {
            dict["generatedAt"] = generatedAt
        }
        patchProviders(
            in: &dict,
            replacement: replacement,
            reconcileLifetime: true,
            replacesSameDay: replacesSameDay
        )
        refreshAggregateLabels(in: &dict)
    }

    private static func patchTodayPresentation(
        in dict: inout [String: Any],
        replacement: [String: Any],
        updateGeneratedAt: Bool
    ) {
        if let today = replacement["today"] { dict["today"] = today }
        if let collapsed = replacement["collapsed"] { dict["collapsed"] = collapsed }
        if updateGeneratedAt, let generatedAt = replacement["generatedAt"] {
            dict["generatedAt"] = generatedAt
        }
        patchProviders(
            in: &dict,
            replacement: replacement,
            reconcileLifetime: false,
            preserveLifetime: true
        )
    }

    private static func patchProviders(
        in dict: inout [String: Any],
        replacement: [String: Any],
        reconcileLifetime: Bool,
        replacesSameDay: Bool = false,
        preserveLifetime: Bool = false
    ) {
        let nextProviders = replacement["providers"] as? [[String: Any]] ?? []
        let nextByClient = Dictionary(
            uniqueKeysWithValues: nextProviders.compactMap { provider -> (String, [String: Any])? in
                guard let client = provider["client"] as? String else { return nil }
                return (client, provider)
            }
        )
        var knownClients = Set<String>()
        var providers = (dict["providers"] as? [[String: Any]] ?? []).map {
            provider -> [String: Any] in
            var provider = provider
            let client = provider["client"] as? String ?? ""
            knownClients.insert(client)
            let next = nextByClient[client]
            if reconcileLifetime {
                provider["costUsd"] = max(
                    0,
                    doubleValue(provider["costUsd"])
                        - (replacesSameDay ? doubleValue(provider["todayCostUsd"]) : 0)
                        + doubleValue(next?["todayCostUsd"])
                )
                provider["tokens"] = max(
                    0,
                    int64Value(provider["tokens"])
                        - (replacesSameDay ? int64Value(provider["todayTokens"]) : 0)
                        + int64Value(next?["todayTokens"])
                )
                provider["messages"] = max(
                    0,
                    intValue(provider["messages"])
                        - (replacesSameDay ? intValue(provider["todayMessages"]) : 0)
                        + intValue(next?["todayMessages"])
                )
            }
            provider["todayCostUsd"] = doubleValue(next?["todayCostUsd"])
            provider["todayTokens"] = int64Value(next?["todayTokens"])
            provider["todayMessages"] = intValue(next?["todayMessages"])
            provider["workTimeMs"] = int64Value(next?["workTimeMs"])
            provider["models"] = patchedModels(
                provider["models"] as? [[String: Any]] ?? [],
                replacement: next?["models"] as? [[String: Any]] ?? [],
                reconcileLifetime: reconcileLifetime,
                replacesSameDay: replacesSameDay,
                preserveLifetime: preserveLifetime
            )
            if let topModel = (provider["models"] as? [[String: Any]])?.max(by: {
                doubleValue($0["costUsd"]) < doubleValue($1["costUsd"])
            })?["model"] {
                provider["topModel"] = topModel
            }
            return provider
        }

        for next in nextProviders {
            guard let client = next["client"] as? String, !knownClients.contains(client) else {
                continue
            }
            providers.append(preserveLifetime ? presentationOnlyProvider(next) : next)
        }
        dict["providers"] = providers.filter {
            int64Value($0["tokens"]) > 0 || doubleValue($0["costUsd"]) > 0
                || intValue($0["messages"]) > 0
                || int64Value($0["todayTokens"]) > 0
                || doubleValue($0["todayCostUsd"]) > 0
                || intValue($0["todayMessages"]) > 0
        }.sorted {
            let lhs = doubleValue($0["costUsd"])
            let rhs = doubleValue($1["costUsd"])
            if lhs == rhs {
                return ($0["client"] as? String ?? "") < ($1["client"] as? String ?? "")
            }
            return lhs > rhs
        }
    }

    private static func patchedModels(
        _ models: [[String: Any]],
        replacement: [[String: Any]],
        reconcileLifetime: Bool,
        replacesSameDay: Bool,
        preserveLifetime: Bool
    ) -> [[String: Any]] {
        let nextByModel = Dictionary(
            uniqueKeysWithValues: replacement.compactMap { model -> (String, [String: Any])? in
                guard let id = model["model"] as? String else { return nil }
                return (id, model)
            }
        )
        var known = Set<String>()
        var result = models.map { model -> [String: Any] in
            var model = model
            let id = model["model"] as? String ?? ""
            known.insert(id)
            let next = nextByModel[id]
            if reconcileLifetime {
                model["costUsd"] = max(
                    0,
                    doubleValue(model["costUsd"])
                        - (replacesSameDay ? doubleValue(model["todayCostUsd"]) : 0)
                        + doubleValue(next?["todayCostUsd"])
                )
                model["tokens"] = max(
                    0,
                    int64Value(model["tokens"])
                        - (replacesSameDay ? int64Value(model["todayTokens"]) : 0)
                        + int64Value(next?["todayTokens"])
                )
                model["messages"] = max(
                    0,
                    intValue(model["messages"])
                        - (replacesSameDay ? intValue(model["todayMessages"]) : 0)
                        + intValue(next?["todayMessages"])
                )
            }
            model["todayCostUsd"] = doubleValue(next?["todayCostUsd"])
            model["todayTokens"] = int64Value(next?["todayTokens"])
            model["todayMessages"] = intValue(next?["todayMessages"])
            return model
        }
        let newModels = replacement.filter { model in
                guard let id = model["model"] as? String else { return false }
                return !known.contains(id)
            }
        result.append(
            contentsOf: preserveLifetime
                ? newModels.map(presentationOnlyModel)
                : newModels
        )
        return result.filter {
            int64Value($0["tokens"]) > 0 || doubleValue($0["costUsd"]) > 0
                || intValue($0["messages"]) > 0
                || int64Value($0["todayTokens"]) > 0
                || doubleValue($0["todayCostUsd"]) > 0
                || intValue($0["todayMessages"]) > 0
        }.sorted {
            let lhs = doubleValue($0["costUsd"])
            let rhs = doubleValue($1["costUsd"])
            if lhs == rhs {
                return ($0["model"] as? String ?? "") < ($1["model"] as? String ?? "")
            }
            return lhs > rhs
        }
    }

    private static func presentationOnlyProvider(_ provider: [String: Any]) -> [String: Any] {
        var provider = provider
        provider["costUsd"] = 0.0
        provider["tokens"] = Int64(0)
        provider["messages"] = 0
        provider["models"] = (provider["models"] as? [[String: Any]] ?? [])
            .map(presentationOnlyModel)
        return provider
    }

    private static func presentationOnlyModel(_ model: [String: Any]) -> [String: Any] {
        var model = model
        model["costUsd"] = 0.0
        model["tokens"] = Int64(0)
        model["messages"] = 0
        return model
    }

    private static func refreshAggregateLabels(in dict: inout [String: Any]) {
        let providers = dict["providers"] as? [[String: Any]] ?? []
        if var totals = dict["totals"] as? [String: Any] {
            totals["clients"] = providers.compactMap { $0["client"] as? String }.sorted()
            let models = providers.flatMap { provider in
                (provider["models"] as? [[String: Any]] ?? []).compactMap {
                    $0["model"] as? String
                }
            }
            totals["models"] = Set(models).count
            dict["totals"] = totals
        }
        if let topProvider = providers.max(by: {
            doubleValue($0["costUsd"]) < doubleValue($1["costUsd"])
        }) {
            var top: [String: Any] = [:]
            if let client = topProvider["client"] { top["client"] = client }
            if let model = topProvider["topModel"] { top["model"] = model }
            dict["top"] = top
        } else {
            dict["top"] = [String: Any]()
        }
    }

    private static func contributionIntensity(cost: Double, maxCost: Double) -> Int {
        guard cost > 0, maxCost > 0 else { return 0 }
        if cost <= maxCost * 0.25 { return 1 }
        if cost <= maxCost * 0.5 { return 2 }
        if cost <= maxCost * 0.75 { return 3 }
        return 4
    }

    private static func doubleValue(_ value: Any?) -> Double {
        (value as? NSNumber)?.doubleValue ?? value as? Double ?? 0
    }

    private static func int64Value(_ value: Any?) -> Int64 {
        (value as? NSNumber)?.int64Value ?? value as? Int64 ?? 0
    }

    private static func intValue(_ value: Any?) -> Int {
        (value as? NSNumber)?.intValue ?? value as? Int ?? 0
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
            var cacheRead: Int64 = 0
            var freshInput: Int64 = 0
            var cacheWrite: Int64 = 0
            var models: [String: ModelAcc] = [:]
        }
        struct ModelAcc {
            var cost = 0.0
            var tokens: Int64 = 0
            var messages = 0
            var todayCost = 0.0
            var todayTokens: Int64 = 0
            var todayMessages = 0
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
                acc.cacheRead += client.tokens.cacheRead
                acc.freshInput += client.tokens.input
                acc.cacheWrite += client.tokens.cacheWrite
                var model = acc.models[client.modelId] ?? ModelAcc()
                model.cost += client.cost
                model.tokens += tokenCount
                model.messages += client.messages
                if isToday {
                    acc.todayCost += client.cost
                    acc.todayTokens += tokenCount
                    acc.todayMessages += client.messages
                    model.todayCost += client.cost
                    model.todayTokens += tokenCount
                    model.todayMessages += client.messages
                }
                acc.models[client.modelId] = model
                providers[client.client] = acc
            }
        }
        return
            providers
            .map { client, acc -> [String: Any] in
                let models = acc.models
                    .map { model, totals -> [String: Any] in
                        [
                            "model": model,
                            "costUsd": totals.cost,
                            "tokens": totals.tokens,
                            "messages": totals.messages,
                            "todayCostUsd": totals.todayCost,
                            "todayTokens": totals.todayTokens,
                            "todayMessages": totals.todayMessages,
                        ]
                    }
                    .sorted { lhs, rhs in
                        let lc = lhs["costUsd"] as? Double ?? 0
                        let rc = rhs["costUsd"] as? Double ?? 0
                        if lc == rc {
                            return (lhs["model"] as? String ?? "") < (rhs["model"] as? String ?? "")
                        }
                        return lc > rc
                    }
                // Prompt-cache hit rate: cache reads (hits) over everything first read
                // this turn — fresh input plus first-time cache writes (both misses).
                // Counting cacheWrite is what keeps Claude off a permanent ~100%.
                let readTotal = acc.cacheRead + acc.freshInput + acc.cacheWrite
                let cacheHitPercent =
                    readTotal > 0
                    ? Double(acc.cacheRead) / Double(readTotal) * 100
                    : 0
                var row: [String: Any] = [
                    "client": client,
                    "costUsd": acc.cost,
                    "tokens": acc.tokens,
                    "messages": acc.messages,
                    "todayCostUsd": acc.todayCost,
                    "todayTokens": acc.todayTokens,
                    "todayMessages": acc.todayMessages,
                    "cacheHitPercent": cacheHitPercent,
                    "workTimeMs": graph.todayWorkTime?[client] ?? 0,
                ]
                row["models"] = models
                let topModel = models.first?["model"] as? String
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
        var modelCostsByClient: [String: [String: Double]] = [:]
        for day in graph.contributions {
            for client in day.clients {
                clientCosts[client.client, default: 0] += client.cost
                modelCostsByClient[client.client, default: [:]][client.modelId, default: 0] +=
                    client.cost
            }
        }
        var top: [String: Any] = [:]
        if let client = clientCosts.max(by: { $0.value < $1.value })?.key {
            top["client"] = client
            if let model = modelCostsByClient[client]?.max(by: { $0.value < $1.value })?.key {
                top["model"] = model
            }
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

extension Double {
    fileprivate func clamped(_ lo: Double, _ hi: Double) -> Double {
        Swift.min(Swift.max(self, lo), hi)
    }
}

public final class SummaryMutationCoordinator: @unchecked Sendable {
    private let queue: DispatchQueue
    private let fileManager: FileManager

    public init(
        label: String = "tokens.menubar.summary-mutations",
        fileManager: FileManager = .default
    ) {
        queue = DispatchQueue(label: label, qos: .utility)
        self.fileManager = fileManager
    }

    public func mutate(
        summaryURL: URL,
        patch: @escaping @Sendable (Data) -> Data?
    ) -> Bool {
        queue.sync {
            guard let latest = try? Data(contentsOf: summaryURL), !latest.isEmpty,
                let patched = patch(latest)
            else { return false }
            return write(patched, to: summaryURL)
        }
    }

    public func replace(summaryURL: URL, with data: Data) -> Bool {
        queue.sync { write(data, to: summaryURL) }
    }

    private func write(_ data: Data, to url: URL) -> Bool {
        var temporaryURL: URL?
        do {
            try fileManager.createDirectory(
                at: url.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            let nextTemporaryURL = url.deletingLastPathComponent().appendingPathComponent(
                ".\(url.lastPathComponent).\(UUID().uuidString).tmp"
            )
            temporaryURL = nextTemporaryURL
            try data.write(to: nextTemporaryURL, options: .atomic)
            if fileManager.fileExists(atPath: url.path) {
                _ = try fileManager.replaceItemAt(url, withItemAt: nextTemporaryURL)
            } else {
                try fileManager.moveItem(at: nextTemporaryURL, to: url)
            }
            return true
        } catch {
            if let temporaryURL {
                try? fileManager.removeItem(at: temporaryURL)
            }
            return false
        }
    }
}
