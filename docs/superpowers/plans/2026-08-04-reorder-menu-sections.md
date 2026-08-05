# Reorder Menu Sections Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reorder the menu bar report to TOTAL → BREAKDOWN → COST → CLIENT → MODEL → PROJECT while preserving the existing error banner and fixed footer behavior.

**Architecture:** Keep every existing SwiftUI section component unchanged and modify only their composition order in the scrolling report body. Update the two existing product documents to make the new ordering authoritative; do not introduce configurable ordering or new view abstractions.

**Tech Stack:** Swift 6, SwiftUI, XCTest, Markdown, Cargo-built `tokens` CLI

## Global Constraints

- PROJECT is the final normal business section.
- The optional error banner remains after PROJECT.
- The fixed footer remains outside the scrolling report body.
- Do not change report schemas, aggregation, formatting, pagination, panel sizing, or section internals.
- Run the current integrated Swift test suite and build the release menu bar product before final verification.
- Do not create a git commit unless the user explicitly authorizes it.

## File Structure

- Modify `Sources/TokensMenuBarCore/Views.swift`: authoritative runtime composition order for report sections.
- Modify `docs/design-spec.md`: product-level dropdown section order.
- Modify `docs/implementation-plan.md`: implementation-level middle-scroll section order.
- No new production source files or test targets; this is a declarative composition reorder, and introducing an ordering abstraction solely for unit testing would violate the minimal-change design.

---

### Task 1: Reorder the Runtime Sections

**Files:**
- Modify: `Sources/TokensMenuBarCore/Views.swift:172-185`

**Interfaces:**
- Consumes: existing `UsageReport`, `UsageStore.lastError`, and existing private section builders.
- Produces: the report-body visual order TOTAL → BREAKDOWN → COST → CLIENT → MODEL → PROJECT → optional error banner.

- [ ] **Step 1: Run a structural check that demonstrates the current order is wrong**

Run:

```bash
python3 - <<'PY'
from pathlib import Path

text = Path("Sources/TokensMenuBarCore/Views.swift").read_text()
start = text.index("private func bodySections")
end = text.index("\n    @ViewBuilder", start + 1)
body = text[start:end]
expected = [
    "totalSection(report)",
    "breakdownSection(report)",
    "costChartSection(report)",
    "clientSection(report)",
    "modelSection(report)",
    "projectSection(report)",
    "errorBanner(error)",
]
positions = [body.index(call) for call in expected]
assert positions == sorted(positions), positions
PY
```

Expected: FAIL because COST currently follows MODEL and PROJECT currently precedes MODEL.

- [ ] **Step 2: Apply the minimal composition reorder**

Make the `VStack` body exactly:

```swift
VStack(alignment: .leading, spacing: MenuBarLayout.sectionSpacing) {
    totalSection(report)
    breakdownSection(report)
    costChartSection(report)
    clientSection(report)
    modelSection(report)
    projectSection(report)
    if let error = store.lastError {
        errorBanner(error)
    }
}
```

Do not modify any section builder implementation.

- [ ] **Step 3: Re-run the structural order check**

Run the exact Python command from Step 1.

Expected: PASS with no output.

- [ ] **Step 4: Compile the menu bar product**

Run:

```bash
swift build --product TokensMenuBar
```

Expected: build completes successfully.

- [ ] **Step 5: Review the focused runtime diff**

Run:

```bash
git diff -- Sources/TokensMenuBarCore/Views.swift
```

Expected: only the existing section calls move; no section internals or unrelated code change.

---

### Task 2: Synchronize the Product Documentation

**Files:**
- Modify: `docs/design-spec.md:241-249`
- Modify: `docs/implementation-plan.md:319-329`

**Interfaces:**
- Consumes: the approved final visual order.
- Produces: product and implementation documentation that matches runtime behavior.

- [ ] **Step 1: Run a documentation check that demonstrates the current order is stale**

Run:

