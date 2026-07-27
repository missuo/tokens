# Upstream Policy

Tokens is a fork of [Tokscale](https://github.com/junhoyeo/tokscale) and keeps
taking from it. This document says what we take, what we refuse, and what has to
be looked at by a human before it lands.

It exists because the answer is not obvious per commit. Upstream ships parser
fixes we want and UI we do not, often in the same pull request, and a reviewer
without a rule will merge whichever is easier to merge.

## What the fork is now

The refactor is finished. Its shape decides most of the rules below.

| | Upstream | Here |
|---|---|---|
| CLI | Full TUI dashboard plus report commands (`models`, `monthly`, `hourly`, `graph`, `wrapped`, `pricing`, …) | Submit only — `login`, `submit`, `serve`, `status`, plus per-provider sync. The TUI and every report command are gone, ~11k lines and fifteen dependencies with them |
| Reporting | In the terminal | On the web, where it can be linked and compared |
| Hosting | Self-hosted Docker, Postgres in a container on the same host | Cloudflare Workers via OpenNext; Aiven Postgres behind Hyperdrive, Worker pinned to the database's region |
| Caching | — | R2 for rendered pages, Durable Objects for tag invalidation, explicit edge caching for image endpoints |
| Frontend | Upstream's components | Rebuilt on shadcn/ui with its own brand marks and per-page Open Graph cards |
| Anti-cheat | — | Cross-device duplicate guard, resubmit monotonicity checks, account bans, public Hall of Shame |
| Identity | Username | Verified badge from two or more GitHub social links, refreshed daily |
| Groups | Team/group leaderboards | Removed — one global ranking |
| Tests | Present | Removed |
| Repository layout | `crates/`, `packages/frontend/` | `cli/`, `web/` |

Two consequences follow directly:

- Any upstream commit touching the TUI, a report command, groups, or the tests
  is not just unwanted — it will not apply.
- Any upstream commit touching `crates/` or `packages/frontend/` needs its paths
  translated before it can be evaluated at all.

## The rule

| Category | Decision | Examples |
|---|---|---|
| Branding, naming, domains, copy | **Never merge** | Anything saying `tokscale`; logos; upstream's marketing text |
| New provider, new client scanner, parser fix | **Always merge** | Support for a new IDE or CLI; a corrected token field |
| Frontend **data capability** | **Merge the capability, rewrite the implementation** | A new chart dimension or statistic → take the data logic, redraw it with our components |
| Frontend **styling, components, layout** | **Never merge** | Upstream's styled-components, HeroUI usage, colours, spacing |
| CLI display, interaction, reporting features | **Skip** | TUI themes, prettier tables, wrapped images |
| Submit pipeline, security, correctness | **Always merge** | Parser overflow, double-counting, dedup |
| Database migrations | **Review each one by hand** | See below |
| Anything unclear | **Open a draft PR listing the commits and ask** | — |

### Why frontend implementations never come across

Take the capability, not the code. An upstream PR that adds a data dimension
*and* its own styling should be read for the data logic and then reimplemented
in our component system — not cherry-picked.

Concretely:

- Components come from `web/src/components/ui/` (shadcn, vendored here). No new
  third-party component library.
- Colours go through semantic tokens (`bg-background`, `text-muted-foreground`,
  `border`). No hardcoded hex, no manual `dark:` branches — both themes must
  fall out of the tokens.
- Numbers use `.tabular` so columns line up.
- No new styled-components usage; the remaining instances go as pages are
  rewritten.

The reason is narrow: let upstream styling in once and the site carries two
design languages at the same time. Every later sync deepens the split, and
eventually nobody can say which one is correct. Hold this line and upstream
stays an input to our data capability rather than a source of design debt.

### Why migrations are reviewed by hand

A migration that is correct upstream can be wrong here, because the data is not
the same shape. On 2026-07-24 a sync-related change produced cross-device double
counting in production; the repair left four backup tables behind. Read what a
migration does to existing rows, not just what it does to the schema.

Migrations are additive by convention. A migration that drops or rewrites data
needs a plan for the rows already in the table before it is merged.

## How to sync

1. Branch from `main`, cherry-pick with `-x` so the origin commit is recorded.
2. Translate paths: `crates/tokscale-core` → `cli/tokens-core`,
   `packages/frontend` → `web`.
3. List skipped commits in the pull request body with a reason for each. A skip
   without a reason becomes a mystery the next time someone syncs.
4. Run `cargo check --manifest-path cli/Cargo.toml --workspace --all-targets`
   and `bun run typecheck` in `web/`. There are no tests; these are the only
   automated checks left.

### When a fix cannot be cherry-picked

The paths often conflict. If our copy of a file is otherwise identical to
upstream's, the fastest correct route is to take upstream's post-fix file whole
and strip what we do not carry.

That is how the Grok `turn_completed` parser fix landed: our production code was
byte-identical to upstream's before the fix, the only difference being the
inline test module this fork no longer has. Taking the fixed file and removing
that module produced exactly the change, with no conflict resolution at all.

## What this policy does not cover

Upstream's release process, its CI, and its packaging are not tracked. Our
release pipeline is ours — it publishes different platform packages, from a
different workspace path, without the TUI's optional features.
