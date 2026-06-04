# Provider-First Companion Performance Slice Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the next menu bar companion slice: provider switching, larger provider-first UI, reduced primary buttons, Settings tab, and a faster SwiftUI/AppKit render path.

**Architecture:** Keep the Rust companion summary contract mostly stable for this slice and build richer provider-facing view models in Swift from the existing `providers`, `quota`, and `history` fields. The menu bar app should create one hosting controller at startup and update an observable state object instead of replacing the whole SwiftUI root view on every reload.

**Tech Stack:** SwiftUI, AppKit `NSPopover`, Swift Package tests, existing Rust `companion-summary.json` schema.

---

## Scope

This plan implements Phase 1 through Phase 3 from `docs/superpowers/specs/2026-06-04-provider-first-companion-performance-design.md` as a shippable UI/performance slice. It does not implement full cross-provider incremental session parsing. Deep incremental scan work remains a later Rust/core plan.

## File Structure

- Modify: `packages/menubar/Sources/TokscaleMenuBarCore/TokscaleSummary.swift`
  - Add provider-focused view model helpers: selected provider focus, quota windows by provider, work-time fallback display values backed by existing fields, and settings display values.
- Modify: `packages/menubar/Tests/TokscaleMenuBarCoreTests/TokscaleSummaryTests.swift`
  - Add tests for provider focus, provider quota filtering, and unavailable quota behavior.
- Modify: `packages/menubar/Sources/TokscaleMenuBar/main.swift`
  - Add a stable observable app state and stop recreating `NSHostingController` during normal reloads.
- Modify: `packages/menubar/Sources/TokscaleMenuBar/TokensPopoverView.swift`
  - Enlarge the popover, add provider chips, route hero/detail panels through selected provider, replace utility dock with Settings tab, and merge Reload/Scan into one refresh action.

No new CSS files. No raw session parsing in the menu bar app. No credential display.

## Task 1: Add Provider Focus Model

**Files:**
- Modify: `packages/menubar/Sources/TokscaleMenuBarCore/TokscaleSummary.swift`
- Modify: `packages/menubar/Tests/TokscaleMenuBarCoreTests/TokscaleSummaryTests.swift`

- [ ] **Step 1: Add failing tests for selected provider focus**

Add this test to `TokscaleSummaryTests`:

```swift
func testDashboardModelBuildsSelectedProviderFocus() throws {
    let summary = try JSONDecoder().decode(TokscaleSummary.self, from: sampleSummaryJSON)
    let dashboard = TokscaleDashboardModel(summary: summary)

    let claude = dashboard.providerFocus(for: "claude")
    XCTAssertEqual(claude.id, "claude")
    XCTAssertEqual(claude.title, "Claude")
    XCTAssertEqual(claude.topModel, "claude-sonnet-4.5")
    XCTAssertEqual(claude.today, "$4.20 today")
    XCTAssertEqual(claude.quotaWindows.map(\.title), ["Session", "Weekly"])
    XCTAssertEqual(claude.primaryQuota?.title, "Session")

    let gemini = dashboard.providerFocus(for: "gemini")
    XCTAssertEqual(gemini.id, "gemini")
    XCTAssertEqual(gemini.title, "Gemini")
    XCTAssertNil(gemini.primaryQuota)
    XCTAssertEqual(gemini.quotaStatus, "No official quota")
}
```

If `sampleSummaryJSON` does not currently include Gemini and Claude quota rows, extend the sample JSON in the same test file with provider rows for `claude`, `codex`, and `gemini`, and quota rows for `Claude` and `Codex` only.

- [ ] **Step 2: Run the test and verify it fails**

Run:

```bash
swift test --filter TokscaleSummaryTests/testDashboardModelBuildsSelectedProviderFocus
```

Expected: fail because `providerFocus(for:)`, `ProviderFocus`, `primaryQuota`, and `quotaStatus` do not exist.

- [ ] **Step 3: Implement provider focus helpers**

