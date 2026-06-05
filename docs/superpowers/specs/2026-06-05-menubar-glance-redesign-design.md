# Menu bar glance redesign — quota decision aid

Status: superseded in part — see Update below
Date: 2026-06-05

> **Update 2026-06-05 (direction change).** The glance-only trim described below was implemented but rejected on review as too thin ("我们那么多功能怎么就剩这点"). Final direction: keep the full quota/spend/history dashboard — restored and adapted onto the new SwiftUI `MenuBarExtra` entry, with the dashboard model built once per load (the original per-render rebuild was the real slowness) — and pursue ClaudeBar feature parity plus our own extras.
>
> Shipped on this branch: MenuBarExtra positioning fix (kills the crooked popover); color-coded menu bar badge rendered as a non-template `NSImage` (MenuBarExtra would not render an inline SwiftUI bar), using ClaudeBar's thresholds on remaining quota — `>50` healthy / `20-50` warning / `<20` critical / `0` depleted; cross-provider **Best now** hint; reset countdowns (already present); dollar amounts relabeled as **API-equivalent value** rather than implied subscription spend; the token/message vanity metric demoted.
>
> Deliberately not done: **system notifications** — the pure transition logic lives in `QuotaGlance.alerts` (unit-tested) but the runtime was removed because an unsigned dev build cannot register/deliver notifications and there is no Apple Developer account; the always-visible colored menu bar badge is the permission-free passive alert. **Per-model quota windows** — providers return only Session + Weekly, so there is no per-model data to show. The implementation plan's later phases (glance popover, app-piece split) are superseded by the restore.

## Problem

The companion popover (560×760) crams a full dashboard — quota board + spend cards + 14-day history + week-over-week overlay + insights — into a menu bar popover. Three concrete failures:

1. **Crooked positioning.** `NSPopover` plus a manual `recenterPopoverWindow` fights the system and mispositions on multi-display setups or when other menu extras are present. Repeated "stabilize popover anchor" commits never fully fixed it.
2. **Slow / janky.** `TokscaleDashboardModel` is rebuilt on every SwiftUI body evaluation (`TokensPopoverView` exposes `model` and `accent` as computed properties), compounded by gradients, shadows, and blur over a large surface.
3. **Information overload.** It is a dashboard, not a glance — "worse than ClaudeBar."

## Goal

Beat ClaudeBar **and** tokscale, not just match them. ClaudeBar is Claude-only; tokscale is a website. Neither answers the question that actually matters: **"Can I keep working right now, and if not, what do I switch to?"** Make the menu bar a cross-provider decision aid: glanceable, fast, correctly positioned.

## Non-goals

- Full dashboard / history / insights in the popover — that lives at tokens.ci.
- Changing the CLI or the `companion-summary` data contract. We consume existing cache fields only.
- The already-shipped open-refresh throttle fix and `RefreshCadence` setting stay; this builds on them.

## Design

### Menu bar (folded state)

- Icon + the single **most-constrained** quota percentage across all providers, e.g. `⚡58%`.
- "Most-constrained" = the lowest `remainingPercent` among every live quota window (both 5h and weekly) of every provider.
- Color by remaining headroom on that window: normal tint > `≤20%` orange > `≤10%` red.
- Falls back to icon-only when no live quota is available.

### Popover (glance-only, ~300×220)

```
┌─────────────────────────────┐
│ ● Tokens          ⟳    ⚙   │
├─────────────────────────────┤
│ Claude  5h▓▓▓▓░ wk▓▓░  ⏱2h │  ← most-constrained, highlighted
│ Codex   5h▓▓▓░░ wk▓░░  ⏱5h │
│ Gemini  5h▓▓░░░ wk▓░░  ⏱1d │
├─────────────────────────────┤
│ Today $399 · 7d $590        │
│ Best now → Codex 78%        │
└─────────────────────────────┘
```

- Top bar: live dot + "Tokens" + refresh + settings (gear).
- One row per provider that has live quota, **sorted most-constrained first**; the most-constrained row is color-highlighted. Each row: `5h` bar, weekly bar, reset countdown.
- Footer line 1: `Today $X · 7d $Y`.
- Footer line 2: `Best now → <provider> <remaining%>` — the provider with the most 5h headroom. Hidden when no provider has live quota.
- Settings (gear) panel keeps the existing toolbar (open tokens.ci / reveal cache / quit) and the `RefreshCadence` control.

### The four decision-aid features (the leapfrog)

1. Most-constrained-first menu bar percentage.
2. Reset countdown per window, formatted from the existing `resetsAt` (e.g. `2h`, `5h`, `1d`).
3. Providers sorted by urgency.
4. "Best now" switch hint (provider with the most 5h remaining).

All four are derivable from existing summary data as pure functions, unit-testable in `TokscaleMenuBarCore`.

### Positioning fix (the crooked popover)

Switch the app entry from the hand-rolled AppKit `NSApplicationDelegate` + manual `NSStatusItem` / `NSPopover` to a SwiftUI App lifecycle:

```swift
@main struct TokensMenuBarApp: App {
    var body: some Scene {
        MenuBarExtra { PopoverContent() } label: { MenuBarLabel() }
            .menuBarExtraStyle(.window)
    }
}
```

The system owns positioning, so it is never crooked (ClaudeBar's approach).

**Risk:** CodeX tried `MenuBarExtra` and reverted — in the hand-built `.app` the status item did not register stably under accessibility. Root cause was almost certainly mixing the AppKit delegate with SwiftUI. **Mitigation:** the first implementation step is a throwaway **spike** — build the SwiftPM `.app` with a trivial `MenuBarExtra` and confirm the icon appears, the popover opens, and it positions correctly on a real multi-display setup. If the spike fails, fall back to keeping `NSPopover` and hardening `recenterPopoverWindow` (screen detection via the status button's own window/screen). The spike result gates the rest of the work.

### Performance fix (the slowness)

- Build `TokscaleDashboardModel` (and `accent`) once per summary load and hold it in state, instead of recomputing in a computed property on every render.
- Drop the large blurred `CompanionBackdrop` gradient and multi-layer shadows; the glance surface is small, so heavy effects are unnecessary.
- Smaller view tree: no `ScrollView` wrapping four heavy sections.

## Data

Everything comes from the existing `companion-summary` cache — no CLI change:

- Per-provider quota windows (`label`, `usedPercent`, `remainingPercent`, `resetsAt`) → bars, countdown, most-constrained, best-now.
- Today and 7-day spend → footer.

## Testing

- **Core (pure, TDD-first):** most-constrained selection, best-now (most-remaining) selection, reset-countdown formatting, provider sort order, color-threshold mapping. New `QuotaGlance` helpers in `TokscaleMenuBarCore` with tests.
- Keep existing `TokscaleSummary` and `RefreshCadence` tests green.
- **App lifecycle / MenuBarExtra:** manual click-test by Bonny on her real multi-display setup — automation is unreliable here per the prior handoff.

## What we cut (YAGNI)

14-day history chart, week-over-week overlay, insights panels, overview / limits cards, the large "Quota" title section, the 560×760 surface.

## Open risks

- `MenuBarExtra` viability in a SwiftPM-built `.app` — the spike gates this; manual fallback is defined.
- Color thresholds: default `≤20%` orange, `≤10%` red on remaining; tune after seeing it live.
- "Best now" / countdown hidden gracefully when no live quota exists.
