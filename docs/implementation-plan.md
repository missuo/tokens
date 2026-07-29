# Menu Bar Minimal Mono UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the macOS Tokens Menu Bar panel and Settings to match the locked **FINAL · 06 Minimal Mono v2** visual language and its interaction frames (chart hover, long-list scroll, settings sheet).

**Architecture:** Keep the existing data path (`UsageStore` → `tokens usage --json`). Replace presentation only: extract design tokens + small pure views (`CostChartView`, share bars, section chrome), rewrite `MenuPanelView` layout, restyle `SettingsView`, and tighten popover sizing in `AppMain`. No CLI schema change; chart uses existing `byDay[].cost` / `byDay[].tokens` / `byDay[].date`.

**Tech Stack:** SwiftUI, AppKit `NSPopover` / `NSWindow`, SwiftPM package `.`, XCTest.

**Design source (canonical):**
- Preview: `design/menubar-ui-v1/` (tab **Final** + **Interactions**)
- Shots: `full-final.png`, `full-ix-hover.png`, `full-ix-scroll.png`, `full-ix-settings.png`, `full-ix-settings-ctx.png`
- Notes: `design/menubar-ui-v1/README.md`

## Global Constraints

- macOS **13.0+**; menu-bar-only app (`LSUIElement` / `.accessory`)
- Panel width stays **400pt** (popover content width)
- Do **not** change CLI JSON contract (`schemaVersion: 1`) in this plan
- Do **not** reintroduce section hairline `Divider`s between content blocks (spacing-only rhythm)
- Mono visual language: prefer monospaced digits + uppercase micro-labels; system font stack is OK if monospaced digit attributes are applied consistently
- Light/Dark: follow system appearance (no custom theme toggle in app v1); tokens must work in both
- Keep existing settings keys / `AppSettings` fields; only restyle UI unless a field is already present but unlabeled
- Copy tone: short, uppercase section labels (`TOTAL`, `BREAKDOWN`, `CLIENT`, `MODEL`, `COST · 14 DAYS`), footer actions uppercase (`REFRESH`, `SETTINGS`, `TOKENS.CI`, `QUIT`)
- Number formatting stays in `Formatting` (existing tests must keep passing)
- Frequent small commits after each green test cycle

## Design Spec Summary (what “done” looks like)

### Visual language (FINAL 06)

| Element | Spec |
|--------|------|
| Panel chrome | Near-black / near-white surface, **2pt** corner radius feel (SwiftUI: `RoundedRectangle(cornerRadius: 2)` only if drawing custom chrome; popover may keep system shape — do not fight NSPopover heavily) |
| Type | Uppercase tracking on labels; large TOTAL (~28–36pt, medium weight, tabular/monospaced digits); body lists 12–13pt |
| Section rhythm | **No** horizontal rules between sections; vertical spacing **≈18–22pt** between section blocks; horizontal padding **18pt** |
| Header | Left `TOKENS` (tracked caps); right muted `usage · local` (or loading state) |
| Period | Underline-style tabs (not filled segmented capsule): `TODAY · 7D · 30D · ALL`; active = 2pt bottom bar + primary text; inactive = muted |
| Summary | Label `TOTAL` → huge tokens; row of `COST` + `MESSAGES`; date range muted `YYYY-MM-DD — YYYY-MM-DD` |
| Breakdown | **4 equal cards** in one row: `in` / `out` / `cache` / `reason`; card fill subtle; **2pt top accent** bars in mono steps (full → ~28% opacity), not neon colors |
| Client | Rows: name · `tokens · share%`; **2pt** high share bar, **radius 0**; sorted by tokens desc (already from CLI) |
| Model | Flat rows: `modelId / provider` · tokens (no nested client disclosure in final UI) |
| Day → Cost chart | Bar chart, **Y = cost ($)**, **X = date**; default window = **last up to 14 days** of `byDay` (see Task 3 rules); Y ticks 0 / mid / max; sparse X labels |
| Footer | No top divider; muted `UPDATED … · {scan.mode}`; actions: `REFRESH` · `SETTINGS` · `TOKENS.CI` · `QUIT` |

