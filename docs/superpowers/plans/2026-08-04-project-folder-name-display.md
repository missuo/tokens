# Project Folder Name Display Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show only the final project folder name in PROJECT rows, truncate long names without displacing usage metrics, and reveal the complete folder name on hover.

**Architecture:** Claude session parsing reads optional JSONL `cwd` values and applies the latest valid final-folder label to `workspace_label` while preserving the path-derived `workspace_key`. Aggregation promotes that label into report `displayName`. The menu bar `ProjectUsage.folderName` presentation property prefers non-empty `displayName` over encoded keys, and PROJECT rows consume it for visible text, hover help, accessibility, and nested-model expansion copy. Report schema fields remain unchanged.

**Tech Stack:** Rust (tokens-core Claude parser), Swift 5.9, SwiftUI, XCTest, Swift Package Manager, macOS 13+

## Global Constraints

- Execute all implementation work in an isolated git worktree created with `superpowers:using-git-worktrees`.
- Preserve Claude `workspace_key` / `projectKey` identity from the session path; never replace it with `cwd`.
- Prefer non-empty report `displayName` (cwd-corrected) for menu bar labels; do not derive visible text from an encoded Claude key when `displayName` is non-empty.
- Do not display the full project path or full cwd in the row, hover text, or accessibility text.
- Keep the project name on one line and truncate overflow with an ellipsis (tail).
- Hover text must contain the complete, untruncated final folder name only.
- Cost and token values must retain layout priority and remain visible.
- A nil project key is unattributed, always renders `Unattributed`, and never exposes display-name or path data.
- Increment only the Claude per-client parser version for cache invalidation.
- Do not change report schema, aggregation, sorting, pagination, panel width, or tooltip styling.
- Do not create git commits unless the user explicitly authorizes commits during execution.

## File Structure

- `cli/tokens-core/src/sessions/claudecode.rs` — deserializes optional `cwd`, tracks latest valid label, applies it to emitted messages; owns focused Rust tests.
- `cli/tokens-core/src/message_cache.rs` — Claude-only `parser_version` bump.
- `Sources/TokensMenuBarCore/Models.swift` — owns the reusable project folder-name derivation and fallback behavior.
- `Sources/TokensMenuBarCore/Views.swift` — renders the derived name, truncation behavior, system hover help, and accessible copy.
- `Tests/TokensMenuBarTests/ProjectUsageTests.swift` — verifies displayName preference, path extraction, fallbacks, long-name preservation, and unattributed privacy behavior.

---

### Task 1: Derive and Render Safe Project Folder Names

**Files:**
- Create: `Tests/TokensMenuBarTests/ProjectUsageTests.swift`
- Modify: `Sources/TokensMenuBarCore/Models.swift:335-343`
- Modify: `Sources/TokensMenuBarCore/Views.swift:510-555`

**Interfaces:**
- Consumes: `ProjectUsage.projectKey: String?` and `ProjectUsage.displayName: String` from decoded usage reports.
- Produces: `ProjectUsage.folderName: String`, a non-empty presentation value that never returns a full path when a usable final component exists.
- UI contract: PROJECT row visible text, hover help, accessibility label, and nested-model expansion noun all consume `folderName`.

- [ ] **Step 1: Create focused failing tests for folder-name selection**

Create `Tests/TokensMenuBarTests/ProjectUsageTests.swift` with:

```swift
import XCTest
@testable import TokensMenuBarCore

final class ProjectUsageTests: XCTestCase {
    func testFolderNameUsesFinalPathComponent() {
        let project = makeProject(
            projectKey: "/Users/example/Documents/Codebase/tokens",
            displayName: "/Users/example/Documents/Codebase/tokens"
        )

        XCTAssertEqual(project.folderName, "tokens")
    }

    func testFolderNameIgnoresTrailingSeparators() {
        let project = makeProject(
            projectKey: "/Users/example/Documents/Codebase/tokens///",
            displayName: "legacy-name"
        )

        XCTAssertEqual(project.folderName, "tokens")
    }

    func testFolderNamePreservesCompleteLongFolderName() {
        let longName = "an-extremely-long-project-folder-name-that-will-not-fit-in-the-row"
        let project = makeProject(
            projectKey: "/Users/example/\(longName)",
            displayName: "legacy-name"
        )

        XCTAssertEqual(project.folderName, longName)
    }

    func testFolderNameFallsBackForUnusableProjectKey() {
        let project = makeProject(projectKey: "///", displayName: "Legacy Project")

        XCTAssertEqual(project.folderName, "Legacy Project")
    }

    func testFolderNameKeepsUnattributedPrivate() {
        let project = makeProject(
            projectKey: nil,
            displayName: "/Users/example/secret-project"
        )

        XCTAssertEqual(project.folderName, "Unattributed")
    }

    func testFolderNameUsesUnattributedWhenAllNamesAreEmpty() {
        let project = makeProject(projectKey: "", displayName: "")

        XCTAssertEqual(project.folderName, "Unattributed")
    }

    private func makeProject(projectKey: String?, displayName: String) -> ProjectUsage {
        ProjectUsage(
            projectKey: projectKey,
            displayName: displayName,
            tokens: 0,
            cost: 0,
            messages: 0,
            models: []
        )
    }
}
```

