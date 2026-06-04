# Provider-First Companion And Performance Design

## Goal

Turn the menu bar companion into a provider-first dashboard that feels closer to ClaudeBar for quota visibility, but avoids ClaudeBar's slow reads, visible friction, and high idle cost. The default open path must be fast: read cached summary data, render immediately, then refresh slower provider/session data in the background.

## Selected Direction

Use the Provider-first Dashboard direction selected from the visual mockup. The expanded popover centers on one selected AI provider at a time: Claude, Codex, Gemini, OpenClaw, and any future supported clients. Each provider gets its own theme color, quota/status cards, model/work-time breakdown, and history rows. The all-AI total remains available, but it is not the primary hero.

## User Experience

The menu bar collapsed state stays compact. It should show one user-selected label such as `AI $789`, `Claude 55%`, `Codex 46%`, or `5h 55%`. It must not grow into a long dashboard in the menu bar.

The expanded popover should move from the current narrow utility panel to a larger dashboard, around `500 x 580` points. It should still feel like a menu bar surface, not a full app window.

Primary controls:

- Provider chips: `Claude`, `Codex`, `Gemini`, `OpenClaw`, plus `All` when useful.
- Main tabs: `Overview`, `Limits`, `History`, `Settings`.
- Main actions: one refresh icon and one settings icon.

Utility actions move into Settings:

- cache reveal,
- diagnostics,
- provider order,
- menu title format,
- auto-refresh cadence,
- deep scan,
- auth status,
- data freshness and cache path.

`Reload` and `Scan` should not be exposed as separate primary buttons. The primary refresh action performs a fast refresh by default. Deep scan is a settings action because it can be slower and more power intensive.

## Provider Cards

Claude should expose:

- 5-hour/session quota,
- weekly quota,
- today's local usage,
- active work time,
- Sonnet-only usage or time when model data is available,
- latest active model/session,
- quota freshness.

Codex should expose the same shape where possible:

- 5-hour/session quota if provider API data exists,
- weekly quota if provider API data exists,
- today's local usage,
- active work time,
- top model,
- cache reuse and reasoning tokens when available,
- quota freshness.

Gemini should expose the same shape where possible:

- today's local usage,
- active work time,
- top model,
- cache/thought/tool token split when available,
- quota or account limits only if an official or reliable provider source is implemented.

Providers without official quota data should not fake quota. They show local usage and clearly mark limits as unavailable or cached.

## Data Freshness Model

The companion summary should split data into independently fresh modules:

- `localUsage`: local session-derived cost, tokens, messages, active time, model mix, and history.
- `providerQuota`: official provider quota/limit windows, fetched through provider APIs when credentials exist.
- `uiSettings`: title format, provider order, selected provider, refresh policy, and visible modules.
- `health`: cache age, latest scan duration, errors, and whether data came from fast cache, incremental scan, deep scan, or provider API.

Each module needs its own `generatedAt`, `expiresAt` or TTL, and `state` such as `fresh`, `stale`, `refreshing`, `needsAuth`, or `failed`.

The menu bar app must render with stale-but-valid data instead of blocking on a refresh.

## Performance Architecture

The companion must separate fast UI reads from expensive work.

Fast path:

- opening the menu bar reads `companion-summary.json` and settings only;
- no session parsing on open;
- no network request on open;
- no full `NSHostingController` recreation for normal reloads;
- provider selection and tab changes are pure SwiftUI state changes.

Background path:

- a fast refresh checks freshness and only updates stale modules;
- quota fetches run per provider with independent TTLs;
- local usage refresh should use existing source/message cache and fingerprints;
- a deep scan is explicit or low-frequency scheduled maintenance;
- weekly or daily reconcile can validate all source files without making normal usage slow.

Energy rules:

- idle menu bar CPU should stay effectively zero;
- idle memory should not grow with session history size;
- full scan memory belongs to short-lived CLI child processes, not the long-lived menu bar app;
- no high-frequency polling of raw session folders;
- avoid launching browser/login flows unless the user opens Settings or starts auth.

## Incremental Session Strategy

Existing foundations should be reused before adding new systems:

- `message_cache` already fingerprints source files and stores parsed messages.
- Codex already has an incremental parse path for appended JSONL lines.
- aggregate cache can support recomputing affected dates instead of rebuilding all history.
- usage quota cache already stores provider subscription results with a short TTL.

The next implementation should productize this for companion usage:

- keep local usage cache displayable when provider quota APIs are slow;
- use fingerprints to skip unchanged Claude, Codex, Gemini, and OpenClaw sources;
- for append-only sources, parse only appended data when the parser supports it;
- write scan duration and cache-hit statistics into the companion health object;
- add a deep-scan mode that ignores incremental shortcuts for validation.

## Settings

Settings should be compact but useful:

- menu title metric: today cost, selected provider quota, all-AI total, tokens, work time;
- provider order and default provider;
- auto-refresh cadence for local usage and provider quota;
- show/hide modules such as work time, Sonnet-only, history, cache stats;
- auth status per provider;
- advanced section for cache path, diagnostics, and deep scan.

Settings should not expose raw credentials or raw session contents.

## Privacy And Safety

The menu bar summary remains metadata-only. It may include provider/client names, model IDs, token counts, costs, timestamps, device name, and derived active-time statistics. It must not store raw prompts, completions, credentials, or full file contents.

Provider quota fetchers must read credentials through existing safe paths only. If credentials are missing or stale, the provider card shows `needs auth` and links to Settings instead of opening auth unexpectedly.

## Phased Implementation

### Phase 1: Data Shape And Provider Selection

Extend the companion summary and Swift model so provider cards can show per-provider local usage, quota windows, freshness, top model, and active time fields. Add stable selected-provider state in the menu bar UI.

### Phase 2: Larger Provider-First UI

Resize the popover, add provider chips, route hero/details through the selected provider, move utility buttons into Settings, and apply provider theme colors consistently.

### Phase 3: Performance Fast Path

Stop recreating the hosting controller on normal reloads. Make open/read paths cache-only. Add per-module freshness display and separate fast refresh from deep scan.

### Phase 4: Incremental And Low-Power Refresh

Wire companion refresh to existing message/fingerprint caches and Codex incremental parsing where available. Record cache hit/miss stats and scan duration. Keep deep scan as explicit validation.

### Phase 5: Provider Enhancements

Add or improve provider-specific quota/status modules only when the source is reliable. Claude and Codex quota are first-class. Gemini should show local usage first and only show quota after a reliable official source exists.

## Non-Goals

- Do not make the menu bar parse raw sessions directly.
- Do not block popover opening on network requests.
- Do not show fake quota for providers without official quota data.
- Do not put cache paths or diagnostics on the main dashboard.
- Do not make the collapsed menu bar label large.
- Do not add mobile behavior in this milestone.

## Success Criteria

- The popover opens from cache without a visible wait.
- Provider chips switch the hero and detail content immediately.
- Claude shows session quota, weekly quota, today usage, work time, and Sonnet-only data when available.
- Codex and Gemini use the same UI structure without pretending unavailable quota exists.
- The main dashboard has fewer primary buttons than the current version.
- Cache/diagnostics/deep scan are available from Settings.
- Normal refresh does not rescan unchanged session files.
- Deep scan remains available for validation and troubleshooting.