Add this nested struct and method to `TokscaleDashboardModel`:

```swift
public func providerFocus(for id: String?) -> ProviderFocus {
    let details = providerDetails(for: id)
    let normalized = details.id.lowercased()
    let quota = quotaWindows.filter { $0.provider.lowercased() == details.title.lowercased() || $0.provider.lowercased() == normalized }
    let primary = quota.first { $0.title.lowercased() == "session" } ?? quota.first
    let weekly = quota.first { $0.title.lowercased() == "weekly" }

    return ProviderFocus(
        id: details.id,
        title: details.title,
        topModel: details.model,
        today: details.today,
        total: details.total,
        tokens: details.tokens,
        messages: details.messages,
        share: details.share,
        quotaWindows: quota,
        primaryQuota: primary,
        weeklyQuota: weekly,
        quotaStatus: quota.isEmpty ? "No official quota" : "Quota fresh",
        workTime: "Work time unavailable",
        focusedModelTime: focusedModelTimeLabel(providerId: details.id, model: details.model)
    )
}

private static func focusedModelTimeLabel(providerId: String, model: String) -> String {
    if providerId.lowercased() == "claude", model.lowercased().contains("sonnet") {
        return "Sonnet-only unavailable"
    }
    return "Model time unavailable"
}

public struct ProviderFocus: Equatable {
    public let id: String
    public let title: String
    public let topModel: String
    public let today: String
    public let total: String
    public let tokens: String
    public let messages: String
    public let share: Double
    public let quotaWindows: [QuotaWindowSummary]
    public let primaryQuota: QuotaWindowSummary?
    public let weeklyQuota: QuotaWindowSummary?
    public let quotaStatus: String
    public let workTime: String
    public let focusedModelTime: String
}
```

If Swift requires the helper to be static because it is called from `init`, keep `focusedModelTimeLabel` static and call it as `Self.focusedModelTimeLabel(...)`.

- [ ] **Step 4: Run focused and full Swift tests**

Run:

```bash
swift test --filter TokscaleSummaryTests/testDashboardModelBuildsSelectedProviderFocus
swift test
```

Expected: focused test passes, then all menubar tests pass.

- [ ] **Step 5: Commit**

```bash
git add packages/menubar/Sources/TokscaleMenuBarCore/TokscaleSummary.swift packages/menubar/Tests/TokscaleMenuBarCoreTests/TokscaleSummaryTests.swift
git commit -m "feat(companion): add provider focus model"
```

## Task 2: Stabilize Menu Bar App State

**Files:**
- Modify: `packages/menubar/Sources/TokscaleMenuBar/main.swift`
- Modify: `packages/menubar/Sources/TokscaleMenuBar/TokensPopoverView.swift`

- [ ] **Step 1: Add stable observable state**

In `main.swift`, add this class above `MenuBarController`:

```swift
@MainActor
final class TokensMenuBarState: ObservableObject {
    @Published var summary: TokscaleSummary?
    @Published var errorMessage: String?
    @Published var isRefreshing = false
    @Published var refreshStatus: String?
}
```

- [ ] **Step 2: Replace stored primitive render state**

In `MenuBarController`, add:

```swift
private let viewState = TokensMenuBarState()
private var hostingController: NSHostingController<TokensPopoverView>?
private let popoverContentSize = NSSize(width: 500, height: 580)
```

Remove `currentSummary`, `currentError`, `isRefreshing`, and `refreshStatus` stored properties. Keep the same user-facing behavior by writing to `viewState`.

- [ ] **Step 3: Create the hosting controller once**

In `applicationDidFinishLaunching`, after the status item is configured, create the hosting controller once:

```swift
let controller = NSHostingController(
    rootView: TokensPopoverView(
        state: viewState,
        onReload: { [weak self] in self?.reload() },
        onRefreshScan: { [weak self] in self?.refreshScan() },
        onOpenTokensCI: { [weak self] in self?.openTokensCI() },
        onRevealCache: { [weak self] in self?.revealCache() },
        onQuit: { [weak self] in self?.quit() }
    )
)
controller.sizingOptions = []
controller.view.frame = NSRect(origin: .zero, size: popoverContentSize)
hostingController = controller
popover.contentViewController = controller
```