### Interactions

#### IX-A · Chart hover / focus
- Hover or keyboard focus on one bar:
  - Active bar: opacity **1.0**, 1pt outline in primary text color
  - Other bars: opacity **≈0.28**
  - Vertical guide line through bar center
  - Tooltip above bar (clamp inside panel): date `YYYY-MM-DD`, `cost`, `tokens`
- Mouse exit / blur: clear hover
- Empty `byDay`: show muted `No daily data` (no chart axis)

#### IX-B · Long list scroll
- Panel body has a max height (keep **~420–480pt** content region so popover stays usable)
- **Client** and **Model** sections each scroll internally when item count > **5** (replace “Show all N” expand pattern for these two lists)
- While scrollable and not at edge: **top and bottom fade** (8–22pt gradient to panel background)
- Prefer thin indicator; if custom scroller is too heavy on AppKit, `ScrollView` + fades is acceptable — do **not** use chunky always-on fat bars
- Mid-scroll state must keep header / period / footer fixed (only middle list region scrolls, or whole middle scroll with sticky header — pick **fixed chrome + one middle ScrollView** as in current structure, with **inner** scroll for long client/model **or** single scroll with fades; preferred: **single middle ScrollView** for summary→chart, and if client/model exceed 8 rows total visible budget, those subsections use nested scroll with max height ~168pt as in design)

**Implementation choice (locked):**
1. Outer structure: header + period + **one** middle `ScrollView` (maxHeight 420) + footer (same as today)
2. Inside client/model: if `count > 8`, wrap that section’s rows in nested `ScrollView` with `.frame(maxHeight: 168)` + fade overlays
3. Remove “Show all N / Show less” toggles for client/model/day (chart replaces day list)

#### IX-C / IX-D · Settings
- Window ~**420×360**, title `Settings` / `Tokens Settings`
- Mono form: section labels uppercase; Display = 3-segment control (Tokens / Cost / Both); Interval = menu/picker showing current value; Full Rescan card row; CLI path selectable monospaced; Recheck + Done
- Opens from footer `SETTINGS` as today (`store.showSettings = true` → `AppMain.presentSettings`)
- Visual restyle only; behavior unchanged

### Explicit non-goals (this plan)
- No Launch at Login, filters, custom date range, or rank/leaderboard embed
- No rewriting `UsageService` / CLI
- No nested client→models disclosure (final design is flat)
- No colorful brand/neon accents from design 04 (only the **card grid structure**)

## File map

| File | Role |
|------|------|
| `Sources/TokensMenuBarCore/DesignTokens.swift` | **Create** — colors, spacing, type helpers for light/dark mono UI |
| `Sources/TokensMenuBarCore/CostChartView.swift` | **Create** — 14-day cost bar chart + hover tooltip |
| `Sources/TokensMenuBarCore/Views.swift` | **Rewrite** — `MenuPanelView` layout to FINAL 06; keep `SettingsView` in same file **or** move if file exceeds ~400 lines |
| `Sources/TokensMenuBarCore/Formatting.swift` | **Extend** — short day labels for chart X axis if needed |
| `Sources/TokensMenuBar/AppMain.swift` | **Tweak** — popover content size, settings window size/title if needed |
| `Tests/TokensMenuBarTests/FormattingTests.swift` | **Keep** + add chart window helper tests |
| `Tests/TokensMenuBarTests/CostChartTests.swift` | **Create** — pure functions for day window + yMax |
| `design/menubar-ui-v1/README.md` | Reference only (already documents final) |
| `docs/design-spec.md` | Optional follow-up doc note: UI visual = Minimal Mono v2 (do not block implementation) |

---

