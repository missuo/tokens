# AI Agent Guidelines

This document provides context for AI agents working on the Tokscale project.

## Maintaining AGENTS.md Files

When updating AGENTS.md files, follow these principles:

- **No hardcoded counts** — Don't write "10 crates" or "5 modules"; these become outdated instantly
- **No exhaustive lists** — Prefer dynamic commands (`ls crates/`) over maintaining complete lists
- **Document constraints, not descriptions** — Focus on non-obvious behaviors, gotchas, and cross-crate dependencies
- **Use nested AGENTS.md** — Place crate-specific details in `crates/{name}/AGENTS.md`, not here
- **Verify before documenting** — Grep/read the code to confirm claims are accurate
- **Delete outdated info** — Outdated docs are worse than no docs

## Authoring PR/Issue Content via `gh` CLI

When writing PR bodies, issue bodies, or comments through `gh pr create`, `gh issue comment`, etc.: **prefer `--body-file` with a written-out file over inline `--body` heredocs.**

Why: inline `--body "$(cat <<'EOF' ... EOF)"` patterns lead to recurring mistakes where backticks are written as `` \` `` (incorrectly escaping them inside a single-quoted heredoc where no escaping is needed). The literal `` \` `` then renders in GitHub markdown as backslash + backtick instead of inline code formatting.

**DO:**

- Write the body to `/tmp/pr-body.md` (or similar) with the `Write` tool, then `gh pr create --body-file /tmp/pr-body.md`
- For comment edits: `gh api -X PATCH repos/<owner>/<repo>/issues/comments/<id> -F body=@/tmp/comment.md`

**DON'T:**

- Use `gh pr create --body "$(cat <<'EOF' ... \` ... \` ... EOF)"` — single-quoted heredoc means backticks are already literal; escaping with `` \` `` is wrong and renders incorrectly.
- Use double-quoted heredoc for bodies containing backticks unless you intend command substitution.
- Hard-wrap paragraphs at ~80 columns inside PR/issue/comment bodies. GitHub's markdown collapses single newlines into spaces so the rendered output looks fine, but the **raw markdown view is what reviewers and authors edit in**, and mid-sentence line breaks read as if the prose was chopped. Write each paragraph as one continuous line and let the renderer wrap it. Same rule for blockquotes, list items that span multiple lines, and table cells. Hard wraps are still fine inside fenced code blocks where preserving line layout matters.

This applies to all GitHub-content authoring through the CLI — PR bodies, issue bodies, comments, edits. Commit message bodies should also follow this rule: write prose paragraphs as continuous lines, not hard-wrapped at 80 columns.

## Git Identity & Merge Discipline

- Before making any commit, verify the local git identity is exactly `Junho Yeo <i@junho.io>`. If it is not, set `git config user.name "Junho Yeo"` and `git config user.email "i@junho.io"` before committing.
- Never commit as worker/agent identities such as `worker1`, `worker2`, `worker3`, or `*@example.invalid`.
- When merging pull requests through `gh`, use squash merge (`gh pr merge --squash ...`) unless the user explicitly requests another merge strategy.
- Before merging, verify the squash commit title is the intended conventional PR title and does not contain worker/agent/internal review jargon.

## Commit Message Convention

```
<type>: <description>

