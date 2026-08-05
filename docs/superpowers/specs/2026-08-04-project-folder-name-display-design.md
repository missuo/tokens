# Project Folder Name Display Design

## Goal

Keep project rows compact and readable by showing only the final folder name instead of a full project path or Claude directory slug. When the folder name does not fit, truncate it and reveal the complete folder name on hover.

## Scope

This change covers:

1. Claude session label correction in the CLI parser (cwd-derived short labels).
2. Menu bar PROJECT row presentation of those labels with tail truncation and hover help.

It does not change project identity, aggregation, sorting, costs, token counts, nested model rows, or the report schema fields.

## Display Rules

1. For an attributed project, prefer a non-empty report `displayName` and show only its final non-empty folder component when the value looks like a path; otherwise show the display name as-is.
2. When `displayName` is empty, fall back to the final `/`-separated component of `projectKey` if usable.
3. Do not display the full project path or full cwd in the row, hover text, or accessibility text.
4. If the final folder name exceeds the available row width, keep it on one line and truncate the overflowing portion with an ellipsis (tail truncation).
5. Hovering the project name shows the complete, untruncated final folder name using the platform's standard help tooltip.
6. Preserve enough layout priority for the cost and token summary so project names cannot push those values out of the row.
7. For an unattributed project (`projectKey == nil`), continue to display `Unattributed` and do not expose workspace or path information.
8. If both names are empty/unusable, use `Unattributed`.

## Data Flow

1. Claude session files under `~/.claude/projects/<slug>/` keep the path-derived `workspace_key` / report `projectKey` as project identity. The key is never replaced by `cwd`.
2. While parsing Claude JSONL, the parser reads optional `cwd` on entries, normalizes it, and tracks the latest valid final-folder label.
3. Emitted Claude messages keep the path-derived key and use the cwd-derived `workspace_label` when available; missing/unusable cwd falls back to the existing path-derived label (often the encoded slug).
4. Aggregation promotes the latest non-empty label into report `displayName`.
5. The menu bar presentation layer (`ProjectUsage.folderName`) prefers non-empty `displayName` over deriving text from an encoded `projectKey`.

## Claude Parser Cache Invalidation

Increment only the Claude per-client parser version so cached Claude transcripts are reparsed with cwd labels. Do not increment the cross-client cache format version.

## Interaction and Accessibility

The hover tooltip contains only the full folder name. The accessible label exposes the complete folder name without revealing the full path or cwd. Nested model expansion nouns also use the short folder name.

## Error and Edge-Case Handling

- Ignore trailing path separators when selecting the final folder component.
- Treat `/` as the path separator for filesystem paths and cwd values.
- Treat empty, root-only, or otherwise unusable cwd values as missing.
- Support sessions whose cwd changes during the file (including relocation into `.claude/worktrees/<folder>`); later messages use the latest valid cwd label.
- Keep duplicate final folder names unchanged; distinguishing same-named projects by exposing paths is outside this change.

## Testing

### Rust (Claude parser)

- Encoded Claude project key remains unchanged while `workspace_label` becomes the final folder from `cwd`.
- A cwd ending in a hyphenated worktree folder preserves the complete folder name.
- A later valid cwd updates the label used by later emitted messages.
- Missing or unusable cwd falls back to the existing path-derived label.

### Swift (presentation)

- Non-empty display names win over encoded project keys.
- Absolute-path display names reduce to final folder components.
- Long folder names remain available in full for tooltip and accessibility use.
- Unattributed projects remain `Unattributed` and never expose path data.

Run focused Rust tests, focused Swift tests, the full Swift suite, the relevant Rust package suite, rebuild/relaunch the menu bar, and verify live `tokens usage --json --period today --refresh` shows encoded keys paired with short final-folder display names.

## Out of Scope

- Showing a full project path or full cwd anywhere in the panel;
- replacing Claude `projectKey` identity with cwd;
- adding custom tooltip styling;
- changing panel width or pagination;
- changing project identity, grouping, or sorting;
- disambiguating projects with identical final folder names;
- decoding Claude directory slugs heuristically.