### Task 1: Design tokens + chart pure helpers (test-first)

**Files:**
- Create: `Sources/TokensMenuBarCore/DesignTokens.swift`
- Create: `Sources/TokensMenuBarCore/CostChartMath.swift`
- Create: `Tests/TokensMenuBarTests/CostChartTests.swift`
- Modify: `Sources/TokensMenuBarCore/Formatting.swift` (add `chartDayLabel`)
- Modify: `Tests/TokensMenuBarTests/FormattingTests.swift`

**Interfaces:**
- Produces:
  - `enum MenuBarTheme` (or `struct MenuBarTokens`) with static metrics: `panelWidth: CGFloat = 400`, `contentMaxHeight: CGFloat = 420`, `sectionSpacing: CGFloat = 22`, `horizontalPadding: CGFloat = 18`, `shareBarHeight: CGFloat = 2`, `breakdownCardTopAccent: [Color]` mono steps
  - `CostChartMath.daysForChart(from: [DayUsage], limit: Int = 14) -> [DayUsage]`
  - `CostChartMath.yMax(costs: [Double]) -> Double`
  - `Formatting.chartDayLabel(isoDate: String) -> String` // `"2026-07-24"` → `"24"` or `"07-24"` — use **day-of-month** `"24"` for dense 14-bar charts; tooltip uses full ISO date

**Rules for `daysForChart`:**
1. Input `byDay` is chronological ascending from CLI (verify in models usage; current UI does `reversed()` for newest-first lists)
2. Sort ascending by `date` string (`YYYY-MM-DD` sorts lexicographically)
3. Take the **last `limit` items** (default 14)
4. Return ascending (oldest → newest) for left→right bars
5. Empty input → empty output

**Rules for `yMax`:**
1. `max(costs)` then `ceil` to a readable top (at least `1` if any positive cost; if all zero → `1` so axis still draws)

- [ ] **Step 1: Write failing tests for chart math**

```swift
// CostChartTests.swift
import XCTest
@testable import TokensMenuBarCore

final class CostChartTests: XCTestCase {
    private func day(_ date: String, cost: Double, tokens: Int64 = 0) -> DayUsage {
        DayUsage(date: date, tokens: tokens, cost: cost, messages: 0, intensity: 0)
    }

    func testDaysForChart_takesLast14Ascending() {
        let input = (1...20).map { day(String(format: "2026-07-%02d", $0), cost: Double($0)) }
        let out = CostChartMath.daysForChart(from: input, limit: 14)
        XCTAssertEqual(out.count, 14)
        XCTAssertEqual(out.first?.date, "2026-07-07")
        XCTAssertEqual(out.last?.date, "2026-07-20")
    }

    func testDaysForChart_shortSeriesPassthrough() {
        let input = [day("2026-07-25", cost: 1), day("2026-07-26", cost: 2)]
        let out = CostChartMath.daysForChart(from: input, limit: 14)
        XCTAssertEqual(out.map(\.date), ["2026-07-25", "2026-07-26"])
    }

    func testYMax_ceils() {
        XCTAssertEqual(CostChartMath.yMax(costs: [1.2, 5.8, 3]), 6)
        XCTAssertEqual(CostChartMath.yMax(costs: [0, 0]), 1)
    }
}
```

- [ ] **Step 2: Run tests — expect FAIL**

```bash
cd . && swift test --filter CostChartTests
```

Expected: compile error `CostChartMath` not found.

- [ ] **Step 3: Implement `CostChartMath` + `Formatting.chartDayLabel` + minimal `DesignTokens`**

```swift
// CostChartMath.swift
import Foundation

public enum CostChartMath {
    public static func daysForChart(from days: [DayUsage], limit: Int = 14) -> [DayUsage] {
        let sorted = days.sorted { $0.date < $1.date }
        guard sorted.count > limit else { return sorted }
        return Array(sorted.suffix(limit))
    }

    public static func yMax(costs: [Double]) -> Double {
        let m = costs.max() ?? 0
        if m <= 0 { return 1 }
        return ceil(m)
    }
}
```