[optional body]
```

### Types

| Type | Description |
|------|-------------|
| `feat` | New feature |
| `fix` | Bug fix |
| `refactor` | Code refactoring (no behavior change) |
| `docs` | Documentation only |
| `test` | Adding or updating tests |
| `chore` | Maintenance tasks |
| `perf` | Performance improvements |

### Examples

```
feat: add session branching with /fork command
fix: handle empty response from provider
refactor: extract streaming logic to separate module
docs: update README with new CLI options
```

### Commit Message & PR Title Rules (CRITICAL)

> These rules apply to **both commit messages AND pull request titles**. PR titles become the squash-merge commit message, so they must follow the same conventions.

**DO:**
- Describe the actual change in plain, technical terms
- Keep commits atomic (one logical change per commit)
- Use the format: `<type>(<scope>): <what changed and why>`

**DON'T:**
- Reference internal review labels (P0, P1, P2, etc.) in commits or PR titles
- Mention "Oracle", "audit", "review findings", "hardening" in commits or PR titles
- Use agent-internal jargon: "wave", "hardening", "compliance", "verification pass"
- Bundle multiple unrelated fixes into one commit
- Use vague messages like "fix issues" or "address feedback"

**Good Examples:**
```
fix(lsp): pass server args to stdio spawn command
fix(lsp): convert 1-indexed input lines to 0-indexed LSP positions
fix(gemini): parse SSE data frames instead of raw JSON lines
fix(orchestrator): route provider tools through approval flow
```

**Bad Examples (NEVER do this):**
```
fix: address P0 issues from Oracle review      ❌
fix(hardening): Oracle Round 4 fixes           ❌
fix: audit findings                            ❌
fix: various improvements                      ❌
fix(tui): harden unreleased changes — P0-P3    ❌  (PR title)
fix: hardening wave 1 compliance fixes         ❌  (PR title)
```

## Migration journal hygiene

Never hand-edit `drizzle/meta/_journal.json` timestamps or sequence numbers. Always run `drizzle-kit generate` to claim a migration slot — the tool assigns the correct monotonic index and timestamp atomically.

Migrations 0010 and 0011 have round-number hand-edited timestamps (`"when": 1780000000000` and `"when": 1780086400000`) as a one-time historical exception made during the 2026-05-25 schema audit. No future migration should follow this pattern; use `drizzle-kit generate` exclusively.


If two branches generate migrations with the same index, resolve the conflict by re-running `drizzle-kit generate` on the branch that was merged later — do not manually renumber files or edit `_journal.json`.

## Agent Command Execution

- When running `tokens` CLI commands from an automated agent (tests, CI, or tool-driven shells), always pass `--no-spinner` unless spinner behavior is the thing being tested.
- This avoids non-interactive terminal issues and keeps command output stable for assertions and logs.

## Release & Deployment

### Overview

Releases are published to npm via a GitHub Actions `workflow_dispatch` pipeline, followed by a manually created GitHub Release with handwritten notes. There is no staging environment — publishes go directly to npm `latest`.

### Release Pipeline

**Workflow:** `.github/workflows/publish-cli.yml`

**Trigger:** Manual — GitHub Actions UI → "Publish" → "Run workflow"

**Inputs:**
- `bump`: Version bump type — `patch (x.x.X)` | `minor (x.X.0)` | `major (X.0.0)`
- `version` (optional): Override string (e.g., `2.0.0-beta.1`), takes precedence over bump

**Stages (sequential):**

| # | Job | Description |
|---|-----|-------------|
| 1 | `bump-versions` | Reads current version from `packages/cli/package.json`, calculates new version, updates the Rust workspace version plus the CLI and platform package manifests, then uploads the bumped manifests as an artifact |
| 2 | `build-cli-binary` | 8-target parallel native Rust builds (macOS x86/arm64, Linux glibc/musl x86/arm64, Windows x86/arm64); produces the `tokens` binary (and `tokens.exe` on Windows) per Cargo `[[bin]]` |
| 3 | `prepare-release-provenance` | Pre-flight npm publish check via `scripts/check-npm-release-state.sh`, then commits the bumped manifests back to the release branch as `chore: bump version to X.Y.Z` (authored by `github-actions[bot]`); all subsequent jobs check out this commit |
| 4 | `publish-platform-packages` | Publishes the 8 platform-specific packages (`@tokens/cli-darwin-arm64`, etc.) containing native binaries to npm |
| 5 | `publish-cli` | Builds `packages/cli/` and publishes `@tokens/cli` to npm (binary dispatcher + 8 optionalDependencies) |
| 6 | `finalize` | Creates the `vX.Y.Z` git tag, generates release notes via `scripts/generate-release-notes.ts`, opens/updates the GitHub Release, and (best-effort) posts to Discord via `scripts/post-discord-release.sh` |

**Duration:** ~15-20 minutes end-to-end.

**Package publish chain:** 8 × `@tokens/cli-{triple}` (platform binaries) → `@tokens/cli` (dispatcher that picks the right platform package at install time). No wrapper is published — `bunx @tokens/cli` puts the `tokens` binary on `PATH` directly because the dispatcher's own `bin: { "tokens": "./bin.js" }` already exposes that command name. (npm has reserved the unscoped `tokens` name since 2014 for an unrelated OAuth library, so an unscoped wrapper would not be reachable anyway.)

### Required Secrets

| Secret | Used by | Purpose |
|---|---|---|
| `NPM_TOKEN` | `check-npm-release-state`, `publish-platform-packages`, `publish-cli` | Publishes the packages and authenticates the pre-flight `npm view` lookups. Must be an automation token with publish rights on the `@tokens/*` scope. |
| `GITHUB_TOKEN` | `prepare-release-provenance`, `finalize` | Default token; pushes the `chore: bump version` commit, creates the tag, and creates/updates the GitHub Release. |
| `DISCORD_RELEASE_WEBHOOK_URL` (optional) | `finalize` | If set, the release notes are also posted to a Discord webhook. The step is `continue-on-error: true` and silently no-ops when the secret is missing, so a release never fails just because the webhook is unconfigured. |

### Post-Pipeline

The `finalize` job handles tagging, GitHub Release, and Discord — there is no separate manual step. After the workflow finishes, verify on npm and in the GitHub Releases tab; smoke-test the install with `bunx @tokens/cli@latest --version`.

### Versioning Conventions

| Bump Type | When to Use | Example |
|-----------|-------------|---------|
| `patch` | Bug fixes, small features, additive parser support | `1.2.0` → `1.2.1` |
| `minor` | New client support, significant features, UI overhauls | `1.1.2` → `1.2.0` |
| `major` | Breaking changes (never used so far) | `1.2.1` → `2.0.0` |

Release version is stored in the Rust workspace and the npm package manifests, and CI updates them together:
- `Cargo.toml` (`[workspace.package].version`) — Rust binary and exported metadata version
- `packages/cli/package.json` — dispatcher package version and platform optional dependency versions
- Platform packages (`packages/cli-*/package.json`) — native package versions

### CI-Only Workflows

- **`.github/workflows/release.yml`** — Triggered on every `v*` tag push. Cross-compiles the `tokens` binary for 4 targets (macOS aarch64/x86_64, Linux x86_64, Linux aarch64), tars + sha256s each artifact, and uploads them to the GitHub Release so `install.sh` can fetch a prebuilt binary. The remaining 4 targets (musl + Windows) are covered by the npm publish pipeline's build matrix, since they are only relevant to npm consumers.

---

### Release Notes Style

#### Title Conventions

| Release Type | Title Format |
|-------------|--------------|
| Standard patch/minor | `` `tokens@vX.Y.Z` is here! `` |
| Flagship feature | `` EMOJI `tokens@vX.Y.Z` is here! (Short subtitle with [link](...)) `` |
| Feature spotlight | Custom banner image replacing the standard hero + call-to-action |

**Examples from past releases:**
- Standard: `` `tokens@v3.0.0` is here! ``
- Flagship: `` 🚀 `tokens@v3.1.0` is here! (Adds [Foo](https://github.com/foo/bar)) ``
- Spotlight: Custom banner + `` Generate your Wrapped 2025 with `tokens@v1.0.16` ``

#### Release Notes Template

```markdown
<div align="center">

