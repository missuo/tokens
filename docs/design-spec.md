# macOS Menu Bar Token Usage App — Design

**Date:** 2026-07-26  
**Status:** Approved for implementation (product decisions locked; remaining details decided by implementer)  
**Repo:** `HuaileiW/tokens` (fork of `missuo/tokens`; `origin` = fork, `upstream` = missuo)

## 1. Goals and non-goals

### Goals

- macOS Menu Bar app that shows **local** AI coding token usage derived from the same session sources as `tokens-cli`.
- Data path: **Swift shell → `tokens` CLI → `tokens-core`** (no Swift reimplementation of parsers).
- Refresh policy:
  - Scan once on launch
  - Default automatic scan every **12 hours** (user-configurable interval)
  - Manual **Refresh**
  - Settings: **Full rescan** (ignore/rebuild caches)
- Menu bar title is **configurable**: tokens only / cost only / both.
- Dropdown is a **full** local dashboard:
  - Periods: **Today / 7d / 30d / All**
  - Summary: total tokens, cost, messages
  - Token breakdown (input / output / cache read / cache write / reasoning)
  - By client (APP / CLI)
  - By project/workspace (nested models; Unattributed for workspace-less usage)
  - By model (with provider), nested under client where useful
  - By day history + cost/token share bars
  - Optional link out to https://tokens.ci
- **Caching**:
  - Reuse unchanged source transcripts (existing core `SourceMessageCache`)
  - Persist aggregated usage snapshots so period switches and repeat queries avoid redundant full work
  - Every successful `tokens usage` run updates reusable records
- Minimal settings only (see §5).

### Non-goals (v1)

- Not a replacement for `tokens submit`, `serve`, or the public leaderboard.
- Rank / social standing is **not** a primary data source (optional external link only).
- No embedded `tokens-core` static library (A2) in v1.
- No Launch-at-Login, notification thresholds, client filters, custom date range, or theming beyond system appearance.
- No Windows/Linux menu bar.

## 2. Architecture

```
TokensMenuBar (SwiftUI + AppKit NSStatusItem)
        │  Process / pipe
        ▼
tokens usage --json --period <p> [--force-rescan]
        │
        ▼
tokens-core scan + aggregate
  Layer A: SourceMessageCache (existing, fingerprint-based)
  Layer B: Usage snapshot cache (new)
```

### Process boundary

- The app **never** reads client session directories itself.
- Single read API for the UI: `tokens usage --json ...`.
- CLI binary resolution (in order):
  1. `UserDefaults` override path if set later (v1: not exposed in UI; reserved)
  2. `PATH` lookup for `tokens`
  3. Common install locations: Homebrew (`/opt/homebrew/bin/tokens`, `/usr/local/bin/tokens`), `~/.local/bin/tokens`
- If binary missing: show install hint (brew / install.sh) and disable Refresh until found; offer **Recheck**.

### Caching layers

| Layer | Store | Role | Invalidate |
|-------|--------|------|------------|
| **A. Source messages** | Existing `source-message-cache-v2` under tokens cache dir | Skip re-parse of unchanged session files | File fingerprint change; `--force-rescan` |
| **B. Usage snapshot** | `usage-snapshot-v2.json` under tokens cache dir | Aggregated daily/client/project/model rollups for fast period filter | Successful rebuild after scan; `--force-rescan`; schema version bump |
| **C. App UI last response** | App `UserDefaults` / memory | Show stale numbers while a scan runs | Replaced on next successful CLI JSON |

**Default scan path:** incremental Layer A → rebuild Layer B → filter by period → JSON.  
**Force rescan:** clear/ignore A for this run (and clear B) → full parse → rebuild A+B → JSON.

CLI and App both treat CLI output as source of truth; App cache is display-only.

## 3. CLI contract

### Command

```bash
tokens usage [OPTIONS]
```

| Flag | Default | Meaning |
|------|---------|---------|
| `--json` | off (human text if TTY); **App always passes** | Machine-readable output |
| `--period today\|7d\|30d\|all` | `today` | Aggregation window in configured bucket timezone (presets) |
| `--since YYYY-MM-DD` / `--until YYYY-MM-DD` | none | Inclusive custom civil dates; require both and `--contract v3` (cannot combine with `--period`) |
| `--contract v2\|v3` | omitted → v2 for presets; required `v3` for custom dates | External report contract. **The Menu Bar app requests v3.** |
| `--force-rescan` | false | Full rescan; rebuild caches |
| `--refresh` | false | Refresh path for the selected request (v3 uses snapshot reuse rules) |