```swift
// Formatting.swift addition
public static func chartDayLabel(isoDate: String) -> String {
    // "2026-07-24" -> "24"
    if isoDate.count >= 10 {
        return String(isoDate.suffix(2))
    }
    return isoDate
}
```

```swift
// DesignTokens.swift — spacing + label fonts only; colors via primary/secondary
import SwiftUI

public enum MenuBarLayout {
    public static let panelWidth: CGFloat = 400
    public static let contentMaxHeight: CGFloat = 420
    public static let horizontalPadding: CGFloat = 18
    public static let sectionSpacing: CGFloat = 22
    public static let nestedListMaxHeight: CGFloat = 168
    public static let nestedListThreshold = 8
    public static let shareBarHeight: CGFloat = 2
    public static let chartHeight: CGFloat = 128
}
```

- [ ] **Step 4: Run tests — expect PASS**

```bash
cd . && swift test --filter CostChartTests
cd . && swift test --filter FormattingTests
```

- [ ] **Step 5: Commit**

```bash
git add Sources/TokensMenuBarCore/CostChartMath.swift \
        Sources/TokensMenuBarCore/DesignTokens.swift \
        Sources/TokensMenuBarCore/Formatting.swift \
        Tests/TokensMenuBarTests/CostChartTests.swift \
        Tests/TokensMenuBarTests/FormattingTests.swift
git commit -m "$(cat <<'EOF'
feat(macos): add Minimal Mono chart math and layout tokens

EOF
)"
```

---

### Task 2: `CostChartView` (bars + hover tooltip)

**Files:**
- Create: `Sources/TokensMenuBarCore/CostChartView.swift`
- Test: reuse `CostChartTests` (no View snapshot tests required)

**Interfaces:**
- Consumes: `CostChartMath`, `Formatting.chartDayLabel`, `Formatting.cost`, `Formatting.compactTokens`, `[DayUsage]`
- Produces: `struct CostChartView: View` with `days: [DayUsage]`, optional `height: CGFloat = MenuBarLayout.chartHeight`

**Interaction details (must match IX-A):**
- `@State private var hoveredDate: String? = nil`
- Each bar is a `Rectangle` in an `HStack(alignment: .bottom, spacing: 3)`
- Bar height = `plotHeight * CGFloat(day.cost / yMax)` with minimum 2pt if cost > 0
- `.onHover { inside in ... }` set/clear `hoveredDate` (macOS)
- When `hoveredDate == day.date`: bar opacity 1 + stroke; else if any hover active: opacity 0.28; else opacity 0.88
- Overlay tooltip `VStack` with date / cost / tokens when hover non-nil; position near hovered index via `GeometryReader` or approximate with `HStack` + background alignment
- Y-axis: three labels using `Formatting.cost` is wrong for axis (axis is numeric dollars without forcing `<$0.01` noise) — use `"$\(Int(v))"` for tick labels 0, yMax/2, yMax
- Accessibility: each bar `accessibilityLabel("\(day.date), \(Formatting.cost(day.cost)), \(Formatting.compactTokens(day.tokens)) tokens")`

- [ ] **Step 1: Implement `CostChartView`**

Sketch (implement fully in file; adjust tooltip positioning carefully):

```swift
import SwiftUI

public struct CostChartView: View {
    public let days: [DayUsage]
    public var height: CGFloat = MenuBarLayout.chartHeight

    @State private var hoveredDate: String?

    public init(days: [DayUsage], height: CGFloat = MenuBarLayout.chartHeight) {
        self.days = days
        self.height = height
    }

    public var body: some View {
        let chartDays = CostChartMath.daysForChart(from: days)
        let costs = chartDays.map(\.cost)
        let yMax = CostChartMath.yMax(costs: costs)
        // ... GeometryReader layout: padL 34, padB 22, bars, hover, tooltip
    }
}
```

