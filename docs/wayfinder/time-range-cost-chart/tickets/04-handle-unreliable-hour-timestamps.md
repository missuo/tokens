---
title: Decide how hourly charts handle unreliable timestamps
tracker: local-markdown
kind: wayfinder-ticket
status: closed
assignee: null
parent: ../map.md
labels:
  - wayfinder:grilling
blocked_by: []
resolution_comment: null
---

## Question

When usage has a reliable calendar date but no trustworthy event time, how should the single-day hourly chart preserve data integrity without falsely assigning the cost to a specific hour or silently making the chart disagree with the dashboard total?

## Resolution

Implemented in PR #8. Daily totals still include unplaced hourly usage; the hourly chart keeps it out of hour buckets via `unplaced_for_hourly` / `timeSeries.unplaced` so chart bars never invent a false hour. See `cli/tokens-cli/src/commands/usage_snapshot.rs` (hour placement + unplaced), `usage_report_v3.rs` (summary conservation with unplaced), and the ReportV3 / plan tests.
