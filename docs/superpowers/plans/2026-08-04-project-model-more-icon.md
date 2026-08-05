# Project Model More Icon Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace only the nested Project model pagination text with a downward chevron icon while leaving the Project section-level `More` button unchanged.

**Architecture:** Keep the shared top-level text pagination control intact. Add a focused icon-only pagination helper beside it and use that helper only within a Project row's nested model list, preserving the existing action and accessibility metadata.

**Tech Stack:** Swift 6, SwiftUI, SF Symbols, Swift Package Manager, XCTest

## Global Constraints

- Only the nested Project model list's visible `More` text becomes a downward chevron.
- The Project section-level `More` text remains unchanged.
- Existing page sizes, pagination state, click behavior, animation behavior, and period-reset behavior remain unchanged.
- The icon-only control keeps an explicit model-specific accessibility label and remaining-count hint.
- Add no dependency or snapshot-test framework for this presentation-only change.

---

### Task 1: Add the nested model pagination icon

**Files:**
- Modify: `Sources/TokensMenuBarCore/Views.swift:437-455`
- Modify: `Sources/TokensMenuBarCore/Views.swift:538-548`
- Verify: `Tests/TokensMenuBarTests/`

**Interfaces:**
- Consumes: the existing `remaining: Int`, `accessibilityNoun: String`, and `action: @escaping () -> Void` pagination inputs.
- Produces: a private SwiftUI helper used only by nested Project model pagination; no public API or data-model changes.

- [ ] **Step 1: Record the current presentation boundary**

Run:

```bash
rg -n 'expandChevron|Text\("More"\)|projectModelVisibleCounts' Sources/TokensMenuBarCore/Views.swift
```

Expected: top-level CLIENT, PROJECT, and MODEL pagination plus nested Project model pagination all use the shared text control.

There is no existing SwiftUI snapshot or view-inspection test infrastructure. Do not add a dependency solely to assert an SF Symbol name; use the compiler, existing regression suite, accessibility metadata inspection, and live UI verification instead.

- [ ] **Step 2: Add the dedicated icon-only helper**

Add this private helper beside the existing text pagination helper:

```swift
private func projectModelExpandIcon(
    remaining: Int,
    accessibilityNoun: String,
    action: @escaping () -> Void
) -> some View {
    Button(action: action) {
        Image(systemName: "chevron.down")
            .font(.system(size: 10, weight: .medium))
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 6)
            .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
    .accessibilityLabel("Show more \(accessibilityNoun)")
    .accessibilityHint("\(remaining) more")
}
```

Keep the existing shared text helper unchanged so top-level pagination continues to show `More`.

- [ ] **Step 3: Switch only nested Project model pagination to the icon helper**

In the `hasMoreModels` branch inside the Project row, replace the shared helper call with:

```swift
projectModelExpandIcon(
    remaining: project.models.count - visibleModels.count,
    accessibilityNoun: "models for \(project.displayName)"
) {
    projectModelVisibleCounts[project.id] = min(
        visibleModelCount + MenuBarLayout.projectModelPageSize,
        project.models.count
    )
}
```

Do not change the surrounding count calculation or action body.

- [ ] **Step 4: Run automated regression tests**

Run:

```bash
swift test
```

Expected: all existing tests pass with zero failures.

- [ ] **Step 5: Verify a release build**

Run:

```bash
swift build -c release
```

Expected: the menu-bar app and core package compile successfully in release configuration.

- [ ] **Step 6: Verify the targeted source boundary**

Run:

```bash
rg -n 'projectModelExpandIcon|expandChevron\(remaining:.*projects|Text\("More"\)' Sources/TokensMenuBarCore/Views.swift
```

Expected:

- Nested Project model pagination calls `projectModelExpandIcon`.
- Project section pagination still calls the existing text helper.
- The visible `More` text remains in the shared top-level helper.

- [ ] **Step 7: Verify the live menu-bar UI**

Launch the release app using the project's established run flow. Open a period containing a Project with more than three models and enough projects to show section pagination.

Expected:

- The nested model control is a centered downward chevron.
- Clicking it reveals the next model page.
- The Project section-level control still reads `More` and loads more projects.
- The two controls no longer appear as duplicate text buttons.
- VoiceOver exposes `Show more models for <project>` and the remaining-count hint for the icon-only control.

Do not commit unless the user explicitly requests a commit.