- [ ] **Step 2: Build package**

```bash
cd . && swift build
```

Expected: success.

- [ ] **Step 3: Manual sanity (optional while developing)**  
  Temporarily preview via `swift run` if easy; otherwise covered when Task 3 wires it.

- [ ] **Step 4: Commit**

```bash
git add Sources/TokensMenuBarCore/CostChartView.swift
git commit -m "$(cat <<'EOF'
feat(macos): add CostChartView with hover tooltip

EOF
)"
```

---

### Task 3: Rewrite `MenuPanelView` to FINAL Minimal Mono

**Files:**
- Modify: `Sources/TokensMenuBarCore/Views.swift` (`MenuPanelView` and private helpers)
- Optionally split large pieces into `MenuPanelComponents.swift` if `Views.swift` becomes hard to edit

**Interfaces:**
- Consumes: `UsageStore`, `AppSettings`, `CostChartView`, `MenuBarLayout`, `Formatting`
- Produces: updated `MenuPanelView` matching FINAL structure

**Section order (top → bottom):**
1. Header — `TOKENS` + trailing status (`usage · local` or spinner)
2. Period underline tabs bound to `store.period` / `store.setPeriod`
3. Middle scroll:
   - TOTAL + cost/messages + date range
   - BREAKDOWN 4 cards from `report.tokenBreakdown` (`input`, `output`, `cacheRead`, `reasoning` — label `in/out/cache/reason`; **omit cacheWrite** in the 4-up to match design; if product wants 5th metric later, do not squeeze into this plan)
   - CLIENT rows (`report.byClient`)
   - MODEL rows (`report.byModel`)
   - `COST · 14 DAYS` + `CostChartView(days: report.byDay)` (helper always caps at 14; period already filters CLI `byDay`)
4. Footer actions

**Remove / replace from current UI:**
- `Divider()` under header and above footer
- Filled `Picker` segmented style → custom underline tabs
- `summaryCard` gray rounded system card → flat TOTAL hierarchy
- Chip row breakdown → 4 cards
- `DisclosureGroup` client nesting + expand toggles
- `daySection` horizontal token bars → `CostChartView`
- “Show all N” buttons

**Period tab binding:**

```swift
ForEach(UsagePeriod.allCases) { period in
    Button {
        store.setPeriod(period)
    } label: {
        Text(period.monoTitle) // add computed prop or map: TODAY/7D/30D/ALL
            .frame(maxWidth: .infinity)
            .padding(.vertical, 6)
            .overlay(alignment: .bottom) {
                Rectangle()
                    .fill(store.period == period ? Color.primary : .clear)
                    .frame(height: 2)
            }
    }
    .buttonStyle(.plain)
}
```

Add on `UsagePeriod`:

```swift
public var monoTitle: String {
    switch self {
    case .today: return "TODAY"
    case .days7: return "7D"
    case .days30: return "30D"
    case .all: return "ALL"
    }
}
```

**Breakdown cards:**

```swift
// labels & values
[("in", b.input), ("out", b.output), ("cache", b.cacheRead), ("reason", b.reasoning)]
```

Top accent: `Color.primary.opacity([1, 0.72, 0.48, 0.28][i])` as 2pt top border.

**Share bar:**

```swift
GeometryReader { geo in
    ZStack(alignment: .leading) {
        Rectangle().fill(Color.secondary.opacity(0.15))
        Rectangle()
            .fill(Color.primary)
            .frame(width: max(2, geo.size.width * CGFloat(min(max(share, 0), 1))))
    }
}
.frame(height: MenuBarLayout.shareBarHeight)
```

**Nested scroll for long lists:**

```swift
if report.byClient.count > MenuBarLayout.nestedListThreshold {
    ScrollView {
        clientRows
    }
    .frame(maxHeight: MenuBarLayout.nestedListMaxHeight)
} else {
    clientRows
}
```