Human-readable non-JSON mode is optional nicety for terminal users; App only needs JSON.

v2 remains supported for compatibility (preset-only, `schemaVersion: 2`). The app requests v3 for presets and custom ranges.

### JSON schema (`schemaVersion: 2`)

Success (exit 0):

```json
{
  "schemaVersion": 2,
  "generatedAt": "ISO-8601",
  "period": "today",
  "dateRange": { "start": "YYYY-MM-DD", "end": "YYYY-MM-DD" },
  "scan": {
    "mode": "incremental",
    "forceRescan": false,
    "durationMs": 0,
    "cache": {
      "sourceHits": 0,
      "sourceMisses": 0,
      "snapshotRebuilt": true
    }
  },
  "summary": {
    "totalTokens": 0,
    "totalCost": 0.0,
    "messages": 0,
    "activeDays": 0,
    "clients": [],
    "models": []
  },
  "tokenBreakdown": {
    "input": 0,
    "output": 0,
    "cacheRead": 0,
    "cacheWrite": 0,
    "reasoning": 0
  },
  "byClient": [
    {
      "client": "claude-code",
      "tokens": 0,
      "cost": 0.0,
      "messages": 0,
      "share": 0.0,
      "models": [
        {
          "modelId": "...",
          "providerId": "...",
          "tokens": 0,
          "cost": 0.0,
          "messages": 0,
          "share": 0.0
        }
      ]
    }
  ],
  "byProject": [
    {
      "projectKey": "/stable/workspace/key",
      "displayName": "workspace-name",
      "tokens": 0,
      "cost": 0.0,
      "messages": 0,
      "models": [
        {
          "modelId": "...",
          "providerId": "...",
          "tokens": 0,
          "cost": 0.0,
          "messages": 0
        }
      ]
    }
  ],
  "byModel": [
    {
      "modelId": "...",
      "providerId": "...",
      "tokens": 0,
      "cost": 0.0,
      "messages": 0,
      "share": 0.0,
      "clients": ["claude-code"]
    }
  ],
  "byDay": [
    {
      "date": "YYYY-MM-DD",
      "tokens": 0,
      "cost": 0.0,
      "messages": 0,
      "intensity": 0
    }
  ],
  "meta": {
    "cliVersion": "...",
    "timezone": "..."
  }
}
```

**Decision:** `byClient[].models[]` remains available. v2 adds `byProject[]`, keyed by workspace key and sorted by cost descending; each project includes cost-sorted model details. Workspace-less usage is represented once as `projectKey: null`, `displayName: "Unattributed"`.

Error (non-zero exit preferred):

```json
{
  "schemaVersion": 2,
  "error": {
    "code": "invalid_args" | "scan_failed" | "internal",
    "message": "human readable"
  }
}
```

### Period semantics

- Use the same bucket timezone as the rest of the CLI (`tokens` timezone settings / `bucket_tz`).
- `today`: single local bucket day
- `7d` / `30d`: inclusive rolling windows ending today
- `all`: all contributions available after scan

### JSON schema (`schemaVersion: 3`, report contract v3)

The Menu Bar app always requests v3. Success (exit 0):

```json
{
  "schemaVersion": 3,
  "generatedAt": "ISO-8601",
  "selection": { "kind": "preset", "preset": "today" }
                 | { "kind": "custom", "startDate": "YYYY-MM-DD", "endDate": "YYYY-MM-DD" },
  "dateRange": { "startDate": "YYYY-MM-DD", "endDate": "YYYY-MM-DD", "timezone": "..." },
  "scan": {
    "mode": "snapshot|incremental|...",
    "forceRescan": false,
    "durationMs": 0,
    "cache": {
      "sourceHits": 0,
      "sourceMisses": 0,
      "snapshotRebuilt": false,
      "snapshotSchemaVersion": 3
    }
  },
  "summary": {
    "totalTokens": 0,
    "totalCost": 0.0,
    "messages": 0,
    "activeDays": 0,
    "clients": [],
    "models": []
  },
  "tokenBreakdown": { "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 },
  "byClient": [ /* same shape as v2 client rows */ ],
  "byProject": [ /* same shape as v2 project rows */ ],
  "byModel": [ /* same shape as v2 model rows */ ],
  "timeSeries": {
    "granularity": "hour" | "day" | "naturalWeek" | "naturalMonth",
    "selectionStart": "ISO-8601",
    "buckets": [
      {
        "id": "ISO-8601",
        "nominalStart": "ISO-8601",
        "nominalEndExclusive": "ISO-8601",
        "coveredStart": "ISO-8601",
        "coveredEndExclusive": "ISO-8601",
        "totals": { "tokens": 0, "cost": 0.0, "messages": 0 },
        "contextOnly": false,
        "incompleteEdge": false,
        "active": false
      }
    ],
    "unplaced": { "tokens": 0, "cost": 0.0, "messages": 0 }
  },
  "meta": {
    "cliVersion": "...",
    "timezone": "...",
    "reportContract": "v3"
  }
}
```

