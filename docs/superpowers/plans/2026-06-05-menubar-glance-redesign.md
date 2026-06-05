# Menu bar glance redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the overloaded companion popover with a fast, correctly-positioned, glance-only cross-provider quota decision aid.

**Architecture:** Add pure `QuotaGlance` helpers in `TokscaleMenuBarCore` (most-constrained, best-now, urgency, reset countdown, recent spend). Move the app entry from a hand-rolled AppKit `NSApplicationDelegate` to a SwiftUI `App` with `MenuBarExtra(.window)` so the system owns positioning. Rebuild the popover as a small glance view and the menu bar label as icon + most-constrained percentage.

**Tech Stack:** Swift 6, SwiftUI, SwiftPM, XCTest. Package root: `packages/menubar`.

**Spec:** `docs/superpowers/specs/2026-06-05-menubar-glance-redesign-design.md`

---

## File Structure

- Create `packages/menubar/Sources/TokscaleMenuBarCore/QuotaGlance.swift` — pure glance derivations.
- Create `packages/menubar/Tests/TokscaleMenuBarCoreTests/QuotaGlanceTests.swift` — tests for the above.
- Modify `packages/menubar/Sources/TokscaleMenuBarCore/TokscaleSummary.swift` — make `QuotaProvider`/`QuotaWindow`/`HistoryDay` public; make `parseISODate` internal.
- Create `packages/menubar/Sources/TokscaleMenuBar/MenuBarModel.swift` — `ObservableObject` owning summary load/refresh/actions (lifted from the old `MenuBarController`).
- Create `packages/menubar/Sources/TokscaleMenuBar/MenuBarLabelView.swift` — icon + most-constrained % with urgency color.
- Replace `packages/menubar/Sources/TokscaleMenuBar/TokensPopoverView.swift` — glance layout (the old one is parked as `_legacy_popover.swift.bak` during the migration).
- Replace `packages/menubar/Sources/TokscaleMenuBar/main.swift` — SwiftUI `App` + `MenuBarExtra` (the old one is parked as `_legacy_main.swift.bak`).

Keep `TokscaleSummary.swift`, `RefreshCadence.swift`, the `TokscaleDashboardModel` + its tests, and the already-shipped throttle/cadence behavior intact.

**Ordering rule:** every task ends on a green `swift build`. The old entry and old popover are parked as `.bak` in Task 0, the new pieces are added as self-contained files that compile while unused (Phase 2), and Phase 3 wires them and replaces the spike entry.

---

## Phase 0 — MenuBarExtra spike (GATES everything)

### Task 0: Prove MenuBarExtra works in this SwiftPM `.app`

**Files:**
- Park: `main.swift` → `_legacy_main.swift.bak`, `TokensPopoverView.swift` → `_legacy_popover.swift.bak`
- Replace: `packages/menubar/Sources/TokscaleMenuBar/main.swift` (temporary minimal entry)

- [ ] **Step 1: Park the old entry + popover, add a minimal MenuBarExtra app**

`main.swift` currently has top-level `app.run()`. A SwiftUI `@main` cannot coexist with top-level code in a file named `main.swift`, so the entry must use `@main` and `main.swift` must contain no top-level statements. The old `TokensPopoverView.swift` references `TokensMenuBarState` defined in the old `main.swift`, so both must leave the build together or the target will not compile. The `.bak` extension keeps files out of the SwiftPM build.

Run:
```bash
git mv packages/menubar/Sources/TokscaleMenuBar/main.swift packages/menubar/Sources/TokscaleMenuBar/_legacy_main.swift.bak
git mv packages/menubar/Sources/TokscaleMenuBar/TokensPopoverView.swift packages/menubar/Sources/TokscaleMenuBar/_legacy_popover.swift.bak
```

Then create a new `main.swift`:

```swift
import SwiftUI

@main
struct TokensMenuBarApp: App {
    var body: some Scene {
        MenuBarExtra {
            Text("Spike OK")
                .padding()
                .frame(width: 220, height: 120)
        } label: {
            Image(systemName: "chart.bar.xaxis")
        }
        .menuBarExtraStyle(.window)
    }
}
```

The app target now contains only the minimal entry; `TokscaleMenuBarCore` is untouched and its tests still pass.

- [ ] **Step 2: Build the `.app`**