Same for models.

**Footer copy:**

```
UPDATED {relative} · {scan.mode}
REFRESH    SETTINGS    TOKENS.CI    QUIT
```

Wire to existing `store.manualRefresh()`, `store.showSettings = true`, `store.openTokensSite()`, `store.quit()`.

**Error / missing CLI / loading:** keep behavior; restyle text to mono muted hierarchy (no need for pixel-perfect system alerts).

- [ ] **Step 1: Add `UsagePeriod.monoTitle` in `Models.swift`**

- [ ] **Step 2: Rewrite `MenuPanelView` body and helpers**  
  Delete obsolete `@State` expand flags if unused (`clientsExpanded`, `modelsExpanded`, `daysExpanded`, `expandedClientModels`).

- [ ] **Step 3: Build + unit tests**

```bash
cd . && swift test
```

Expected: all pass.

- [ ] **Step 4: Run app and visual-check against `full-final.png`**

```bash
cd . && swift run TokensMenuBar
```

Checklist:
- [ ] No hairline dividers between sections
- [ ] TOTAL hierarchy readable
- [ ] 4 breakdown cards
- [ ] Chart shows ≤14 days, Y is cost
- [ ] Hover tooltip works on chart
- [ ] Period switch updates data
- [ ] Footer actions work

- [ ] **Step 5: Commit**

```bash
git add Sources/TokensMenuBarCore/Models.swift \
        Sources/TokensMenuBarCore/Views.swift
git commit -m "$(cat <<'EOF'
feat(macos): restyle MenuPanelView to Minimal Mono final design

EOF
)"
```

---

### Task 4: Restyle Settings (IX-C)

**Files:**
- Modify: `Sources/TokensMenuBarCore/Views.swift` (`SettingsView`)
- Modify: `Sources/TokensMenuBar/AppMain.swift` (`presentSettings` size/title)

**Behavior (unchanged):**
- Display mode picker → `settings.displayMode` → `store.updateStatusTitle()`
- Interval picker → `settings.scanInterval` → `store.restartTimer()`
- Full Rescan → `store.fullRescan()` then dismiss
- Recheck CLI → `store.resolveBinary()`
- Show `store.binaryPath`, `store.lastError`

**Visual:**
- Prefer custom `VStack` form matching mono cards over heavy system `Form` grouped chrome **if** Form fights the look; acceptable hybrid: `Form` with monospaced path + uppercase section headers via `.font(.system(.caption, design: .monospaced))`
- Window content size: **420×360**
- Title: `Settings`
- Primary actions: Done (toolbar or bottom), Full Rescan emphasized row

**AppMain tweak:**

```swift
window.title = "Settings"
window.setContentSize(NSSize(width: 420, height: 360))
```

Popover (panel) size — allow taller chart:

```swift
popover.contentSize = NSSize(width: 400, height: 680)
```

- [ ] **Step 1: Restyle `SettingsView`**
- [ ] **Step 2: Update `presentSettings` + popover content size**
- [ ] **Step 3: Build + run; open Settings from footer**

```bash
cd . && swift build && swift run TokensMenuBar
```

Verify: change Display to Both updates status item; Full Rescan triggers load; Done closes window.

- [ ] **Step 4: Commit**

```bash
git add Sources/TokensMenuBarCore/Views.swift \
        Sources/TokensMenuBar/AppMain.swift
git commit -m "$(cat <<'EOF'
feat(macos): restyle Settings to Minimal Mono language

EOF
)"
```

---

### Task 5: Scroll fades + polish + regression pass

**Files:**
- Modify: `Views.swift` (fade overlays on nested lists)
- Modify: `CostChartView.swift` if tooltip clamping bugs appear
- Update: `README.md` — one short “UI: Minimal Mono v2” note + link to design folder

**Fade overlay pattern:**