```bash
python3 - <<'PY'
from pathlib import Path

checks = {
    "docs/design-spec.md": [
        "**By day / cost**",
        "**By client**",
        "**By model**",
        "**By project**",
        "**Optional error banner**",
        "**Fixed footer**",
    ],
    "docs/implementation-plan.md": [
        "`COST · 14 DAYS`",
        "CLIENT rows",
        "MODEL rows",
        "PROJECT rows",
        "Footer actions",
    ],
}
for filename, markers in checks.items():
    text = Path(filename).read_text()
    positions = [text.index(marker) for marker in markers]
    assert positions == sorted(positions), (filename, positions)
PY
```

Expected: FAIL because both documents currently place PROJECT before MODEL and COST.

- [ ] **Step 2: Update the product design order**

Replace the dropdown list with this order and preserve the existing section descriptions:

```markdown
1. **Period control** — segmented: Today | 7d | 30d | All
2. **Summary** — tokens, cost, messages; optional mini token-breakdown
3. **By day / cost** — compact cost bars
4. **By client** — sorted by tokens desc; progress share bar
5. **By model** — flat list with provider label; share bar
6. **By project** — sorted by cost desc; each workspace row shows cost + tokens and its models sorted by cost desc. `Unattributed` never exposes workspace keys or diagnostic session details
7. **Optional error banner** — when present, follows PROJECT within the scrolling report content
8. **Fixed footer** — outside the scrolling content; Last updated (`generatedAt`); **Refresh**; **Settings…**; **Open tokens.ci**; **Quit**
```

- [ ] **Step 3: Update the implementation-plan middle-scroll order**

Make the middle-scroll bullets appear in this order while preserving their existing wording:

```markdown
- TOTAL + cost/messages + date range
- BREAKDOWN 4 cards from `report.tokenBreakdown`
- `COST · 14 DAYS` + `CostChartView(days: report.byDay)`
- CLIENT rows (`report.byClient`) with uppercase `CLIENT` section label
- MODEL rows (`report.byModel`) with uppercase `MODEL` section label
- PROJECT rows (`report.byProject`) with uppercase `PROJECT` section label
```

Keep `Footer actions` after the middle scroll.

- [ ] **Step 4: Re-run the documentation order check**

Run the exact Python command from Step 1.

Expected: PASS with no output.

- [ ] **Step 5: Review the documentation diff**

Run:

```bash
git diff -- docs/design-spec.md docs/implementation-plan.md
```

Expected: only the ordering lists change; descriptions retain their prior meaning.

---

### Task 3: Verify the Integrated Change

**Files:**
- Verify: `Sources/TokensMenuBarCore/Views.swift`
- Verify: `docs/design-spec.md`
- Verify: `docs/implementation-plan.md`

**Interfaces:**
- Consumes: completed runtime and documentation changes.
- Produces: build, test, and real-application evidence that the reorder is correct and regression-free.

- [ ] **Step 1: Check formatting and repository state**

Run:

```bash
git diff --check
git status --short
```

Expected: no whitespace errors; only the approved design document, implementation plan, runtime file, and synchronized product documents are changed or untracked.

- [ ] **Step 2: Build the release menu bar application**

Run:

```bash
swift build -c release --product TokensMenuBar
```

Expected: the release menu bar app builds successfully.

- [ ] **Step 3: Run the full integrated Swift test suite**

Run:

```bash
swift test
```

Expected: all current XCTest tests pass, including the CLI integration coverage.

- [ ] **Step 4: Restart the release menu bar application**

Run:

```bash
make restart-release
```

Expected: any running TokensMenuBar process is stopped and the newly built release binary starts.

- [ ] **Step 5: Perform the visual acceptance check**

Open the menu bar panel and verify:

1. The visible business sections are TOTAL → BREAKDOWN → COST → CLIENT → MODEL → PROJECT.
2. PROJECT is the last normal data section.
3. COST still renders its chart and hover behavior.
4. PROJECT workspace and nested-model pagination still work.
5. The panel scrolls correctly when PROJECT expands.
6. The footer remains fixed below the scrolling body.
7. If an error banner is present, it appears after PROJECT and before the fixed footer.

- [ ] **Step 6: Review the complete diff**

Run:

```bash
git diff --check
git diff --stat
git status --short
```

Expected: the change remains narrowly scoped, with no generated build artifacts tracked and no unrelated files modified.

- [ ] **Step 7: Stop before committing**

Report the verification evidence and leave the worktree changes uncommitted. Commit only after explicit user authorization.
