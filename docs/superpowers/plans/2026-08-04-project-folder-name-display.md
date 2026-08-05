# Project Folder Name Display Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show the final project folder name in PROJECT rows, prefer corrected report display names over encoded project keys, truncate long names without displacing usage metrics, and reveal the complete folder name on hover.

**Architecture:** Claude session parsing keeps the path-derived `workspace_key` as identity while deriving `workspace_label` from the latest usable JSONL `cwd`. Duplicate merges preserve the final valid label for timestamp-based project aggregation. Aggregation promotes that label into report `displayName`. The menu bar normalizes attributed project labels in this order: usable `displayName`, usable `projectKey`, then `Unattributed`. Nil project keys remain private and always render `Unattributed`. Report schema fields remain unchanged.

**Tech Stack:** Rust (tokens-core Claude parser and aggregation), Swift 5.9, SwiftUI, XCTest, Swift Package Manager, macOS 13+

## Global Constraints

- Execute implementation work in an isolated git worktree created with `superpowers:using-git-worktrees`.
- Preserve Claude `workspace_key` / report `projectKey` identity from the session path; never replace it with `cwd`.
- For attributed projects, try a usable non-empty `displayName` before `projectKey`; never surface an encoded Claude key while a usable display name exists.
- Treat empty, whitespace-only, root-only, and all-separator names as unusable.
- A nil project key is unattributed, always renders `Unattributed`, and never exposes display-name or path data.
- Do not display the full project path or full cwd in row text, hover text, or accessibility text.
- Keep project names on one line with tail truncation; cost and token values retain layout priority.
- Increment only the Claude per-client parser version for cache invalidation.
- Do not change report schema, project identity, aggregation rules, sorting, pagination, panel width, cost/token totals, or day attribution.
- Do not create commits unless the user explicitly authorizes them during execution.

## File Structure

- `cli/tokens-core/src/sessions/claudecode.rs` — deserialize optional `cwd`, derive labels, merge duplicate labels safely, and own focused parser-to-aggregation regressions.
- `cli/tokens-core/src/message_cache.rs` — increment the Claude-only parser version so cached transcripts are reparsed.
- `Sources/TokensMenuBarCore/Models.swift` — normalize attributed project names by trying usable `displayName`, then usable `projectKey`, then `Unattributed`.
- `Sources/TokensMenuBarCore/Views.swift` — render the normalized name with truncation, system hover help, and accessible copy.
- `Tests/TokensMenuBarTests/ProjectUsageTests.swift` — verify display-name priority, normalization fallbacks, long names, and unattributed privacy.
- `docs/superpowers/specs/2026-08-04-project-folder-name-display-design.md` — authoritative behavior and scope.
- `docs/superpowers/plans/2026-08-04-project-folder-name-display.md` — integrated implementation and verification sequence.

---

### Task 1: Derive Stable Claude Workspace Labels

**Files:**
- Modify: `cli/tokens-core/src/sessions/claudecode.rs`
- Modify: `cli/tokens-core/src/message_cache.rs`

**Interfaces:**
- Consumes: optional Claude JSONL entry `cwd`, path-derived workspace identity, message/request dedup keys, and message timestamps.
- Produces: `UnifiedMessage.workspace_label` containing a usable final folder name while leaving `workspace_key`, timestamps, dates, token counts, costs, and message counts unchanged.

- [ ] **Step 1: Add focused failing Rust regressions**

Add tests covering:

1. An encoded Claude project key remains unchanged while a valid cwd produces its final folder label.
2. A hyphenated worktree cwd preserves the complete folder name.
3. A later valid cwd updates labels for later messages without changing project identity.
4. Missing, whitespace-only, or root-only cwd falls back to the path-derived label.
5. An assistant duplicate carrying a later valid cwd refreshes the deduplicated message label.
6. A tool-result duplicate carrying a later valid cwd refreshes the deduplicated message label without double-counting tokens.
7. End-to-end ordering: older message A, newer message B, then a late duplicate of A carrying the final cwd label. Feed parsed messages through `aggregate_by_date` and assert the project label is final while project key, total tokens, total messages, and day count remain unchanged.

- [ ] **Step 2: Run the focused Rust tests and verify RED**

Run:

```bash
cargo test --manifest-path cli/Cargo.toml -p tokens-core --lib sessions::claudecode::tests -- --nocapture
```

Expected before implementation: the cwd/duplicate regressions fail because Claude entries do not yet provide stable final labels through deduplication and timestamp-based aggregation.

- [ ] **Step 3: Deserialize and normalize optional cwd labels**

In `ClaudeEntry`, deserialize optional `cwd`.

During parsing:

1. Derive `workspace_key` and the initial fallback label from the transcript path.
2. Normalize each usable cwd using existing workspace normalization helpers.
3. Extract only its final non-empty folder component into `workspace_label`.
4. Keep the path-derived `workspace_key` unchanged.
5. Ignore unusable cwd values so the previous valid or path-derived label remains available.

Apply the current label to assistant, tool-result, and headless messages without altering report fields or message accounting.

- [ ] **Step 4: Preserve label ordering across duplicate merges**

When a late duplicate supplies a valid label:

1. Merge tokens and timing with the existing dedup behavior.
2. Refresh the duplicate's label.
3. Refresh already-emitted messages in the same attributed workspace whose timestamps could otherwise outrank the duplicate during aggregation.
4. Do not rewrite timestamps, derived dates, tokens, costs, message counts, or workspace identity.

Reuse the existing duplicate maps and message vector; do not add report fields or change aggregation ordering.

- [ ] **Step 5: Invalidate only Claude parser caches**