Run: `bash packages/menubar/scripts/build-app.sh`
Expected: `Build complete!` and the `.app` path printed.

- [ ] **Step 3: Manual verify (Bonny, real multi-display)**

Run: `open packages/menubar/.build/TokscaleMenuBar.app`
Confirm: the chart icon appears in the menu bar; clicking it opens a small window-style popover that is NOT crooked on the real display arrangement; no Desktop permission prompt.

- [ ] **Step 4: Decision gate**

- PASS → proceed to Phase 1; the `.bak` files are deleted in Task 10.
- FAIL → stop and switch to the fallback: restore `_legacy_main.swift.bak` → `main.swift` and `_legacy_popover.swift.bak` → `TokensPopoverView.swift`, keep `NSPopover`, and harden `recenterPopoverWindow` (detect the screen via `statusItem.button.window.screen`). Re-plan Phases 2–4 against the NSPopover structure.

- [ ] **Step 5: Commit (only if PASS)**

```bash
git add -A packages/menubar/Sources/TokscaleMenuBar/
git commit -m "feat(companion): switch menu bar entry to MenuBarExtra"
```

---

## Phase 1 — Core QuotaGlance helpers (TDD, spike-independent)

### Task 1: Make quota/history types usable and date parser reusable

**Files:**
- Modify: `packages/menubar/Sources/TokscaleMenuBarCore/TokscaleSummary.swift`

- [ ] **Step 1: Make the decode types public and the parser internal**

In `TokscaleSummary.swift`, change `struct QuotaProvider`, `struct QuotaWindow`, and `struct HistoryDay` to `public struct ...` (their members are already `public let`; `@testable import` then exposes the internal memberwise init to tests). Change the free function `private func parseISODate` to `func parseISODate` (internal) so `QuotaGlance.swift` in the same module can reuse it.

- [ ] **Step 2: Build to confirm visibility compiles**

Run: `cd packages/menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 3: Commit**

```bash
git add packages/menubar/Sources/TokscaleMenuBarCore/TokscaleSummary.swift
git commit -m "refactor(companion): expose quota types and date parser in core"
```

### Task 2: `QuotaGlance.recentSpend`

**Files:**
- Create: `packages/menubar/Sources/TokscaleMenuBarCore/QuotaGlance.swift`
- Test: `packages/menubar/Tests/TokscaleMenuBarCoreTests/QuotaGlanceTests.swift`

- [ ] **Step 1: Write the failing test**

```swift
import XCTest

@testable import TokscaleMenuBarCore

final class QuotaGlanceTests: XCTestCase {
    private func day(_ date: String, _ cost: Double) -> TokscaleSummary.HistoryDay {
        TokscaleSummary.HistoryDay(date: date, costUsd: cost, tokens: 0, messages: 0)
    }

