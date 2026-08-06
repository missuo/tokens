---
title: Finalize loading, empty, error, and invalid-range states
tracker: local-markdown
kind: wayfinder-ticket
status: closed
assignee: null
parent: ../map.md
labels:
  - wayfinder:grilling
blocked_by:
  - 01-validate-range-control-and-chart-density.md
  - 02b-define-bucketed-report-and-cache-contract.md
  - 04-handle-unreliable-hour-timestamps.md
  - 07-identify-incomplete-and-active-buckets.md
resolution_comment: null
---

## Question

What should the user see while a newly selected time range is loading, when it contains no usage, when only stale matching data exists, when cache refresh fails, when a previously selected Custom range is no longer valid, and when single-day usage contributes to totals but cannot be placed in a trustworthy hour?

## Resolution

Implemented in PR #8 via stale-while-revalidate in `Sources/TokensMenuBarCore/UsageStore.swift` (keep prior matching report, surface stale/error explicitly, reject late mismatched responses) and panel empty/loading copy in `Views.swift` / `CostChartView.swift`. Covered by `UsageStoreTests` and chart empty-state rendering.