- [ ] **Step 4: Make render update state only**

Replace `render()` with:

```swift
private func render() {
    statusItem?.button?.title = viewState.summary?.menuBarTitle ?? "AI Tokens"
    popover.contentSize = popoverContentSize
    hostingController?.view.frame = NSRect(origin: .zero, size: popoverContentSize)
}
```

In `reload()`, set `viewState.summary` and `viewState.errorMessage` before calling `render()`. In `refreshScan()`, set `viewState.isRefreshing` and `viewState.refreshStatus`.

- [ ] **Step 5: Update `TokensPopoverView` initializer**

Change the root view from value props to:

```swift
struct TokensPopoverView: View {
    @ObservedObject var state: TokensMenuBarState
    let onReload: () -> Void
    let onRefreshScan: () -> Void
    let onOpenTokensCI: () -> Void
    let onRevealCache: () -> Void
    let onQuit: () -> Void
}
```

Then replace direct property reads:

- `summary` -> `state.summary`
- `errorMessage` -> `state.errorMessage`
- `isRefreshing` -> `state.isRefreshing`
- `refreshStatus` -> `state.refreshStatus`

- [ ] **Step 6: Run Swift tests and build**

Run:

```bash
swift test
swift build
```

Expected: tests and build pass. The app no longer recreates `NSHostingController` on every normal reload.

- [ ] **Step 7: Commit**

```bash
git add packages/menubar/Sources/TokscaleMenuBar/main.swift packages/menubar/Sources/TokscaleMenuBar/TokensPopoverView.swift
git commit -m "perf(companion): keep menu bar host stable"
```

## Task 3: Provider-First Popover UI

**Files:**
- Modify: `packages/menubar/Sources/TokscaleMenuBar/TokensPopoverView.swift`

- [ ] **Step 1: Enlarge root frame**

Update the root `.frame` from `420 x 460` to `500 x 580`.

- [ ] **Step 2: Add selected provider state**

In `SummaryContent`, add:

```swift
@State private var selectedProviderId: String?

private var selectedFocus: TokscaleDashboardModel.ProviderFocus {
    model.providerFocus(for: selectedProviderId)
}
```

Add `syncSelectedProvider()`:

```swift
private func syncSelectedProvider() {
    if let selectedProviderId, model.providers.contains(where: { $0.id == selectedProviderId }) {
        return
    }
    selectedProviderId = model.providers.first?.id
}
```

Call it from `onAppear` and `onChange(of: model.providers)`.

- [ ] **Step 3: Add provider chips**

Create `ProviderChipRow`:

```swift
private struct ProviderChipRow: View {
    let providers: [TokscaleDashboardModel.ProviderSummary]
    let selectedProviderId: String?
    let onSelect: (String) -> Void

    var body: some View {
        HStack(spacing: 7) {
            ForEach(providers.prefix(5), id: \.id) { provider in
                Button {
                    onSelect(provider.id)
                } label: {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(provider.label)
                            .font(.system(size: 11, weight: .semibold))
                        Text(provider.value)
                            .font(.system(size: 10, weight: .medium))
                            .monospacedDigit()
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(9)
                    .background(
                        RoundedRectangle(cornerRadius: 12, style: .continuous)
                            .fill(providerColor(provider.id).opacity(provider.id == selectedProviderId ? 0.18 : 0.08))
                    )
                    .overlay(
                        RoundedRectangle(cornerRadius: 12, style: .continuous)
                            .stroke(providerColor(provider.id).opacity(provider.id == selectedProviderId ? 0.55 : 0.16), lineWidth: 1)
                    )
                }
                .buttonStyle(.plain)
                .help(provider.label)
            }
        }
    }
}
```

Place this row between `CompanionHeader` and the hero.

