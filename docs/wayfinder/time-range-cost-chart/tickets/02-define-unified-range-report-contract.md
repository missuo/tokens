---
title: Define canonical time-range and granularity rules
tracker: local-markdown
kind: wayfinder-ticket
status: closed
assignee: claude
parent: ../map.md
labels:
  - wayfinder:prototype
blocked_by: []
prototype_asset: ../prototypes/time-range-logic/README.md
resolution_comment: ../resolutions/02-define-unified-range-report-contract.md
---

## Question

What precise rules resolve Today, 7D, 30D, All, and Custom into one canonical inclusive time range and then choose hour, day, natural-week, or natural-month chart buckets, including the treatment of incomplete edge chart buckets?

## Resolution

Decided in the linked resolution file; implementation landed in PR #8 (CLI v3 report/snapshot + Menu Bar DateRangePicker/CostChartView/UsageStore).
