---
title: Lock the cross-layer acceptance scenarios
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
  - 03-research-native-date-range-controls.md
  - 04-handle-unreliable-hour-timestamps.md
  - 05-finalize-loading-empty-and-error-states.md
  - 07-identify-incomplete-and-active-buckets.md
resolution_comment: null
---

## Question

Which end-to-end scenarios and invariants must the implementation satisfy to prove that presets and Custom ranges, reporting-timezone boundaries, automatic chart-bucket granularity, incomplete edge chart buckets, the active bucket, cache reuse, totals, and Cost chart values remain consistent?

## Resolution

Acceptance is covered by the Rust plan/build tests in `cli/tokens-cli/src/commands/usage_report_v3.rs`, the CLI integration tests in `cli/tokens-cli/tests/usage_v3_cli.rs`, and the Swift tests `ReportV3Tests`, `DateRangePickerTests`, `CostChartTests`, `UsageStoreTests`, and `UsageServiceTests`. Implemented in PR #8.