**v3 rules (locked):**

- Totals (`summary`, breakdowns, byClient/byProject/byModel) cover the selected inclusive range only; `contextOnly` buckets are excluded from totals and may prepend prior-day hours so a single-day chart that is reporting today has at least 12 hourly marks.
- Granularity is automatic from the inclusive day count: 1 → hour; 2–14 → day; 15–90 → natural week (Mon–Sun); >90 → natural month.
- `unplaced` holds usage with a reliable day but no trustworthy hour so the chart never invents a false hour.
- Layer B full-history snapshot: `usage-snapshot-v3.json` (`schemaVersion: 3`) under the tokens cache dir, with per-day totals, client/project rows, hour facts, and `unplacedForHourly`. Range switches reuse the snapshot without a full rescan when facts are still valid.

### Implementation notes (CLI)

- Reuse `tokens-core` scan + `aggregate_by_date` / summary helpers already used by submit path.
- Prefer extending core with a focused `build_usage_report(...)` rather than scraping submit payloads.
- Layer B snapshot: store post-aggregate daily contributions (enough to derive summary, byClient, byProject, byModel, byDay for any period). Invalidate when scan produces new messages or force rescan.
- Each successful real scan also updates local-only `unattributed-sessions-v1.json` in the tokens cache. It upserts by client + session ID and stores session/model token-cost totals, first/last timestamps, and bounded, hashed source identifier samples. It never stores prompts or responses, is owner-readable/writable on Unix, is not returned by `tokens usage --json`, and a diagnostic write failure does not fail usage reporting. The unreleased ledger intentionally remains cumulative for now so diagnostic samples can accumulate. **TODO before release:** define a bounded retention/deletion policy. Residual risks accepted for this iteration: empty/unknown session IDs may collide, and Windows ACL hardening is not yet implemented.
- `--force-rescan`: expose or call a clear/rebuild path on `SourceMessageCache` (add API if missing) plus delete Layer B file.
- Do not upload anything; read-only local command.

## 4. Menu Bar UI

### Status item (always visible)

- Configurable format:
  - `tokens` → e.g. `1.2M`
  - `cost` → e.g. `$4.20`
  - `both` → e.g. `1.2M · $4.20`
- Prefix icon: app mark if space allows; otherwise text only.
- While scanning: keep last value; optional subtle `…` or progress only inside panel (avoid status item flicker).
- Error / missing CLI: `tokens?` or `—` plus panel explanation.

Number formatting:

- Tokens: raw &lt; 1000; else `K` / `M` / `B` with one decimal when needed
- Cost: `$X.XX`; values in `(0, 0.01)` → `<$0.01`

### Dropdown panel sections (top → bottom)

1. **Period control** — segmented: Today | 7d | 30d | All; fixed above the report body
2. **TOTAL** — required report-body section with total tokens, cost, messages, and date range
3. **BREAKDOWN** — required report-body section with input, output, cache, and reasoning token metrics
4. **COST** — compact daily cost bars
5. **CLIENT** — sorted by tokens desc; progress share bar
6. **MODEL** — flat list with provider label; share bar
7. **PROJECT** — sorted by cost desc; each workspace row shows cost + tokens and its models sorted by cost desc. `Unattributed` never exposes workspace keys or diagnostic session details
8. **Optional error banner** — when present, follows PROJECT within the scrolling report content
9. **Fixed footer** — outside the scrolling content; Last updated (`generatedAt`); **Refresh**; **Settings…**; **Open tokens.ci**; **Quit**

The required scrolling report-body order is TOTAL → BREAKDOWN → COST → CLIENT → MODEL → PROJECT. The period control remains above that body, an optional error banner may follow PROJECT inside it, and the footer remains fixed outside the scrolling content.