- [ ] **Step 2: Run the focused tests and verify the expected failure**

Run:

```bash
swift test --filter ProjectUsageTests
```

Expected: compilation fails because `ProjectUsage` has no member named `folderName`.

- [ ] **Step 3: Add the minimal folder-name derivation**

Add this computed property inside `ProjectUsage` in `Sources/TokensMenuBarCore/Models.swift`:

```swift
public var folderName: String {
    guard let projectKey else { return "Unattributed" }
    let fallbackName = displayName.isEmpty ? "Unattributed" : displayName
    guard let lastComponent = projectKey
        .split(separator: "/", omittingEmptySubsequences: true)
        .last
    else {
        return fallbackName
    }
    return String(lastComponent)
}
```

Keep `id`, stored properties, and Codable behavior unchanged.

- [ ] **Step 4: Run the focused tests and verify they pass**

Run:

```bash
swift test --filter ProjectUsageTests
```

Expected: all six `ProjectUsageTests` pass.

- [ ] **Step 5: Render the derived name with truncation and hover help**

In `projectRow(_:)`, bind the presentation name once near the existing model-count locals:

```swift
let folderName = project.folderName
```

Replace the project-name text with:

```swift
Text(folderName)
    .font(.system(size: 12, weight: .medium, design: .monospaced))
    .lineLimit(1)
    .truncationMode(.tail)
    .frame(minWidth: 0, maxWidth: .infinity, alignment: .leading)
    .help(folderName)
    .accessibilityLabel(folderName)
```

Keep the cost/token text's existing `.layoutPriority(2)` unchanged. Replace the nested expansion accessibility noun with:

```swift
accessibilityNoun: "models for \(folderName)"
```

Do not use `projectKey` directly in any visible, hover, or accessibility string.

- [ ] **Step 6: Run the complete Swift test suite**

Run:

```bash
make test
```

Expected: the existing suite and all new tests pass with zero failures.

- [ ] **Step 7: Build and relaunch the release app from the worktree**

Run:

```bash
make restart-release
pgrep -fl TokensMenuBar
```

Expected: the release build succeeds and `pgrep` reports a running `TokensMenuBar` process launched from the active worktree.

- [ ] **Step 8: Manually verify the approved interaction**

Use report data containing both a short project folder and a folder named `an-extremely-long-project-folder-name-that-will-not-fit-in-the-row`, then verify:

1. Each PROJECT row shows only the final folder component.
2. The long name truncates at the tail with an ellipsis.
3. The cost and token summary remains fully visible.
4. Hovering the name shows the complete folder name and no parent path.
5. `Unattributed` remains unchanged.
6. Expanding nested models announces the folder name rather than the full path.

- [ ] **Step 9: Review the diff for scope and privacy**

Run:

```bash
git diff --check
git diff -- Sources/TokensMenuBarCore/Models.swift Sources/TokensMenuBarCore/Views.swift Tests/TokensMenuBarTests/ProjectUsageTests.swift
```

Expected: no whitespace errors; no report schema, sorting, pagination, panel width, custom tooltip, or full-path UI changes.

- [ ] **Step 10: Commit only after explicit user authorization**

After the user explicitly approves creating a commit, run:

```bash
git add Sources/TokensMenuBarCore/Models.swift Sources/TokensMenuBarCore/Views.swift Tests/TokensMenuBarTests/ProjectUsageTests.swift docs/superpowers/specs/2026-08-04-project-folder-name-display-design.md docs/superpowers/plans/2026-08-04-project-folder-name-display.md
git commit -m "feat: shorten project names in menu bar

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

Expected: one focused commit containing the approved design, implementation plan, implementation, and tests; unrelated `piolium/` content remains untracked and excluded.

---

### Amendment: Claude cwd-based folder labels

**Files:**
- Modify: `cli/tokens-core/src/sessions/claudecode.rs`
- Modify: `cli/tokens-core/src/message_cache.rs` (`ClientId::Claude` parser version 2 → 3)
- Modify: `Sources/TokensMenuBarCore/Models.swift` (`folderName` prefers non-empty `displayName`)
- Modify: `Tests/TokensMenuBarTests/ProjectUsageTests.swift`
- Modify: design + plan docs

**Steps:**
1. Add failing Rust tests for key stability, worktree folder preservation, later-cwd updates, and missing/unusable cwd fallback.
2. Deserialize optional `cwd`, track latest valid normalized label, apply to emitted messages; keep path-derived key.
3. Bump Claude parser version only.
4. Update Swift `folderName` + tests to prefer corrected `displayName`.
5. Run focused Rust/Swift tests, full Swift suite, tokens-core package tests, `make restart-release`, and live `tokens usage --json --period today --refresh`.
