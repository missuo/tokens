# Agent Working Guidelines

## Core Principles

- Choose the smallest workflow that safely matches the task's risk and uncertainty.
- Classify the workflow autonomously; do not ask the user to choose a workflow level or answer routine classification questions.
- Keep implementation and documentation consistent. Remove or replace obsolete statements instead of leaving contradictory old and new descriptions together.
- Ask the user only when a genuine product decision cannot be inferred, requirements conflict, or an irreversible or outward-facing action requires authorization.

## Workflow Classification

### Fast Path

Use the fast path when all six answers are yes:

1. Is the requirement unambiguous?
2. Is the change mechanical, such as ordering, copy, styling, or a small configuration adjustment?
3. Is the implementation limited to roughly one or two source/configuration files? Required documentation updates do not count against this limit.
4. Does the change avoid public APIs, persisted data, schemas, authentication, authorization, security, privacy, payments, concurrency, and migrations?
5. Is the change easy to reverse?
6. Can correctness be demonstrated with a focused build, targeted tests, a structural check, or a small smoke test?

The agent answers these questions internally. If all are yes, proceed immediately:

1. Inspect the relevant implementation and existing documentation.
2. Make the smallest implementation change.
3. Update all affected documentation in the same change: delete stale requirements, replace obsolete examples or ordering, and add the new behavior where needed. Do not ask the user to approve routine documentation synchronization.
4. Run proportional verification and `git diff --check`.
5. Report the result, evidence, and remaining risk concisely.

Skip brainstorming, separate design documents, detailed implementation plans, formal TDD ceremony, subagents, multi-round reviews, and exhaustive GUI automation unless the user requests them or a concrete risk requires them. Use a worktree when requested or when the session is already isolated.

### Standard Workflow

Use a standard workflow when the change remains local and reversible but one or more fast-path answers are no. Use only the minimum additional process needed: a short approach, implementation, relevant tests, one review, and a smoke test when necessary. Do not automatically create design and plan documents.

### Full Workflow

Use a full design and verification workflow for:

- public API or protocol compatibility;
- persisted data, schemas, caches, or migrations;
- authentication, authorization, security, privacy, or payments;
- destructive or difficult-to-reverse operations;
- concurrency, transactions, or distributed state;
- cross-module or cross-service architecture;
- substantial ambiguity requiring a product decision;
- deployment, rollout, monitoring, or rollback planning;
- failures that could cause data loss or service interruption.

## Proportional Verification

Match verification cost to risk:

- Copy, ordering, and small style changes: structural/diff check plus compilation or a targeted test.
- Local behavior changes: relevant unit tests plus a complete build.
- User interaction changes: add one real-application smoke test when static checks cannot establish correctness.
- Data, API, security, migration, or release changes: full tests, integration/E2E coverage, and rollback validation where applicable.

Do not perform exhaustive GUI automation merely because a UI exists. Prefer versioned, deterministic GUI scripts over one-off AI-driven interaction. When reusable Tart-based isolation is available, use one disposable VM clone per GUI job or test-suite shard, not per individual test. Keep XCUITest as the primary driver, use a small Accessibility fallback only when necessary, and reserve AI for initial test authoring, failure diagnosis, Accessibility changes, and visual-difference review. Keep true multi-display behavior in a separate controlled real-machine suite.

## Multi-Agent Workflow

Use subagents or workflow orchestration only for at least three genuinely independent tasks with limited overlap, a broad audit or migration, or multi-subsystem research where parallel work materially reduces time. Sequential tasks sharing files or decisions are not independent.

Default to a single agent for mechanical changes, ordering, copy edits, documentation synchronization, and any task where coordination and repeated review would cost more than implementation.

## Git Remotes and Pull Requests

Before pushing or creating a pull request, inspect remotes and identify repository ownership explicitly. The default destination is the user's personal repository, normally `origin`. Do not push to an upstream project, another person's repository, or another fork unless explicitly requested.

Create pull requests inside the user's personal repository by default. Specify the repository, base branch, and head branch explicitly rather than relying on automatic inference.