Increment `ClientId::Claude` in `parser_version()` from 2 to 3 and document that v3 adds cwd-derived labels. Do not increment `CACHE_FORMAT_VERSION` or another client's parser version.

- [ ] **Step 6: Run focused Rust tests and verify GREEN**

Run:

```bash
cargo test --manifest-path cli/Cargo.toml -p tokens-core --lib sessions::claudecode::tests -- --nocapture
```

Expected: all seven Claude parser tests pass, including the parser-to-aggregation late-duplicate regression.

---

### Task 2: Normalize Safe Project Folder Names in Swift

**Files:**
- Create: `Tests/TokensMenuBarTests/ProjectUsageTests.swift`
- Modify: `Sources/TokensMenuBarCore/Models.swift`

**Interfaces:**
- Consumes: `ProjectUsage.displayName: String` and `ProjectUsage.projectKey: String?` from decoded usage reports.
- Produces: `ProjectUsage.folderName: String`, a non-empty presentation value.

- [ ] **Step 1: Add focused failing Swift tests**

Create `ProjectUsageTests` covering these eleven cases:

1. A path-shaped non-empty `displayName` returns its final folder component.
2. A usable `displayName` wins over an encoded `projectKey`.
3. A usable `displayName` wins when the key has trailing separators.
4. A complete long `displayName` is preserved for UI truncation and hover use.
5. Empty `displayName` falls back to the final usable key component.
6. Whitespace-only `displayName` falls back to the final usable key component.
7. Root-only `displayName` falls back to the final usable key component.
8. An unusable key does not override a usable `displayName`.
9. Nil `projectKey` always returns `Unattributed`, even if `displayName` contains a path.
10. Empty names return `Unattributed`.
11. Whitespace/all-separator `displayName` and `projectKey` values return `Unattributed`.

- [ ] **Step 2: Run the focused Swift tests and verify RED**

Run:

```bash
swift test --filter ProjectUsageTests
```

Expected before normalization: root-only and whitespace-only values can be returned as visible labels instead of falling back.

- [ ] **Step 3: Add one private normalization path**

Inside `ProjectUsage`:

1. Return `Unattributed` immediately when `projectKey == nil` so display-name/path data remains private.
2. Use one private helper to trim whitespace, normalize path separators, discard empty/all-separator inputs, and return the final usable component.
3. Try the helper with `displayName` first.
4. If unusable, try the helper with the non-nil `projectKey`.
5. If both are unusable, return `Unattributed`.

Keep stored properties, `id`, Codable behavior, and report schema unchanged.

- [ ] **Step 4: Run focused Swift tests and verify GREEN**

Run:

```bash
swift test --filter ProjectUsageTests
```

Expected: all eleven `ProjectUsageTests` pass.

---

### Task 3: Render the Normalized Name

**Files:**
- Modify: `Sources/TokensMenuBarCore/Views.swift`

- [ ] **Step 1: Bind the normalized presentation value once**

In `projectRow(_:)`, bind:

```swift
let folderName = project.folderName
```

Do not build visible strings directly from `projectKey` or `displayName` in the view.

- [ ] **Step 2: Apply row truncation and standard hover help**

Render `folderName` with the existing monospaced project-row styling plus:

```swift
.lineLimit(1)
.truncationMode(.tail)
.frame(minWidth: 0, maxWidth: .infinity, alignment: .leading)
.help(folderName)
.accessibilityLabel(folderName)
```

Keep the cost/token summary's existing `.layoutPriority(2)` unchanged.

- [ ] **Step 3: Use the same safe name for nested-model accessibility**

Pass:

```swift
accessibilityNoun: "models for \(folderName)"
```

The visible label, hover help, accessibility label, and expansion noun must all use the same normalized folder name.

---

### Task 4: Verify Behavior, Scope, and Documentation

- [ ] **Step 1: Run the complete targeted and full suites**

Run:

```bash
cargo test --manifest-path cli/Cargo.toml -p tokens-core --lib
swift test
```

Expected: all tests pass with zero failures.

- [ ] **Step 2: Build and relaunch the release app**

Run:

```bash
make restart-release
pgrep -fl TokensMenuBar
```

Expected: the release build succeeds and the menu bar process is running from the active worktree.

- [ ] **Step 3: Verify the approved interaction**

Using report data with a short folder, a long folder, an encoded Claude key plus corrected `displayName`, and an unattributed project, verify:

1. PROJECT rows prefer usable `displayName` before `projectKey`.
2. Only the final folder component is visible.
3. Long names truncate at the tail while cost and token values remain visible.
4. Hover and accessibility expose the complete folder name, never the parent path or cwd.
5. Root-only/whitespace values fall through to the next usable source.
6. Nil keys always remain `Unattributed`.
7. Nested-model accessibility uses the same folder name.

- [ ] **Step 4: Review the diff for scope and whitespace**

Run:

```bash
git diff --check origin/main...HEAD
git diff origin/main...HEAD -- \
  cli/tokens-core/src/sessions/claudecode.rs \
  cli/tokens-core/src/message_cache.rs \
  Sources/TokensMenuBarCore/Models.swift \
  Sources/TokensMenuBarCore/Views.swift \
  Tests/TokensMenuBarTests/ProjectUsageTests.swift \
  docs/superpowers/specs/2026-08-04-project-folder-name-display-design.md \
  docs/superpowers/plans/2026-08-04-project-folder-name-display.md
```

Expected: no whitespace errors and no unrelated schema, identity, aggregation, sorting, pagination, panel-width, total, timestamp, or day-attribution changes.

- [ ] **Step 5: Commit only after explicit user authorization**

Stage only the seven files listed above and create one focused conventional commit ending with:

```text
Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
```