```swift
.overlay(alignment: .top) {
    LinearGradient(colors: [panelBg, panelBg.opacity(0)], startPoint: .top, endPoint: .bottom)
        .frame(height: 16)
        .allowsHitTesting(false)
}
```

Use `Color(nsColor: .windowBackgroundColor)` or solid approximate for popover material.

**Polish checklist:**
- [ ] Tabular numbers on all metrics
- [ ] Truncation middle on long model ids
- [ ] Loading spinner only in header trailing, not blocking chart
- [ ] Period change clears chart hover state (`.onChange(of: store.period)`)
- [ ] Missing CLI / error still usable
- [ ] Dark + light appearance both readable (toggle macOS appearance)

- [ ] **Step 1: Implement fades + hover clear on period change**
- [ ] **Step 2: Full test suite**

```bash
cd . && swift test
```

- [ ] **Step 3: Manual QA script**
  1. Open popover — matches FINAL structure  
  2. Hover peak day — tooltip + dim siblings  
  3. Switch Today / 7d / 30d / All — chart bar count changes with data  
  4. If many clients (or mock), nested list scrolls with fade  
  5. Settings open/close, display mode, rescan  
  6. Quit / tokens.ci / Refresh  

- [ ] **Step 4: Commit**

```bash
git add Sources/TokensMenuBarCore \
        Sources/TokensMenuBar/AppMain.swift \
        README.md
git commit -m "$(cat <<'EOF'
feat(macos): polish Minimal Mono scroll fades and QA fixes

EOF
)"
```

---

### Task 6: Spec cross-link (docs only)

**Files:**
- Modify: `docs/design-spec.md` — add a short “UI visual language” subsection under §4 pointing at FINAL 06 + this plan
- Modify: `design/menubar-ui-v1/README.md` — mark “Implemented in progress / done” only if code landed (set status when Task 5 merges)

- [ ] **Step 1: Add spec pointer**

```markdown
### UI visual language (2026-07-26)

Locked design: **Minimal Mono v2** (`design/menubar-ui-v1/`, FINAL 06).
Implementation plan: `docs/implementation-plan.md`.

Overrides earlier generic “system appearance only” chrome: mono typography, spacing-only sections, breakdown cards, cost chart (≤14 days), chart hover, nested long-list scroll, restyled settings.
```

- [ ] **Step 2: Commit**

```bash
git add docs/design-spec.md \
        design/menubar-ui-v1/README.md
git commit -m "$(cat <<'EOF'
docs: link Menu Bar Minimal Mono UI plan to product spec

EOF
)"
```

---

## Self-review

| Spec / design item | Task |
|--------------------|------|
| Mono visual FINAL 06 | Task 3 |
| Breakdown 4 cards | Task 3 |
| No section hairlines / spacing | Task 3 |
| Cost chart ≤14d, Y=cost | Tasks 1–3 |
| Chart hover tooltip | Task 2 |
| Long list scroll + fades | Tasks 3, 5 |
| Settings mono restyle | Task 4 |
| Footer actions | Task 3 |
| Existing formatting tests | Task 1 keeps them |
| No CLI changes | All tasks |

**Placeholder scan:** none intentional.  
**Type consistency:** `DayUsage`, `CostChartMath.daysForChart`, `MenuBarLayout.*`, `UsagePeriod.monoTitle` named consistently across tasks.

## Out of scope / follow-ups (do not implement now)
- Custom NSPanel instead of NSPopover for exact 2px radius chrome
- Animated chart transitions
- Keyboard roving tab index across bars beyond system focus
- Widget / iOS parity
- Theming beyond system light/dark

---

## Execution handoff

Plan saved to `docs/implementation-plan.md`.

**Two execution options:**

1. **Subagent-Driven (recommended)** — fresh subagent per task, review between tasks  
2. **Inline Execution** — run tasks in this session with executing-plans checkpoints  

Which approach?