[![Tokens](https://github.com/missuo/tokens/raw/main/.github/assets/hero-v2.png)](https://github.com/missuo/tokens)

# `tokens@vX.Y.Z` is here!
</div>

## What's Changed
* scope(area): description by @author in https://github.com/missuo/tokens/pull/NNN
* scope(area): description by @author in https://github.com/missuo/tokens/pull/NNN

## New Contributors
* @username made their first contribution in https://github.com/missuo/tokens/pull/NNN

**Full Changelog**: https://github.com/missuo/tokens/compare/vPREVIOUS...vNEW
```

#### Style Rules

| Element | Rule |
|---------|------|
| **Header** | Always centered `<div align="center">` with hero banner image linked to the repo |
| **Title** | Backtick-wrapped `tokens@vX.Y.Z` — package name, not just version |
| **PR list** | `* scope(area): description by @author in URL` — mirrors the PR title exactly as merged |
| **Optional summary** | For releases with many changes or when PR titles alone don't convey impact, add a brief bullet list between the title and "What's Changed" |
| **New Contributors** | Include section when there are first-time contributors |
| **Full Changelog** | Always present at bottom as a GitHub compare link `vPREV...vNEW` |
| **Tone** | Concise. No prose paragraphs. Let the PR list speak for itself. |
| **No draft issues** | Never reference draft release issues (e.g., #1) in the notes |

#### When to Add a Summary Block

Add a short bullet list summary (before "What's Changed") when:
- The release has 4+ PRs spanning different areas
- PR titles alone don't convey the user-facing impact
- A new client/integration is the headline

**Example:**
```markdown
- Improved model price resolver (Rust)
- Add support for Amp (AmpCode) and Droid (Factory Droid)
- Improved sorting feature on TUI
```

### Deployment Checklist

```
1. [ ] All target PRs merged to main
2. [ ] `cargo test` passes in crates/tokscale-cli
3. [ ] No open blocker bugs (regressions from changes being released)
4. [ ] Run "Publish" workflow via GitHub Actions UI
   - Select bump type (patch/minor/major) or set `version` to an override
   - Wait for all 6 stages to complete
5. [ ] Verify `chore: bump version to X.Y.Z` commit was pushed by CI
6. [ ] Verify packages on npm under the @tokens scope: @tokens/cli and the 8 @tokens/cli-{triple} packages
7. [ ] Verify the GitHub Release was created/updated by the finalize job (tag vX.Y.Z)
8. [ ] Smoke test: bunx @tokens/cli@latest --version
```