- [ ] **Step 4: Route hero through provider focus**

Change `FocusHeroCard` to accept `focus: TokscaleDashboardModel.ProviderFocus`. Its hero title should prefer `focus.primaryQuota?.value`, then `focus.today`. The subtitle should prefer quota detail/reset, then `focus.topModel`. The weekly mini metric should use `focus.weeklyQuota?.value`, falling back to `focus.messages`.

- [ ] **Step 5: Replace panel enum labels**

Change `CompanionPanel` cases to:

```swift
case overview = "Overview"
case limits = "Limits"
case history = "History"
case settings = "Settings"
```

Map icons to `chart.pie`, `gauge.with.dots.needle.67percent`, `chart.bar`, and `gearshape`.

- [ ] **Step 6: Replace detail panes**

Update `DynamicDetailPane` to accept `focus` and render:

- `OverviewPane(summary:model:focus:)`: today cost, tokens/messages, work time, focused model time.
- `LimitsPane(focus:)`: quota windows for selected provider, or no-official-quota message.
- `HistoryPane(model:)`: existing 7-day chart.
- `SettingsPane(...)`: refresh, open web, cache, quit, title/provider settings rows.

Settings may show non-interactive rows for future title/provider-order controls in this slice, but the rows must be visually placed under Settings rather than the main dashboard.

- [ ] **Step 7: Remove primary ActionDock**

Delete the bottom `ActionDock` from the main VStack. Keep the action button implementation only if reused inside Settings. The main UI should expose only the header refresh icon and Settings tab.

- [ ] **Step 8: Run Swift build/test**

Run:

```bash
swift test
swift build
```

Expected: all tests pass and the menu bar app builds.

- [ ] **Step 9: Commit**

```bash
git add packages/menubar/Sources/TokscaleMenuBar/TokensPopoverView.swift
git commit -m "feat(companion): add provider-first popover"
```

## Task 4: Local Verification

**Files:**
- No source changes expected.

- [ ] **Step 1: Build the `.app` bundle**

Run:

```bash
bash packages/menubar/scripts/build-app.sh
```

Expected: prints `packages/menubar/.build/TokscaleMenuBar.app`.

- [ ] **Step 2: Restart the local app**

Run:

```bash
pgrep -fl tokens-menubar
kill <pid>
open packages/menubar/.build/TokscaleMenuBar.app
```

Expected: the menu bar item returns with the cached label.

- [ ] **Step 3: Manual checks**

Open the popover and verify:

- provider chips switch the hero immediately;
- Claude shows session and weekly quota when cache has quota data;
- Gemini does not show fake quota;
- Settings contains Cache and diagnostics-related actions;
- the main dashboard no longer shows separate Reload, Scan, Web, Cache, Quit dock buttons;
- popover feels larger and does not clip the quota rows;
- repeated Reload/Refresh does not visually reset the whole popover tree.

- [ ] **Step 4: Commit any verification-only code fixes**

If manual verification exposes a real code issue, fix it in the smallest relevant file and rerun:

```bash
swift test
swift build
```

Then commit with a concrete message such as:

```bash
git commit -m "fix(companion): preserve selected provider on refresh"
```

## Task 5: Push

**Files:**
- No source changes expected.

- [ ] **Step 1: Check status and identity**

Run:

```bash
git status --short --branch
git config user.name
git config user.email
```

Expected identity for Bonny's branch:

```text
Bonny07
111042029+Bonny07@users.noreply.github.com
```

- [ ] **Step 2: Push branch**

Run:

```bash
git push origin codex/accuracy-layer-v0
```

Expected: branch updates on `origin/codex/accuracy-layer-v0`.

## Deferred Work

These are intentionally not in this slice:

- full Rust deep-scan versus fast-refresh mode split,
- per-provider quota TTL configuration UI,
- true Sonnet-only active-time computation if the summary does not yet expose model-level active time,
- Gemini official quota fetch,
- file watcher or launchd cadence changes,
- mobile/web cockpit reuse.