    func testRecentSpendSumsLastNDays() {
        let history = [
            day("2026-05-30", 10), day("2026-05-31", 20), day("2026-06-01", 30),
            day("2026-06-02", 40), day("2026-06-03", 50), day("2026-06-04", 60),
            day("2026-06-05", 70), day("2026-06-06", 80),
        ]
        XCTAssertEqual(QuotaGlance.recentSpend(history, days: 7), 350, accuracy: 0.001)
        XCTAssertEqual(QuotaGlance.recentSpend(history, days: 100), 360, accuracy: 0.001)
        XCTAssertEqual(QuotaGlance.recentSpend([], days: 7), 0, accuracy: 0.001)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd packages/menubar && swift test --filter QuotaGlanceTests`
Expected: FAIL to compile — `cannot find 'QuotaGlance' in scope`.

- [ ] **Step 3: Write minimal implementation**

```swift
import Foundation

public enum QuotaGlance {
    public static func recentSpend(_ history: [TokscaleSummary.HistoryDay], days: Int) -> Double {
        history.suffix(days).reduce(0) { $0 + $1.costUsd }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd packages/menubar && swift test --filter QuotaGlanceTests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/menubar/Sources/TokscaleMenuBarCore/QuotaGlance.swift packages/menubar/Tests/TokscaleMenuBarCoreTests/QuotaGlanceTests.swift
git commit -m "feat(companion): add recent spend glance helper"
```

### Task 3: `QuotaGlance.urgency` thresholds

**Files:**
- Modify: `QuotaGlance.swift`, `QuotaGlanceTests.swift`

- [ ] **Step 1: Write the failing test**

```swift
    func testUrgencyThresholds() {
        XCTAssertEqual(QuotaGlance.urgency(remainingPercent: 50), .normal)
        XCTAssertEqual(QuotaGlance.urgency(remainingPercent: 21), .normal)
        XCTAssertEqual(QuotaGlance.urgency(remainingPercent: 20), .warning)
        XCTAssertEqual(QuotaGlance.urgency(remainingPercent: 11), .warning)
        XCTAssertEqual(QuotaGlance.urgency(remainingPercent: 10), .critical)
        XCTAssertEqual(QuotaGlance.urgency(remainingPercent: 0), .critical)
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd packages/menubar && swift test --filter QuotaGlanceTests`
Expected: FAIL — `cannot find 'UrgencyLevel'` / `urgency`.

- [ ] **Step 3: Write minimal implementation**

Add to `QuotaGlance.swift`:

```swift
public enum UrgencyLevel: Equatable {
    case normal
    case warning
    case critical
}

extension QuotaGlance {
    public static func urgency(remainingPercent: Double) -> UrgencyLevel {
        if remainingPercent <= 10 { return .critical }
        if remainingPercent <= 20 { return .warning }
        return .normal
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd packages/menubar && swift test --filter QuotaGlanceTests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A packages/menubar
git commit -m "feat(companion): add quota urgency thresholds"
```

### Task 4: `QuotaGlance.resetCountdown`

**Files:**
- Modify: `QuotaGlance.swift`, `QuotaGlanceTests.swift`

- [ ] **Step 1: Write the failing test**

```swift
    private func iso(_ value: String) throws -> Date {
        try XCTUnwrap(ISO8601DateFormatter().date(from: value))
    }

    func testResetCountdownFormats() throws {
        let now = try iso("2026-06-05T00:00:00Z")
        XCTAssertEqual(QuotaGlance.resetCountdown(from: "2026-06-05T00:30:00Z", now: now), "30m")
        XCTAssertEqual(QuotaGlance.resetCountdown(from: "2026-06-05T02:00:00Z", now: now), "2h")
        XCTAssertEqual(QuotaGlance.resetCountdown(from: "2026-06-06T12:00:00Z", now: now), "1d")
        XCTAssertEqual(QuotaGlance.resetCountdown(from: "2026-06-05T00:00:30Z", now: now), "1m")
        XCTAssertNil(QuotaGlance.resetCountdown(from: "2026-06-04T23:00:00Z", now: now))
        XCTAssertNil(QuotaGlance.resetCountdown(from: nil, now: now))
        XCTAssertNil(QuotaGlance.resetCountdown(from: "not-a-date", now: now))
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd packages/menubar && swift test --filter QuotaGlanceTests`
Expected: FAIL — `cannot find 'resetCountdown'`.

- [ ] **Step 3: Write minimal implementation**

Add to `QuotaGlance.swift`:

```swift
extension QuotaGlance {
    public static func resetCountdown(from resetsAt: String?, now: Date = Date()) -> String? {
        guard let resetsAt, let resetDate = parseISODate(resetsAt) else { return nil }
        let seconds = resetDate.timeIntervalSince(now)
        guard seconds > 0 else { return nil }
        let minutes = Int(seconds / 60)
        if minutes < 60 { return "\(max(minutes, 1))m" }
        let hours = minutes / 60
        if hours < 24 { return "\(hours)h" }
        return "\(hours / 24)d"
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd packages/menubar && swift test --filter QuotaGlanceTests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A packages/menubar
git commit -m "feat(companion): add reset countdown formatting"
```

### Task 5: `mostConstrained`, `bestNow`, `providersByUrgency`

**Files:**
- Modify: `QuotaGlance.swift`, `QuotaGlanceTests.swift`

- [ ] **Step 1: Write the failing test**

```swift
    private func window(_ label: String, remaining: Double, resetsAt: String? = nil)
        -> TokscaleSummary.QuotaWindow
    {
        TokscaleSummary.QuotaWindow(
            label: label,
            usedPercent: 100 - remaining,
            remainingPercent: remaining,
            remainingLabel: nil,
            resetsAt: resetsAt
        )
    }

    private func provider(_ name: String, _ windows: [TokscaleSummary.QuotaWindow])
        -> TokscaleSummary.QuotaProvider
    {
        TokscaleSummary.QuotaProvider(provider: name, plan: nil, windows: windows)
    }

    func testMostConstrainedPicksGlobalLowestRemaining() {
        let providers = [
            provider("Claude", [window("Session", remaining: 28), window("Weekly", remaining: 59)]),
            provider("Codex", [window("Session", remaining: 12), window("Weekly", remaining: 70)]),
        ]
        let result = QuotaGlance.mostConstrained(in: providers)
        XCTAssertEqual(result?.provider, "Codex")
        XCTAssertEqual(result?.remainingPercent, 12)
        XCTAssertNil(QuotaGlance.mostConstrained(in: []))
    }

    func testBestNowPicksProviderWithHighestMinRemaining() {
        let providers = [
            provider("Claude", [window("Session", remaining: 28), window("Weekly", remaining: 59)]),
            provider("Codex", [window("Session", remaining: 12), window("Weekly", remaining: 70)]),
            provider("Gemini", [window("Session", remaining: 80), window("Weekly", remaining: 40)]),
        ]
        let result = QuotaGlance.bestNow(in: providers)
        XCTAssertEqual(result?.provider, "Gemini")
        XCTAssertEqual(result?.remainingPercent, 40)
    }

    func testProvidersByUrgencySortsAndDropsEmpty() {
        let providers = [
            provider("Claude", [window("Session", remaining: 28)]),
            provider("Codex", [window("Session", remaining: 12)]),
            provider("Empty", []),
            provider("Gemini", [window("Session", remaining: 80)]),
        ]
        XCTAssertEqual(QuotaGlance.providersByUrgency(providers), ["Codex", "Claude", "Gemini"])
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd packages/menubar && swift test --filter QuotaGlanceTests`
Expected: FAIL — `cannot find 'mostConstrained'` etc.

- [ ] **Step 3: Write minimal implementation**

Add to `QuotaGlance.swift`:

```swift
extension QuotaGlance {
    public struct GlanceWindow: Equatable {
        public let provider: String
        public let label: String
        public let usedPercent: Double
        public let remainingPercent: Double
        public let resetsAt: String?
    }

    public struct ProviderHeadroom: Equatable {
        public let provider: String
        public let remainingPercent: Double
    }

    public static func mostConstrained(
        in providers: [TokscaleSummary.QuotaProvider]
    ) -> GlanceWindow? {
        var best: GlanceWindow?
        for provider in providers {
            for window in provider.windows {
                let candidate = GlanceWindow(
                    provider: provider.provider,
                    label: window.label,
                    usedPercent: window.usedPercent,
                    remainingPercent: window.remainingPercent,
                    resetsAt: window.resetsAt
                )
                if let current = best, current.remainingPercent <= candidate.remainingPercent {
                    continue
                }
                best = candidate
            }
        }
        return best
    }

    public static func bestNow(
        in providers: [TokscaleSummary.QuotaProvider]
    ) -> ProviderHeadroom? {
        var result: ProviderHeadroom?
        for provider in providers where !provider.windows.isEmpty {
            let headroom = provider.windows.map(\.remainingPercent).min() ?? 0
            if let current = result, current.remainingPercent >= headroom {
                continue
            }
            result = ProviderHeadroom(provider: provider.provider, remainingPercent: headroom)
        }
        return result
    }

    public static func providersByUrgency(
        _ providers: [TokscaleSummary.QuotaProvider]
    ) -> [String] {
        providers
            .filter { !$0.windows.isEmpty }
            .sorted { lhs, rhs in
                (lhs.windows.map(\.remainingPercent).min() ?? 0)
                    < (rhs.windows.map(\.remainingPercent).min() ?? 0)
            }
            .map(\.provider)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd packages/menubar && swift test --filter QuotaGlanceTests`
Expected: PASS.

- [ ] **Step 5: Run the full suite**

Run: `cd packages/menubar && swift test`
Expected: all prior tests still pass (TokscaleSummary, RefreshCadence, QuotaGlance).

- [ ] **Step 6: Commit**

```bash
git add -A packages/menubar
git commit -m "feat(companion): add most-constrained, best-now, urgency sort"
```

---

## Phase 2 — Build the app pieces (each compiles standalone)

Each task adds one self-contained file that builds while still unused. Phase 3 wires them and replaces the spike entry. This keeps every `swift build` green.

### Task 6: `MenuBarModel`

**Files:**
- Create: `packages/menubar/Sources/TokscaleMenuBar/MenuBarModel.swift`

- [ ] **Step 1: Create the model**

An `@MainActor ObservableObject` owning the summary and the actions previously on `MenuBarController`. The subprocess plumbing (`runCompanionCommand`, `companionRefreshCandidates`, `dedupePaths`, `runCompanionRefreshProcess`) is copied verbatim from `_legacy_main.swift.bak` (it is unchanged). All six action methods must be declared here so Task 8's view compiles against them.

```swift
import AppKit
import SwiftUI
import TokscaleMenuBarCore

@MainActor
final class MenuBarModel: ObservableObject {
    @Published var summary: TokscaleSummary?
    @Published var errorMessage: String?
    @Published var isRefreshing = false
    @Published var refreshStatus: String?

    private let store = TokscaleSummaryStore()
    private var refreshTimer: Timer?

    init() {
        reload()
        refreshTimer = Timer.scheduledTimer(withTimeInterval: 60, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.reload() }
        }
    }

    func reload() {
        do {
            summary = try store.load()
            errorMessage = nil
        } catch {
            summary = nil
            errorMessage = error.localizedDescription
        }
    }

    func refreshScan(status: String = "Scanning local AI sessions...") {
        runRefresh(status: status, arguments: ["--no-spinner", "companion-summary", "--refresh", "--json"])
    }

    func refreshQuota(status: String = "Refreshing live quota...") {
        runRefresh(status: status, arguments: ["--no-spinner", "companion-summary", "--refresh-quota", "--json"])
    }

    func refreshQuotaOnOpenIfNeeded() {
        guard !isRefreshing else { return }
        let cadence = RefreshCadence(
            storedValue: UserDefaults.standard.string(forKey: RefreshCadence.storageKey)
        )
        guard let minimumInterval = cadence.minimumInterval else { return }
        guard summary?.needsRefreshOnOpen(minimumInterval: minimumInterval) ?? true else { return }
        refreshQuota(status: "Refreshing quota on open...")
    }

    func openTokensCI() {
        if let url = URL(string: "https://tokens.ci/settings") {
            NSWorkspace.shared.open(url)
        }
    }

    func revealCache() {
        if FileManager.default.fileExists(atPath: store.summaryURL.path) {
            NSWorkspace.shared.activateFileViewerSelecting([store.summaryURL])
            return
        }
        NSWorkspace.shared.open(store.summaryURL.deletingLastPathComponent())
    }

    func quit() {
        NSApp.terminate(nil)
    }

    private func runRefresh(status: String, arguments: [String]) {
        guard !isRefreshing else { return }
        isRefreshing = true
        refreshStatus = status
        DispatchQueue.global(qos: .utility).async { [weak self] in
            let result = Self.runCompanionCommand(arguments: arguments)
            DispatchQueue.main.async {
                guard let self else { return }
                self.isRefreshing = false
                self.refreshStatus = result
                self.reload()
            }
        }
    }

    // Copy verbatim from _legacy_main.swift.bak (drop the `nonisolated private static`
    // wrappers' `Self.` call sites already match):
    //   static func runCompanionCommand(arguments:) -> String
    //   static func companionRefreshCandidates() -> [String]
    //   static func dedupePaths(_:) -> [String]
    //   static func runCompanionRefreshProcess(executableURL:arguments:) -> (Bool, String?)
}
```

- [ ] **Step 2: Build (compiles, unused)**

Run: `cd packages/menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 3: Commit**

```bash
git add packages/menubar/Sources/TokscaleMenuBar/MenuBarModel.swift
git commit -m "feat(companion): add MenuBarModel observable object"
```

### Task 7: `MenuBarLabelView`

**Files:**
- Create: `packages/menubar/Sources/TokscaleMenuBar/MenuBarLabelView.swift`

- [ ] **Step 1: Implement the label view**

```swift
import SwiftUI
import TokscaleMenuBarCore

struct MenuBarLabelView: View {
    let summary: TokscaleSummary?

    var body: some View {
        if let constrained = summary.flatMap({ QuotaGlance.mostConstrained(in: $0.quota) }) {
            let remaining = Int(constrained.remainingPercent.rounded())
            HStack(spacing: 3) {
                Image(systemName: "bolt.fill")
                Text("\(remaining)%")
            }
            .foregroundStyle(color(for: constrained.remainingPercent))
        } else {
            Image(systemName: "chart.bar.xaxis")
        }
    }

    private func color(for remaining: Double) -> Color {
        switch QuotaGlance.urgency(remainingPercent: remaining) {
        case .normal: return .primary
        case .warning: return .orange
        case .critical: return .red
        }
    }
}
```

- [ ] **Step 2: Build (compiles, unused)**

Run: `cd packages/menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 3: Commit**

```bash
git add packages/menubar/Sources/TokscaleMenuBar/MenuBarLabelView.swift
git commit -m "feat(companion): add menu bar label view"
```

### Task 8: Glance `TokensPopoverView`

**Files:**
- Create: `packages/menubar/Sources/TokscaleMenuBar/TokensPopoverView.swift` (new file; the old one is parked as `_legacy_popover.swift.bak`)

- [ ] **Step 1: Create the glance popover**

`TokensPopoverView(model: MenuBarModel)` reads `model.summary` and computes glance data once in the body (no `TokscaleDashboardModel`). Port the still-needed presentational helpers from `_legacy_popover.swift.bak` into this file: `providerColor`, `clientDisplayName`, the quota bar subview from `ProviderQuotaRow`, `LiveDot`, `HeaderIconButton`, `ToolbarIconButton`, and `RefreshCadenceRow` + `RefreshCadenceToggle` (move the already-shipped cadence control here unchanged). Do NOT port the dashboard sections (`QuotaBoardSection`, `CompactOverviewStrip`, `OverviewSection`, `LimitsSection`, `HistorySection`).

Layout:

```swift
import SwiftUI
import TokscaleMenuBarCore

struct TokensPopoverView: View {
    @ObservedObject var model: MenuBarModel
    @State private var settingsVisible = false

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            header
            if settingsVisible { settingsPanel }
            if let summary = model.summary {
                quotaRows(summary)
                footer(summary)
            } else {
                Text(model.errorMessage ?? "No data yet. Run `tokens submit` once.")
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
            }
        }
        .padding(14)
        .frame(width: 300, alignment: .leading)
    }

    private var header: some View {
        HStack(spacing: 8) {
            LiveDot(stale: model.summary?.stale ?? true, active: model.isRefreshing)
            Text("Tokens").font(.system(size: 14, weight: .bold, design: .rounded))
            Spacer()
            HeaderIconButton(systemName: model.isRefreshing ? "hourglass" : "arrow.clockwise",
                             tint: .orange, active: model.isRefreshing, disabled: model.isRefreshing,
                             help: "Refresh scan") { model.refreshScan() }
            HeaderIconButton(systemName: "gearshape", tint: .orange, active: settingsVisible,
                             help: "Settings") { settingsVisible.toggle() }
        }
    }

    private var settingsPanel: some View {
        VStack(spacing: 7) {
            HStack(spacing: 7) {
                ToolbarIconButton(systemName: "safari", tint: providerColor("codex"), help: "Open tokens.ci") { model.openTokensCI() }
                ToolbarIconButton(systemName: "folder", tint: providerColor("openclaw"), help: "Reveal cache") { model.revealCache() }
                ToolbarIconButton(systemName: "power", tint: providerColor("claude"), help: "Quit") { model.quit() }
            }
            RefreshCadenceRow(color: providerColor("codex"))
        }
    }

    @ViewBuilder
    private func quotaRows(_ summary: TokscaleSummary) -> some View {
        let order = QuotaGlance.providersByUrgency(summary.quota)
        let mostConstrained = QuotaGlance.mostConstrained(in: summary.quota)?.provider
        if order.isEmpty {
            Text("No live quota windows.").font(.system(size: 11)).foregroundStyle(.secondary)
        } else {
            ForEach(order, id: \.self) { name in
                if let provider = summary.quota.first(where: { $0.provider == name }) {
                    GlanceQuotaRow(provider: provider, isMostConstrained: name == mostConstrained)
                }
            }
        }
    }

    @ViewBuilder
    private func footer(_ summary: TokscaleSummary) -> some View {
        let sevenDay = QuotaGlance.recentSpend(summary.history, days: 7)
        VStack(alignment: .leading, spacing: 3) {
            Text("Today \(usd(summary.today.costUsd)) · 7d \(usd(sevenDay))")
                .font(.system(size: 11, weight: .semibold))
            if let best = QuotaGlance.bestNow(in: summary.quota) {
                Text("Best now → \(clientDisplayName(best.provider)) \(Int(best.remainingPercent.rounded()))%")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.secondary)
            }
        }
    }

    private func usd(_ value: Double) -> String { "$" + String(Int(value.rounded())) }
}
```

`GlanceQuotaRow` is a new private view: provider name (via `clientDisplayName`), a 5h and weekly bar (use the ported bar subview driven by `window.usedPercent`), and `QuotaGlance.resetCountdown(from: window.resetsAt)` shown as `⏱<n>`. When `isMostConstrained`, tint the row with `providerColor(provider.provider)` and a subtle background. Define it in this file.

- [ ] **Step 2: Build (compiles, unused)**

Run: `cd packages/menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 3: Commit**

```bash
git add packages/menubar/Sources/TokscaleMenuBar/TokensPopoverView.swift
git commit -m "feat(companion): glance-only popover with decision-aid layout"
```

---

## Phase 3 — Wire it together

### Task 9: Replace the spike entry and verify the whole experience

**Files:**
- Replace: `packages/menubar/Sources/TokscaleMenuBar/main.swift`

- [ ] **Step 1: Wire the model, label, and popover**

```swift
import SwiftUI

@main
struct TokensMenuBarApp: App {
    @StateObject private var model = MenuBarModel()

    var body: some Scene {
        MenuBarExtra {
            TokensPopoverView(model: model)
                .onAppear { model.refreshQuotaOnOpenIfNeeded() }
        } label: {
            MenuBarLabelView(summary: model.summary)
        }
        .menuBarExtraStyle(.window)
    }
}
```

- [ ] **Step 2: Build**

Run: `cd packages/menubar && swift build`
Expected: `Build complete!`

- [ ] **Step 3: Manual verify the full experience (Bonny)**

Run: `bash packages/menubar/scripts/build-app.sh && open packages/menubar/.build/TokscaleMenuBar.app`
Confirm: menu bar shows `⚡<n>%` colored by urgency; popover is small and opens fast and not crooked; providers sorted most-constrained first with the top one highlighted; reset countdowns show; footer shows today/7d spend and `Best now →`; gear reveals the cadence control + toolbar, and they work.

- [ ] **Step 4: Commit**

```bash
git add packages/menubar/Sources/TokscaleMenuBar/main.swift
git commit -m "feat(companion): wire MenuBarExtra glance app"
```

---

## Phase 4 — Cleanup and ship

### Task 10: Remove backups, prune, full verify, push

**Files:**
- Delete: `_legacy_main.swift.bak`, `_legacy_popover.swift.bak`

- [ ] **Step 1: Delete the parked legacy files**

Run:
```bash
git rm packages/menubar/Sources/TokscaleMenuBar/_legacy_main.swift.bak
git rm packages/menubar/Sources/TokscaleMenuBar/_legacy_popover.swift.bak
```

`TokscaleDashboardModel` and its tests stay (still compile and pass even though the app no longer uses them; removing tested core is out of scope).

- [ ] **Step 2: Full build + tests**

Run: `cd packages/menubar && swift build && swift test`
Expected: `Build complete!` and all tests pass.

- [ ] **Step 3: Manual verify final**

Run: `bash packages/menubar/scripts/build-app.sh && open packages/menubar/.build/TokscaleMenuBar.app`
Confirm the full glance experience and that the gear settings (cadence + toolbar) still work.

- [ ] **Step 4: Commit and push**

```bash
git add -A packages/menubar
git commit -m "chore(companion): drop legacy AppKit menu bar entry"
git push origin codex/accuracy-layer-v0
```

Verify after push: `git rev-parse --short HEAD origin/codex/accuracy-layer-v0` match; `origin/main` unchanged.

---

## Notes

- Git identity must be `Bonny07 <111042029+Bonny07@users.noreply.github.com>`; no AI co-author trailer.
- The already-shipped open-refresh throttle fix and `RefreshCadence` setting are preserved; `refreshQuotaOnOpenIfNeeded` keeps its cadence gate (lifted into `MenuBarModel`).
- `cargo fmt` shows a pre-existing unrelated diff in `crates/tokscale-cli/src/antigravity.rs`; do not touch it.
- Color thresholds default to `≤20%` orange, `≤10%` red; tune after seeing it live.
