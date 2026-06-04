# Menu Bar Companion Design

## Goal

Make tokens useful without forcing users to open tokens.ci. The first desktop surface should be a tiny menu bar companion that shows one glanceable usage signal while collapsed, then expands into a focused local cockpit for cost, token, project, model, device, and service health views.

## Current Baseline

The CLI can scan local client data, submit usage to tokens.ci, report service status, and expose machine-readable state through JSON commands. Release `v3.0.3` also moved scheduled submissions into short-lived child processes, which keeps the long-running service small while scans run separately.

The remaining product gap is presentation. Users have data, but the default workflow still asks them to open a website or read CLI output. Competitors prove that always-visible status works, but they also show the failure modes: high idle resource use, missing background reliability, unclear source accuracy, and cramped dashboards that do not explain why usage changed.

## Product Principles

- The collapsed state must stay small. It should fit into the menu bar as one number plus a tiny state indicator, not a dashboard.
- Click expands into detail. Every secondary metric belongs in the popover or full cockpit, not in the collapsed menu bar label.
- Users can switch the collapsed metric. The default is today cost, with alternatives for today tokens, budget percent, burn rate, or current session cost.
- The UI must not rescan raw sessions. It reads compact local status/cache files written by CLI submit/status paths.
- Local-first by default. The menu bar should work without opening tokens.ci and without storing raw prompts, completions, credentials, or chat content.
- Resource use is a feature. Idle CPU should be effectively zero, and idle memory should be bounded by the UI shell rather than by the size of the user's session history.

## Recommended Approach

Build a macOS menu bar companion first, backed by the existing CLI JSON/status surfaces and future compact cache files. Treat tokens.ci and mobile as downstream consumers of the same summarized data model.

Two alternatives were considered:

- Web dashboard first: easiest to ship with the existing site, but it does not solve the "I have to open a webpage" problem.
- Mobile app first: useful for summaries and alerts, but mobile cannot read local AI session files directly, so it depends on desktop submission first.
- Menu bar first: recommended, because it attacks the daily user pain directly and creates a reusable local data contract for web and mobile later.

## Collapsed Menu Bar State

The collapsed label is intentionally tiny:

```text
$1.24
```

Optional variants:

```text
42%
18M
```

Rules:

- Width target: one compact metric, usually 4 to 8 visible characters.
- No inline charts, project names, model names, or long status text in the collapsed label.
- A subtle state indicator can encode health: normal, stale, syncing, warning, or failed.
- The collapsed metric is user-selectable from the expanded popover.
- The label updates from cached summary data on a slow cadence, not from raw session scans.
- If data is stale, keep the label small and show the explanation only after click.

## Expanded Popover

Clicking the menu bar item opens a compact popover with switchable views:

- `Today`: cost, tokens, budget progress, last submit time, burn rate, and projected month total.
- `Session`: current or latest session cost, model mix, client, and time since last activity.
- `Projects`: top projects or directories by cost/tokens for today and 7 days.
- `Models`: model breakdown with pricing confidence and unpriced model warnings.
- `Devices`: local device name, submit status, last server acceptance, and multi-device freshness.
- `Health`: service state, scan duration, peak scan memory from latest run if available, stale cache warnings, and recommended fix actions.

The popover should use segmented controls or tabs, not stacked cards. The main interaction is switching views quickly, then drilling into a full local cockpit only when needed.

## Full Local Cockpit

The full cockpit is a secondary surface launched from the popover, not the default first screen. It can be a local web UI or native window.

Recommended sections:

- Timeline: hourly usage heatmap and daily trend.
- Breakdown: client, project, model, device, and pricing source.
- Explain: why today's cost changed, which model or project drove the spike, and whether the number is local-estimated or provider-official.
- Reliability: submit history, failed submissions, service drift, and cache freshness.
- Privacy: what is stored locally, what is submitted, and what is never collected.

## Data Contract

The companion should read one compact local summary, not raw session files:

```json
{
  "version": 1,
  "generatedAt": "2026-06-04T00:00:00Z",
  "stale": false,
  "collapsed": {
    "metric": "todayCost",
    "label": "$1.24",
    "state": "normal"
  },
  "today": {
    "costUsd": 1.24,
    "tokens": 18000000,
    "budgetPercent": 42,
    "projectedMonthCostUsd": 38.5
  },
  "latestSession": {
    "client": "codex",
    "project": "tokens",
    "costUsd": 0.18,
    "tokens": 2400000
  },
  "health": {
    "serviceRunning": true,
    "lastSubmitAt": "2026-06-04T00:00:00Z",
    "lastScanDurationMs": 1800,
    "lastScanPeakRssBytes": 560000000,
    "warnings": []
  }
}
```

Rules:

- Raw prompts, completions, and credentials never enter this summary.
- The summary is written by CLI/background paths after submit, status refresh, or explicit cache refresh.
- The menu bar app treats stale data as displayable but marked.
- The menu bar app can trigger a CLI refresh, but it must not parse every session itself.
- The same summary can feed Raycast, a native app, local cockpit, and future mobile sync.

## Resource Budget

The product should define resource budgets as acceptance criteria:

- Collapsed idle CPU: effectively 0%.
- Collapsed idle memory: target below 40 MB for a native menu bar app; if a JS shell is used, it must still stay small enough to beat heavyweight competitors.
- No full scan on every UI open.
- No high-frequency polling. Use file watch or a slow refresh cadence for compact summary files.
- Full-history scan remains manual, weekly reconcile, or explicit repair, not the normal menu bar refresh path.
- Large session histories must affect scan child-process peak memory, not long-lived UI memory.

## Accuracy And Trust

Every visible total should be explainable:

- Show source: local scan, submitted server data, provider official, estimated pricing, or custom pricing.
- Show confidence only when useful: high, medium, low, stale, or unpriced.
- Explain differences from ClaudeBar, Tokscale, provider dashboards, and tokens.ci by source, pricing table, device coverage, and time window.
- Avoid claiming billing-grade accuracy when the number is local-estimated.

## Mobile Direction

Mobile should come after the desktop summary contract is stable. Its role is not real-time local scanning; it is a companion for:

- budget alerts,
- daily and weekly digests,
- leaderboard/social views,
- device freshness,
- spike explanations,
- remote view of submitted summaries.

The mobile app should consume server-submitted summaries and never require access to local desktop session files.

## Phases

### Phase 1: Local Summary Contract

Add or formalize a compact local summary generated by CLI/status paths. Include collapsed label data, today totals, latest session, health, and accuracy summary.

### Phase 2: Menu Bar Prototype

Ship the smallest usable menu bar companion. Collapsed state shows one metric; click opens the tabbed popover; settings allow switching collapsed metric and alert thresholds.

### Phase 3: Local Cockpit

Add the full local cockpit for timeline, breakdown, health, and explainability. This can start as a local web UI launched from the popover if that is faster than native UI.

### Phase 4: tokens.ci Personal Cockpit

Use submitted summaries to improve the web profile beyond leaderboard data. Add personal trends, device freshness, and source confidence.

### Phase 5: Mobile Companion

Build mobile notifications and summaries after the server has enough submitted data to make the app useful without local scans.

## Non-Goals

- Do not build a heavyweight Electron dashboard as the first default surface.
- Do not put charts or long labels in the menu bar collapsed state.
- Do not make the UI parse raw sessions.
- Do not require tokens.ci to be open for daily usage awareness.
- Do not submit raw prompts, completions, credentials, or chat content.
- Do not solve provider-official billing reconciliation in the first menu bar milestone.

## Success Criteria

- The menu bar collapsed state stays visually small and useful at a glance.
- Clicking expands into switchable views without opening a browser.
- A user can see today cost, latest session, top project/model, device freshness, and service health in under two clicks.
- The long-running UI does not retain memory proportional to session history size.
- Heavy scans happen in CLI child processes, scheduled reconcile, or explicit refresh flows.
- The same summary model can later support Raycast, tokens.ci, and mobile without rethinking privacy or accuracy.
