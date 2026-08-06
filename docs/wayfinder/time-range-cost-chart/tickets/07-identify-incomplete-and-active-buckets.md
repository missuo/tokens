---
title: Decide how incomplete and active chart buckets are identified
tracker: local-markdown
kind: wayfinder-ticket
status: closed
assignee: null
parent: ../map.md
labels:
  - wayfinder:grilling
blocked_by:
  - 01-validate-range-control-and-chart-density.md
  - 02-define-unified-range-report-contract.md
  - 02b-define-bucketed-report-and-cache-contract.md
resolution_comment: ../resolutions/02-define-unified-range-report-contract.md
---

## Question

How should incomplete edge chart buckets and the active bucket be identified in the Cost chart and related copy so users can recognize partial data without the chart appearing to disagree with dashboard totals?

## Resolution

Data contract (resolution 02): each bucket carries `active` and `incompleteEdge` flags on `UsageBucketMetadata` / `UsageReportTimeBucket` in `cli/tokens-cli/src/commands/usage_report_v3.rs`. Trailing clipped hour uses `active` + clipped `coveredEndExclusive`; week/month edges use `incompleteEdge`. Chart rendering consumes these flags in `Sources/TokensMenuBarCore/CostChartView.swift`.