Period changes: call CLI with new `--period` (Layer B should make this cheap after one warm scan). Prefer not blocking UI: show spinner in panel, keep prior period data until new JSON arrives.

### Settings (minimal)

- Scan interval: presets **1h / 6h / 12h (default) / 24h / Manual only**
- Menu bar display: **Tokens / Cost / Both** (default: Tokens)
- **Full Rescan Now** → `tokens usage --json --force-rescan --period <current>`
- Read-only: resolved CLI path, last error
- No Launch at Login in v1

Settings storage: app `UserDefaults` (interval, display mode). Scan caches live under tokens config/cache dirs owned by CLI.

### UI visual language (2026-07-26)

Locked design: **Minimal Mono v2** (`design/menubar-ui-v1/`, FINAL 06).
Implementation plan: `docs/implementation-plan.md`.

Overrides earlier generic “system appearance only” chrome: mono typography, spacing-only sections, breakdown cards, cost chart (≤14 days), chart hover, nested long-list scroll, restyled settings.

## 5. App project layout

Monorepo addition:

```

  Package.swift                 # or Xcode project
  Sources/TokensMenuBar/
    AppMain.swift
    StatusItemController.swift
    UsageService.swift          # Process wrapper
    Models/UsageReport.swift    # Codable mirrors CLI JSON
    Views/MenuPanelView.swift
    Views/SettingsView.swift
    Formatting.swift
  Tests/TokensMenuBarTests/
  README.md
```

**Decision:** SwiftPM + thin Xcode wrapper if needed for `LSUIElement` menu-bar-only app. Bundle is agent-only (`LSUIElement` = true): no Dock icon.

**Bundle ID:** `ci.tokens.menubar`  
**Display name:** `Tokens`

Minimum macOS: **13.0** (Ventura) unless packaging constraints force 14.

## 6. Error handling

| Situation | Behavior |
|-----------|----------|
| `tokens` not found | Panel: install instructions; status `—`; Refresh = recheck PATH |
| CLI non-zero / invalid JSON | Keep last good data; show error banner with `error.message` |
| Empty usage | Zeros + “No local usage found” |
| Scan timeout | Soft timeout (e.g. 120s) then error; do not kill mid-write if avoidable — prefer letting CLI finish |
| Concurrent refresh | Coalesce: one in-flight process; newer requests wait or replace queue of one |

## 7. Testing

### CLI

- Unit tests for period filtering and JSON shape (fixture contributions → expected `byClient` / `byProject` / `byModel` / `byDay`).
- Test `--force-rescan` clears snapshot (temp HOME / config dir).
- Snapshot schema version mismatch rebuilds cleanly.

### App

- Decode golden JSON fixtures into `UsageReport`.
- Formatting unit tests (tokens/cost/menu title).
- Manual: run against real `tokens usage` on a dev machine.

### Regression

- Ensure `tokens usage` does not break existing submit/status commands.
- `cargo test` for touched crates; Swift tests if package present.

## 8. Implementation phases

1. **CLI `tokens usage --json`** with periods + force-rescan + Layer B snapshot  
2. **Swift Menu Bar MVP**: status title, summary, period, refresh, settings shell, missing-CLI state  
3. **Panel completeness**: byDay / COST, byClient (+ nested models), byModel, byProject / PROJECT (+ nested models), share bars, tokens.ci link
4. **Polish**: interval timer, stale-while-revalidate, formatting edge cases, README  

## 9. Security and privacy

- Same trust model as CLI: all session reads stay local; command does not submit.
- App only executes the resolved `tokens` binary with fixed argv (no shell string concat of user free text beyond validated enums).
- Do not log full JSON to crash reporters in v1 (none integrated).

## 10. Open decisions closed by default

| Topic | Decision |
|-------|----------|
| Nested models under client | **Yes** in v1 |
| Layer B format | Versioned JSON or bincode under tokens cache dir; implementer chooses based on size; prefer JSON first for debuggability |
| Menu bar default display | **Tokens only** |
| Interval default | **12 hours** |
| Rank / API | Out of scope for primary UI |
| Distribution | Dev: build locally; notarized DMG later |
| Command name | **`usage`** |

## 11. Success criteria

- With CLI installed and local sessions present, Menu Bar shows today’s token total within one scan of launch.
- Second scan within unchanged sources is materially faster (Layer A hits) and still refreshes snapshot metadata.
- Full Rescan changes `scan.mode` to full and rebuilds caches.
- Period switches update summary and lists without requiring the user to understand CLI flags.
- No network required for core functionality.
